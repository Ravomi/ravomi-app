//! Native input adapter. Bevy keeps its GPU surface; only text input is a WebView.
use super::*;
use bevy::winit::{WINIT_WINDOWS, WinitUserEvent};
use project_graph_bevy_canvas::edit_protocol::{self, Input};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use wry::{
    Rect, WebView, WebViewBuilder,
    dpi::{LogicalPosition, LogicalSize},
};

const SCRIPT: &str = include_str!("../app/dist-editor/editor.js");
const STYLE: &str = include_str!("../app/src/canvas-editor.css");

pub fn initialize() -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        use gtk::prelude::DisplayExtManual;
        gtk::init().map_err(|e| format!("React 编辑层无法初始化：{e}"))?;
        if gtk::gdk::Display::default().is_none_or(|d| d.backend().is_wayland()) {
            return Err(
                "同窗口 React 编辑暂不支持原生 Wayland，请使用 app/run-canvas.sh 的 XWayland 入口"
                    .into(),
            );
        }
        // Wry's wgpu example installs this hook: WebKit can destroy an already
        // released GLX window on the shared X connection. Do not attribute that
        // asynchronous error to the next unrelated winit request (e.g. title).
        winit::platform::x11::register_xlib_error_hook(Box::new(|_, error| {
            // SAFETY: winit invokes the hook with a live Xlib XErrorEvent.
            let error = unsafe { &*(error as *const x11_dl::xlib::XErrorEvent) };
            let handled = error.error_code == 170 && error.minor_code == 32;
            if handled {
                warn!("WebKit reported GLXBadWindow while releasing its child surface");
            }
            handled
        }));
    }
    Ok(())
}

#[derive(Default)]
struct NativeEditor {
    view: Option<WebView>,
    context: Option<wry::WebContext>,
    messages: Option<mpsc::Receiver<String>>,
    overflow: Arc<AtomicBool>,
    ready: bool,
    serial: Option<u64>,
    last_state: String,
    bounds: Option<Rect>,
    visible: bool,
}

pub fn install(app: &mut App) {
    app.insert_non_send(NativeEditor::default())
        .add_systems(EguiPrimaryContextPass, update.after(ui));
}

#[allow(clippy::too_many_arguments)]
fn update(
    mut native: NonSendMut<NativeEditor>,
    mut editor: ResMut<Editor>,
    mut scene: ResMut<Scene>,
    window: Single<(Entity, &Window), With<PrimaryWindow>>,
    camera: Single<(&Transform, &Projection), With<Camera2d>>,
    proxy: Res<bevy::winit::EventLoopProxyWrapper>,
) {
    #[cfg(target_os = "linux")]
    for _ in 0..64 {
        if !gtk::events_pending() {
            break;
        }
        gtk::main_iteration_do(false);
    }
    if let Err(error) = synchronize(
        &mut native,
        &mut editor,
        &mut scene.0,
        *window,
        *camera,
        &proxy,
    ) {
        editor.report_error(format!("React 编辑层：{error}"));
        // Keep the draft and expose the existing editor after a host failure.
        editor.web_input = false;
        if let Some(edit) = editor.active.as_mut() {
            edit.composing = false;
            edit.focus = true;
        }
        native.view = None;
        native.visible = false;
    }
}

fn synchronize(
    native: &mut NativeEditor,
    editor: &mut Editor,
    doc: &mut Document,
    (window_entity, window): (Entity, &Window),
    (camera, projection): (&Transform, &Projection),
    proxy: &bevy::winit::EventLoopProxyWrapper,
) -> Result<()> {
    if !editor.web_input {
        return Ok(());
    }
    let Projection::Orthographic(projection) = projection else {
        return Ok(());
    };
    let scale = 1.0 / projection.scale;
    if native.overflow.swap(false, Ordering::Relaxed) {
        return Err("编辑消息队列已满，请重新进入编辑".into());
    }
    let incoming: Vec<_> = native
        .messages
        .as_ref()
        .map(|rx| rx.try_iter().take(64).collect())
        .unwrap_or_default();
    for message in incoming {
        let message = edit_protocol::decode(&message)?;
        if matches!(message, Input::Ready) {
            native.ready = true;
            native.last_state.clear();
            continue;
        }
        let Some(edit) = editor.active.as_mut().filter(|e| !e.edge) else {
            continue;
        };
        let (serial, text, dimensions, composing, commit, cancel) = match message {
            Input::Draft {
                session,
                text,
                composing,
                width,
                height,
            } => (
                session,
                Some(text),
                Some([width, height]),
                composing,
                false,
                false,
            ),
            Input::Commit {
                session,
                text,
                width,
                height,
            } => (
                session,
                Some(text),
                Some([width, height]),
                false,
                true,
                false,
            ),
            Input::Cancel { session } => (session, None, None, false, false, true),
            Input::Ready => continue,
        };
        if serial != editor.serial {
            continue;
        }
        // A composition must end with a Draft(false) before any commit is accepted.
        if commit && edit.composing {
            continue;
        }
        let Some(node) = doc.nodes.iter().find(|n| n.id == edit.id) else {
            continue;
        };
        if let Some(text) = text {
            edit.draft = text;
        }
        edit.composing = composing;
        if let Some([width, height]) = dimensions {
            let inset = 2.0 * padding(node);
            if node.kind != "Section" {
                edit.size = [
                    if node.auto_size {
                        (width / scale + inset).max(1.0)
                    } else {
                        node.rect[2]
                    },
                    (height / scale + inset).max(48.0),
                ];
            }
        }
        if cancel {
            editor.active = None;
        } else if commit {
            let accepted = editor.commit(doc);
            if let Some(view) = &native.view {
                let result = serde_json::json!({"session": serial, "accepted": accepted, "revision": doc.revision});
                view.evaluate_script(&format!(
                    "window.dispatchEvent(new CustomEvent('hm:edit-result',{{detail:{result}}}))"
                ))?;
            }
        }
    }
    let Some(edit) = editor.active.as_ref().filter(|e| !e.edge) else {
        if native.visible {
            if let Some(view) = &native.view {
                view.set_visible(false)?;
                view.focus_parent()?;
            }
            native.visible = false;
        }
        native.serial = None;
        native.last_state.clear();
        return Ok(());
    };
    let Some(node) = doc.nodes.iter().find(|n| n.id == edit.id) else {
        return Ok(());
    };
    if native.view.is_none() {
        let (tx, rx) = mpsc::sync_channel(64);
        let overflow = native.overflow.clone();
        let proxy = std::ops::Deref::deref(proxy).clone();
        let context = native.context.get_or_insert_with(|| {
            let mut context = wry::WebContext::new(None);
            context.set_allows_automation(
                std::env::var("TAURI_WEBVIEW_AUTOMATION").as_deref() == Ok("true"),
            );
            context
        });
        let html = format!(
            "<!doctype html><html><head><meta charset=\"utf-8\"><meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'\"><style>{STYLE}</style></head><body><div id=\"root\"></div><script>{SCRIPT}</script></body></html>"
        );
        let view = WINIT_WINDOWS.with(|windows| -> Result<WebView> {
            let windows = windows.borrow();
            let parent = windows
                .get_window(window_entity)
                .ok_or("画布窗口尚未创建")?;
            Ok(WebViewBuilder::new_with_web_context(context)
                // X11 child windows do not reliably composite transparent WebKit surfaces.
                .with_transparent(false)
                .with_visible(false)
                .with_html(html)
                .with_navigation_handler(|url| url == "about:blank")
                .with_ipc_handler(move |request| {
                    let message = request.into_body();
                    if message.len() > 2 * 1024 * 1024 || tx.try_send(message).is_err() {
                        overflow.store(true, Ordering::Relaxed);
                    }
                    // Event loop closure during shutdown does not require a UI error.
                    let _ = proxy.send_event(WinitUserEvent::WakeUp);
                })
                .build_as_child(&**parent)?)
        })?;
        native.view = Some(view);
        native.messages = Some(rx);
    }
    let view = native.view.as_ref().ok_or("编辑窗口不可用")?;
    let content_width = ((edit.size[0] - 2.0 * padding(node)) * scale).max(1.0);
    let content_height = ((edit.size[1] - 2.0 * padding(node)) * scale).max(1.0);
    let position = LogicalPosition::new(
        f64::from(
            window.width() * 0.5 + (node.rect[0] + padding(node) - camera.translation.x) * scale,
        ),
        f64::from(
            window.height() * 0.5 + (node.rect[1] + padding(node) + camera.translation.y) * scale,
        ),
    );
    let bounds = Rect {
        position: position.into(),
        size: LogicalSize::new(
            f64::from(content_width.min(16384.0)),
            f64::from(content_height.min(16384.0)),
        )
        .into(),
    };
    if native.bounds != Some(bounds) {
        view.set_bounds(bounds)?;
        native.bounds = Some(bounds);
    }
    if native.ready {
        let new_session = native.serial != Some(editor.serial);
        let palette = theme::palette(editor.light);
        let background = if node.color[3] > 0.0 {
            color(node.color).with_alpha(1.0)
        } else {
            palette.surface
        }
        .to_srgba()
        .to_u8_array();
        let light_text_background =
            node.color[0] * 0.2126 + node.color[1] * 0.7152 + node.color[2] * 0.0722 > 0.6;
        let state = serde_json::json!({
            "session": editor.serial,
            "text": if new_session { edit.draft.as_str() } else { "" },
            "fontSize": (if node.kind == "Section" {30.0} else {node.font_size}) * scale,
            "contentWidth": content_width,
            "autoSize": node.kind != "Section" && node.auto_size,
            "light": if node.color[3] > 0.6 { light_text_background } else { editor.light },
            "background": format!("rgb({} {} {})", background[0], background[1], background[2]),
        })
        .to_string();
        if state != native.last_state || new_session {
            view.evaluate_script(&format!("window.setCanvasEdit({state})"))?;
            native.last_state = state;
        }
        native.serial = Some(editor.serial);
        if !native.visible {
            view.set_visible(true)?;
            view.focus()?;
            native.visible = true;
            if let Some(edit) = editor.active.as_mut() {
                edit.focus = false;
            }
        }
    }
    Ok(())
}
