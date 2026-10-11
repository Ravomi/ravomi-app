use super::*;
use bevy_egui::{EguiContexts, egui};
use std::sync::{Mutex, mpsc};

#[cfg(feature = "react-editor")]
#[path = "react_editor.rs"]
pub(super) mod react_editor;

fn layout_text(
    ctx: &egui::Context,
    text: &str,
    size: f32,
    width: f32,
    color: egui::Color32,
) -> std::sync::Arc<egui::Galley> {
    let mut job =
        egui::text::LayoutJob::simple(text.into(), egui::FontId::proportional(size), color, width);
    job.halign = egui::Align::Center;
    job.wrap.break_anywhere = width.is_finite();
    for section in &mut job.sections {
        section.format.line_height = Some(size * 1.5);
    }
    ctx.fonts_mut(|f| f.layout_job(job))
}

fn edit_shortcut(input: &mut egui::InputState, composing: bool, focused: bool) -> (bool, bool) {
    if composing || !focused {
        return (false, false);
    }
    // consume_key permits extra Shift/Alt modifiers; reserve Shift+Enter for
    // the text widget instead of consuming it as a completion shortcut.
    let enter = input.events.iter().any(|event| {
        matches!(event,
        egui::Event::Key { key: egui::Key::Enter, pressed: true, modifiers, .. }
            if !modifiers.shift && !modifiers.alt)
    }) && (input.consume_key(egui::Modifiers::COMMAND, egui::Key::Enter)
        || input.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
    let finish = enter || input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)
        || input.events.iter().any(|event| matches!(event, egui::Event::MouseWheel { delta, .. } if *delta != egui::Vec2::ZERO));
    (finish, enter)
}

#[allow(deprecated)]
pub(super) fn composition_frame(composing: &mut bool, events: &[egui::Event]) -> bool {
    let mut active = *composing;
    for event in events {
        if let egui::Event::Ime(event) = event {
            match event {
                egui::ImeEvent::Preedit { text, .. } => {
                    active |= *composing || !text.is_empty();
                    *composing = !text.is_empty();
                }
                egui::ImeEvent::Commit(text) => {
                    // Wayland can send empty housekeeping events with no composition.
                    active |= *composing || !text.is_empty();
                    *composing = false;
                }
                egui::ImeEvent::Disabled => *composing = false,
                _ => {}
            }
        }
    }
    active
}

fn inline_text_edit(
    ui: &mut egui::Ui,
    draft: &mut String,
    id: egui::Id,
    font_size: f32,
    content_width: f32,
    auto_size: bool,
) -> egui::text_edit::TextEditOutput {
    let mut layouter = |ui: &egui::Ui, text: &dyn egui::TextBuffer, _width| {
        layout_text(
            ui.ctx(),
            text.as_str(),
            font_size,
            if auto_size {
                f32::INFINITY
            } else {
                content_width
            },
            ui.visuals().text_color(),
        )
    };
    egui::TextEdit::multiline(draft)
        .id(id)
        .font(egui::FontId::proportional(font_size))
        .layouter(&mut layouter)
        .frame(egui::Frame::NONE)
        .background_color(egui::Color32::TRANSPARENT)
        .margin(0.0)
        .desired_width(content_width)
        .horizontal_align(egui::Align::Center)
        .desired_rows(1)
        .return_key(Some(egui::KeyboardShortcut::new(
            egui::Modifiers::SHIFT,
            egui::Key::Enter,
        )))
        .show(ui)
}

#[cfg(test)]
mod inline_edit_tests {
    use super::*;

    #[test]
    fn rounded_node_preserves_edit_preview_and_restores_after_editing() {
        let mut doc = stress::synthetic(1, false, false, false).unwrap();
        doc.nodes[0].rect = [0.0, 0.0, 100.0, 80.0];
        let original_rect = doc.nodes[0].rect;
        let original_text = doc.nodes[0].text.clone();
        let revision = doc.revision;
        let dirty = doc.dirty();
        let mut editor = Editor::default();
        editor.begin(&doc.nodes[0]);
        editor.active.as_mut().unwrap().size = [360.0, 100.0];
        let mut images = Assets::<Image>::default();
        let mut app = App::new();
        app.insert_resource(Scene(doc))
            .insert_resource(editor)
            .insert_resource(sections::Display::new(true))
            .insert_resource(NodeAssets {
                font: default(),
                corners: theme::NodeCorners::new(&mut images),
                textures: default(),
            })
            .add_systems(Update, (sync_labels, sections::sync_corners).chain());
        let entity = app
            .world_mut()
            .spawn((
                NodePart {
                    index: 0,
                    border: None,
                },
                Sprite::default(),
                Transform::default(),
            ))
            .id();
        let camera = app
            .world_mut()
            .spawn((
                Camera2d,
                Projection::Orthographic(OrthographicProjection::default_2d()),
            ))
            .id();
        for scale in [0.25, 1.0, 4.0] {
            let mut projection = app.world_mut().get_mut::<Projection>(camera).unwrap();
            let Projection::Orthographic(projection) = &mut *projection else {
                unreachable!()
            };
            projection.scale = scale;
            app.update();
            let size = Vec2::new(360.0, 100.0);
            let radius = theme::corners::group_radius(size, scale);
            assert_eq!(
                app.world().get::<Sprite>(entity).unwrap().custom_size,
                Some(size + Vec2::splat(2.0 * radius / theme::corners::MAX_RADIUS as f32)),
            );
            assert_eq!(
                app.world()
                    .get::<Transform>(entity)
                    .unwrap()
                    .translation
                    .truncate(),
                Vec2::new(130.0, -10.0)
            );
        }
        app.world_mut().resource_mut::<Editor>().active = None;
        app.update();
        assert!(app.world().get::<NodePreviewSize>(entity).is_none());
        assert_eq!(
            app.world()
                .get::<Transform>(entity)
                .unwrap()
                .translation
                .truncate(),
            Vec2::ZERO
        );
        let doc = &app.world().resource::<Scene>().0;
        assert_eq!(doc.nodes[0].rect, original_rect);
        assert_eq!(doc.nodes[0].text, original_text);
        assert_eq!(doc.revision, revision);
        assert_eq!(doc.dirty(), dirty);
    }

    #[test]
    fn idle_ime_notifications_do_not_block_completion() {
        let mut composing = false;
        assert!(!composition_frame(
            &mut composing,
            &[egui::Event::Ime(egui::ImeEvent::Commit(String::new()))]
        ));
        assert!(!composing);
        assert!(composition_frame(
            &mut composing,
            &[egui::Event::Ime(egui::ImeEvent::Commit("你好".into()))]
        ));
        assert!(!composition_frame(&mut composing, &[]));
    }

    #[test]
    fn inline_editor_keeps_short_text_centered() {
        let ctx = egui::Context::default();
        let mut draft = "Node 0".to_owned();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let output =
                inline_text_edit(ui, &mut draft, egui::Id::new("test"), 24.0, 240.0, false);
            let center = output.galley_pos.x + output.galley.size().x * 0.5;
            assert!((center - output.response.rect.center().x).abs() < 1.0);
        });
        output.textures_delta.clear();
    }

    #[test]
    fn exit_shortcuts_preserve_shift_enter_and_ime() {
        for (key, modifiers, composing, focused, expected) in [
            (
                egui::Key::Enter,
                egui::Modifiers::NONE,
                false,
                true,
                (true, true),
            ),
            (
                egui::Key::Enter,
                egui::Modifiers::COMMAND,
                false,
                true,
                (true, true),
            ),
            (
                egui::Key::Enter,
                egui::Modifiers::SHIFT,
                false,
                true,
                (false, false),
            ),
            (
                egui::Key::Escape,
                egui::Modifiers::NONE,
                false,
                true,
                (true, false),
            ),
            (
                egui::Key::Enter,
                egui::Modifiers::NONE,
                true,
                true,
                (false, false),
            ),
            (
                egui::Key::Escape,
                egui::Modifiers::NONE,
                true,
                true,
                (false, false),
            ),
            (
                egui::Key::Enter,
                egui::Modifiers::NONE,
                false,
                false,
                (false, false),
            ),
        ] {
            let ctx = egui::Context::default();
            let input = egui::RawInput {
                events: vec![egui::Event::Key {
                    key,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers,
                }],
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| {
                let ctx = ui.ctx();
                assert_eq!(
                    ctx.input_mut(|i| edit_shortcut(i, composing, focused)),
                    expected,
                    "{key:?}, {modifiers:?}, composing={composing}, focused={focused}"
                );
                if !expected.0 {
                    assert!(ctx.input(|i| i.key_pressed(key)));
                }
            });
            output.textures_delta.clear();
        }
    }

    #[test]
    fn wheel_finishes_editing_but_not_during_composition() {
        for composing in [false, true] {
            let ctx = egui::Context::default();
            let input = egui::RawInput {
                events: vec![egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0.0, 8.0),
                    phase: egui::TouchPhase::Move,
                    modifiers: egui::Modifiers::NONE,
                }],
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| {
                let ctx = ui.ctx();
                assert_eq!(
                    ctx.input_mut(|i| edit_shortcut(i, composing, true)),
                    (!composing, false)
                );
            });
            output.textures_delta.clear();
        }
    }
}

pub struct EditSession {
    edge: bool,
    id: String,
    draft: String,
    size: [f32; 2],
    focus: bool,
    composing: bool,
}

#[derive(Resource, Default)]
pub struct Editor {
    pub(super) web_input: bool,
    pub(super) export: super::export_ui::Export,
    pub(super) recovery: super::recovery_ui::Recovery,
    pub overview: section_overview::Overview,
    pub tools: canvas_tools::Tools,
    pub clipboard: project_graph_bevy_canvas::document::GraphClipboard,
    pub(super) clipboard_copy_text: Option<String>,
    pub active: Option<EditSession>,
    pub capture: bool,
    gesture: bool,
    initialized: bool,
    serial: u64,
    pub light: bool,
    pub ui_scale: f32,
    pub connect_mode: bool,
    help: bool,
    pub(super) closing: bool,
    error: Option<String>,
    dialog: Option<Mutex<mpsc::Receiver<Option<PathBuf>>>>,
    pub(super) importing: Option<image_import::Pending>,
    opening_urls: Option<url_open::Pending>,
    icons: Vec<egui::TextureHandle>,
    window_action: Option<WindowAction>,
    menu: dock_menu::Dropdown,
    home: home::Home,
    pub(super) about: bool,
    pub dock_space: Option<(f32, f32)>,
}

enum WindowAction {
    Drag,
    Minimize,
    ToggleMaximize,
    Fullscreen,
    Pin,
}

pub fn set_brand_window_icon(
    windows: Query<Entity, With<PrimaryWindow>>,
    mut editor: ResMut<Editor>,
    mut applied: Local<bool>,
    _main_thread: bevy::ecs::system::NonSendMarker,
) {
    if *applied {
        return;
    }
    let Ok(entity) = windows.single() else { return };
    bevy::winit::WINIT_WINDOWS.with_borrow(|windows| {
        let Some(window) = windows.get_window(entity) else {
            return;
        };
        *applied = true;
        let result =
            image::load_from_memory(include_bytes!("../assets/branding/app-icons/128x128.png"))
                .map_err(|error| error.to_string())
                .and_then(|image| {
                    let rgba = image.to_rgba8();
                    let (width, height) = rgba.dimensions();
                    winit::window::Icon::from_rgba(rgba.into_raw(), width, height)
                        .map_err(|error| error.to_string())
                });
        match result {
            Ok(icon) => window.set_window_icon(Some(icon)),
            Err(error) => editor.error = Some(format!("窗口图标加载失败：{error}")),
        }
    });
}

pub fn apply_window_action(world: &mut World) {
    let Some(action) = world.resource_mut::<Editor>().window_action.take() else {
        return;
    };
    let Ok((entity, mut window)) = world
        .query_filtered::<(Entity, &mut Window), With<PrimaryWindow>>()
        .single_mut(world)
    else {
        return;
    };
    match action {
        WindowAction::Drag => {
            let result = bevy::winit::WINIT_WINDOWS.with_borrow(|windows| {
                windows
                    .get_window(entity)
                    .map(|window| window.drag_window())
            });
            if let Some(Err(error)) = result {
                world
                    .resource_mut::<Editor>()
                    .report_error(format!("无法拖动窗口：{error}"));
            }
        }
        WindowAction::Fullscreen => {
            window.mode = if window.mode == bevy::window::WindowMode::Windowed {
                bevy::window::WindowMode::BorderlessFullscreen(
                    bevy::window::MonitorSelection::Current,
                )
            } else {
                bevy::window::WindowMode::Windowed
            };
        }
        WindowAction::Pin => {
            window.window_level = if window.window_level == bevy::window::WindowLevel::AlwaysOnTop {
                bevy::window::WindowLevel::Normal
            } else {
                bevy::window::WindowLevel::AlwaysOnTop
            };
        }
        WindowAction::Minimize => window.set_minimized(true),
        WindowAction::ToggleMaximize => {
            let maximized = bevy::winit::WINIT_WINDOWS.with_borrow(|windows| {
                windows
                    .get_window(entity)
                    .is_some_and(|window| window.is_maximized())
            });
            window.set_maximized(!maximized);
        }
    }
}

fn load_dock_icons(ctx: &egui::Context) -> Vec<egui::TextureHandle> {
    [
        include_bytes!("../assets/icons/save.svg").as_slice(),
        include_bytes!("../assets/icons/undo.svg"),
        include_bytes!("../assets/icons/redo.svg"),
        include_bytes!("../assets/icons/scan.svg"),
        include_bytes!("../assets/icons/link.svg"),
        include_bytes!("../assets/icons/minus.svg"),
        include_bytes!("../assets/icons/maximize.svg"),
        include_bytes!("../assets/icons/x.svg"),
        include_bytes!("../assets/icons/file.svg"),
        include_bytes!("../assets/icons/settings.svg"),
        include_bytes!("../assets/icons/circle-help.svg"),
        include_bytes!("../assets/icons/moon.svg"),
        include_bytes!("../assets/icons/app-window.svg"),
        include_bytes!("../assets/icons/blocks.svg"),
        include_bytes!("../assets/icons/view.svg"),
        include_bytes!("../assets/icons/axe.svg"),
        include_bytes!("../assets/icons/folder-open.svg"),
        include_bytes!("../assets/icons/clock.svg"),
        include_bytes!("../assets/icons/plus.svg"),
        include_bytes!("../assets/icons/download.svg"),
        include_bytes!("../assets/icons/upload.svg"),
        include_bytes!("../assets/icons/book-open.svg"),
        include_bytes!("../assets/icons/search.svg"),
        include_bytes!("../assets/icons/pin.svg"),
        include_bytes!("../assets/icons/blocks.svg"),
        include_bytes!("../assets/icons/info.svg"),
        include_bytes!("../assets/icons/globe.svg"),
        include_bytes!("../assets/icons/copy.svg"),
        include_bytes!("../assets/icons/clipboard.svg"),
        include_bytes!("../assets/icons/trash-2.svg"),
        include_bytes!("../assets/icons/type.svg"),
        include_bytes!("../assets/icons/palette.svg"),
        include_bytes!("../assets/icons/scissors.svg"),
    ]
    .iter()
    .enumerate()
    .map(|(index, bytes)| {
        let tree = resvg::usvg::Tree::from_data(bytes, &resvg::usvg::Options::default())
            .expect("bundled Lucide icon");
        let size = 48;
        let mut pixels = resvg::tiny_skia::Pixmap::new(size, size).expect("icon dimensions");
        resvg::render(
            &tree,
            resvg::tiny_skia::Transform::from_scale(
                size as f32 / tree.size().width(),
                size as f32 / tree.size().height(),
            ),
            &mut pixels.as_mut(),
        );
        let mut rgba = pixels.take();
        for pixel in rgba.as_chunks_mut::<4>().0 {
            pixel[..3].fill(255);
        }
        ctx.load_texture(
            format!("dock-{index}"),
            egui::ColorImage::from_rgba_unmultiplied([size as usize; 2], &rgba),
            egui::TextureOptions::LINEAR,
        )
    })
    .collect()
}

fn about_content(ui: &mut egui::Ui, light: bool) {
    let palette = theme::chrome_palette(light);
    ui.spacing_mut().item_spacing.y = 16.0;
    ui.horizontal_wrapped(|ui| {
        ui.label(egui::RichText::new("Ravomi").size(30.0).strong());
        if ui
            .add(
                egui::Button::new(
                    egui::RichText::new(concat!("v", env!("CARGO_PKG_VERSION"))).size(18.0),
                )
                .fill(palette.surface)
                .stroke(egui::Stroke::new(1.0, palette.border))
                .corner_radius(8),
            )
            .on_hover_text("查看版本记录")
            .clicked()
        {
            ui.ctx().open_url(egui::OpenUrl::new_tab(
                "https://github.com/Ravomi/ravomi-app/releases",
            ));
        }
    });
    ui.colored_label(palette.muted, "Ravomi · 无限画布");
    ui.heading("人和 Agent，共绘工作。");
    ui.add(egui::Label::new("以无限画布为工作空间的多 Agent 协作平台。将代码、素材、任务与子代理组织为可连接的模块，让工作过程可见、任务关系清晰、执行过程可控。").wrap());
    ui.add(egui::Label::new("当前为画布编辑原型；Agent 编排与执行仍在开发规划中。").wrap());
    let card = |ui: &mut egui::Ui, community: bool| {
        egui::Frame::new()
            .stroke(egui::Stroke::new(1.0, palette.border))
            .corner_radius(8)
            .inner_margin(16)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.set_min_height(88.0);
                ui.spacing_mut().item_spacing.y = 8.0;
                ui.colored_label(
                    palette.muted,
                    egui::RichText::new(if community {
                        "社区与贡献"
                    } else {
                        "项目仓库"
                    })
                    .size(12.0),
                );
                if community {
                    ui.label("感谢参与项目及上游开发的贡献者");
                    ui.hyperlink_to(
                        "贡献者与致谢",
                        "https://github.com/Ravomi/ravomi-app/blob/main/CREDITS.md",
                    );
                } else {
                    ui.hyperlink_to("Ravomi", "https://github.com/Ravomi/ravomi-app");
                }
            });
    };
    if ui.available_width() >= 440.0 {
        ui.columns(2, |columns| {
            card(&mut columns[0], true);
            card(&mut columns[1], false);
        });
    } else {
        card(ui, true);
        card(ui, false);
    }
    ui.hyperlink_to(
        egui::RichText::new("开源许可 · GPL-3.0-only")
            .size(12.0)
            .color(palette.muted),
        "https://github.com/Ravomi/ravomi-app/blob/main/LICENSE",
    );
}

fn dock_button(editor: &Editor, ui: &egui::Ui, index: usize) -> egui::Button<'static> {
    egui::Button::image(
        egui::Image::new((editor.icons[index].id(), egui::vec2(18.0, 18.0)))
            .tint(ui.visuals().text_color()),
    )
    .frame(false)
    .min_size(if ui.ctx().content_rect().width() < 900.0 {
        egui::vec2(28.0, 32.0)
    } else {
        egui::vec2(32.0, theme::DOCK_ITEM)
    })
}
impl Editor {
    pub fn with_startup(open: bool, home: bool) -> Self {
        let mut editor = Self::default();
        editor.menu.open = open;
        editor.home.visible = home;
        editor
    }
    pub(super) fn tabs_blocked(&self) -> bool {
        self.recovery.open
            || self.export.open
            || self.dialog.is_some()
            || self.importing.is_some()
            || self.error.is_some()
            || self.active.as_ref().is_some_and(|e| e.composing)
    }
    pub fn is_editing(&self, node: &GraphNode) -> bool {
        self.active.as_ref().is_some_and(|edit| edit.id == node.id)
    }
    pub(super) fn canvas_resize_allowed(&self) -> bool {
        !self.tabs_blocked()
            && self.active.is_none()
            && !self.home.visible
            && !self.closing
            && !self.about
            && !self.menu.visible()
            && !self.tools.visible()
    }
    pub fn report_error(&mut self, error: String) {
        if self.error.is_none() {
            self.error = Some(error);
        }
    }
    pub(super) fn paste_graph(
        &mut self,
        doc: &mut Document,
        interaction: &mut Interaction,
        center: [f32; 2],
    ) {
        match self.clipboard.paste(doc, center) {
            Ok(selected) if !selected.is_empty() => {
                interaction.selected = selected;
                interaction.selected_edges.clear();
                interaction.dirty = true;
            }
            Ok(_) => self.report_error("剪贴板中没有可粘贴的节点、图片或 SVG 源码".into()),
            Err(error) => self.report_error(format!("粘贴失败：{error}")),
        }
    }
    pub fn hide_home(&mut self) {
        self.home.visible = false;
    }
    pub fn open_urls(&mut self, doc: &Document, selected: &HashSet<usize>) -> Result<()> {
        if self.opening_urls.is_some() {
            return Err("正在打开网址，请稍后重试".into());
        }
        self.opening_urls = Some(url_open::Pending::start(doc, selected)?);
        Ok(())
    }
    pub fn begin(&mut self, node: &GraphNode) {
        self.serial += 1;
        self.active = Some(EditSession {
            edge: false,
            id: node.id.clone(),
            draft: node.text.clone(),
            size: [node.rect[2], node.rect[3]],
            focus: true,
            composing: false,
        });
        self.capture = true;
    }
    pub fn begin_edge(&mut self, edge: &project_graph_bevy_canvas::GraphEdge) {
        self.serial += 1;
        self.active = Some(EditSession {
            edge: true,
            id: edge.id.clone(),
            draft: edge.text.clone(),
            size: [0.0; 2],
            focus: true,
            composing: false,
        });
        self.capture = true;
    }
    pub fn begin_new(&mut self, node: &GraphNode) {
        self.begin(node);
        if let Some(edit) = &mut self.active {
            edit.draft.clear();
        }
    }
    pub(super) fn commit(&mut self, doc: &mut Document) -> bool {
        if let Some(edit) = self.active.take() {
            if edit.edge {
                let result = doc
                    .edges
                    .iter()
                    .position(|e| e.id == edit.id)
                    .ok_or_else(|| "连线不存在".into())
                    .and_then(|index| doc.edit_edge(index, edit.draft.clone(), false));
                if let Err(error) = result {
                    self.report_error(format!("连线标题未保存：{error}"));
                    self.active = Some(edit);
                    return false;
                }
            } else {
                doc.edit_text(&edit.id, edit.draft, edit.size);
            }
        }
        true
    }
    fn save(
        &mut self,
        doc: &mut Document,
        physics: &mut physics::PhysicsEditor,
        copy: bool,
        ai: Option<&mut canvas_ai::State>,
    ) {
        if self.active.as_ref().is_some_and(|e| e.composing) {
            return;
        }
        physics.finish(doc);
        if !self.commit(doc) {
            return;
        }
        doc.finish();
        if let Some(ai) = ai
            && let Err(error) = ai.prepare_save(doc)
        {
            self.report_error(error);
            return;
        }
        if copy || doc.path.is_none() {
            if self.dialog.is_none() {
                let (tx, rx) = mpsc::channel();
                use project_graph_bevy_canvas::project;
                let name = doc
                    .path
                    .as_ref()
                    .map(|path| path.with_extension(project::EXTENSION))
                    .and_then(|path| {
                        path.file_name()
                            .map(|name| name.to_string_lossy().into_owned())
                    })
                    .unwrap_or_else(|| project::DEFAULT_FILE_NAME.into());
                std::thread::spawn(move || {
                    let path = rfd::FileDialog::new()
                        .add_filter(project::FILTER_NAME, &[project::EXTENSION])
                        .add_filter("兼容画布 (.prg)", &[project::LEGACY_EXTENSION])
                        .set_file_name(&name)
                        .save_file();
                    let _ = tx.send(path);
                });
                self.dialog = Some(Mutex::new(rx));
            }
        } else if let Some(path) = doc.path.clone()
            && let Err(e) = doc.save(&path)
        {
            self.error = Some(e.to_string());
        }
    }
}

pub fn close_request(
    mut requests: MessageReader<bevy::window::WindowCloseRequested>,
    mut editor: ResMut<Editor>,
    scene: Res<Scene>,
    tabs: Res<tabs::Tabs>,
    mut exit: MessageWriter<AppExit>,
) {
    if requests.read().next().is_some() {
        if scene.0.dirty() || editor.active.is_some() || tabs.dirty() {
            editor.closing = true;
        } else {
            exit.write(AppExit::Success);
        }
    }
}

pub fn draw_overview(
    mut contexts: EguiContexts,
    mut editor: ResMut<Editor>,
    scene: Res<Scene>,
    window: Single<&Window, With<PrimaryWindow>>,
    camera: Single<(&Transform, &Projection), With<Camera2d>>,
    config: Res<Config>,
) -> bevy::prelude::Result {
    if config.no_ui || editor.home.visible {
        editor.overview.clear();
        return Ok(());
    }
    let (transform, Projection::Orthographic(projection)) = *camera else {
        return Ok(());
    };
    let editing = editor.active.as_ref().map(|edit| edit.id.clone());
    let light = editor.light;
    editor.overview.draw(
        contexts.ctx_mut()?,
        &scene.0,
        &window,
        transform,
        projection,
        light,
        editing.as_deref(),
    );
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn ui(
    mut contexts: EguiContexts,
    mut editor: ResMut<Editor>,
    mut scene: ResMut<Scene>,
    mut interaction: ResMut<Interaction>,
    config: Res<Config>,
    mut camera: Single<(&mut Transform, &mut Projection), With<Camera2d>>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut exit: MessageWriter<AppExit>,
    mut physics: ResMut<physics::PhysicsEditor>,
    mut effects: ResMut<effects::Effects>,
    mut tabs: ResMut<tabs::Tabs>,
    mut ai: Option<ResMut<canvas_ai::State>>,
    mut assets: ResMut<NodeAssets>,
    mut images: ResMut<Assets<Image>>,
) -> bevy::prelude::Result {
    if config.no_ui {
        editor.capture = false;
        return Ok(());
    }
    let ctx = contexts.ctx_mut()?;
    if let Some(text) = editor.clipboard_copy_text.take() {
        ctx.copy_text(text);
    }
    if let Some(pending) = &editor.importing {
        let center = pending.center;
        let max_size = pending.max_size;
        let paste_graph = pending.paste_graph;
        if let Some(result) = pending.poll() {
            editor.importing = None;
            match result {
                Ok(Some(imported)) => {
                    let (width, height, rgba) = imported.pixels;
                    let factor = (max_size / width.max(height) as f32).min(1.0);
                    match scene.0.import_image(
                        center,
                        [width as f32 * factor, height as f32 * factor],
                        &imported.filename,
                        imported.bytes,
                    ) {
                        Ok(index) => {
                            let Some(name) = scene.0.nodes[index].attachment.clone() else {
                                editor.report_error("图片附件未关联，请撤销后重新导入".into());
                                return Ok(());
                            };
                            let image = Image::new(
                                Extent3d {
                                    width,
                                    height,
                                    depth_or_array_layers: 1,
                                },
                                TextureDimension::D2,
                                rgba,
                                TextureFormat::Rgba8UnormSrgb,
                                RenderAssetUsages::default(),
                            );
                            assets.textures.insert(name, images.add(image));
                            interaction.selected = HashSet::from([index]);
                            interaction.selected_edges.clear();
                            interaction.dirty = true;
                            editor.hide_home();
                        }
                        Err(error) => editor.report_error(format!("图片未导入：{error}")),
                    }
                }
                Ok(None) => {
                    if paste_graph {
                        editor.paste_graph(&mut scene.0, &mut interaction, center);
                    }
                }
                Err(error) => editor.report_error(format!("图片未导入：{error}")),
            }
        } else {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
    }
    if let Some(pending) = &editor.opening_urls {
        if let Some(result) = pending.poll() {
            editor.opening_urls = None;
            if let Err(error) = result {
                editor.report_error(error);
            }
        } else {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
    }
    if !editor.initialized {
        editor.light = config.light;
        ctx.set_zoom_factor(config.ui_scale.clamp(0.75, 2.0));
        theme::apply(ctx, editor.light);
        let mut fonts = egui::FontDefinitions::default();
        fonts.font_data.insert(
            "CJK".into(),
            egui::FontData::from_owned(config.font.clone()).into(),
        );
        fonts
            .families
            .entry(egui::FontFamily::Proportional)
            .or_default()
            .insert(0, "CJK".into());
        ctx.set_fonts(fonts);
        editor.icons = load_dock_icons(ctx);
        editor.menu.icons = editor.icons.clone();
        editor.home.icons = editor.icons.clone();
        editor.tools.icons = editor.icons.clone();
        // Actions mutate the document, so one UI pass must correspond to one input frame.
        ctx.options_mut(|o| o.max_passes = std::num::NonZeroUsize::new(1).unwrap());
        editor.initialized = true;
    }
    if cfg!(target_os = "macos") {
        // AppKit controls do not scale with egui. Reserve a fixed logical 36 px
        // strip so traffic lights never overlap tabs, including at 200% UI zoom.
        egui::Area::new("macos-window-chrome".into())
            .fixed_pos(egui::Pos2::ZERO)
            .movable(false)
            .show(ctx, |ui| {
                let (mut rect, _) = ui.allocate_exact_size(
                    egui::vec2(ctx.content_rect().width(), theme::window_chrome_height(ctx)),
                    egui::Sense::hover(),
                );
                ui.painter().rect_filled(
                    rect,
                    0.0,
                    theme::egui_color(theme::window_background(editor.light)),
                );
                rect.min.x += 88.0 / ctx.zoom_factor();
                let response =
                    ui.interact(rect, ui.id().with("drag"), egui::Sense::click_and_drag());
                if response.double_clicked() {
                    editor.window_action = Some(WindowAction::ToggleMaximize);
                } else if response.drag_started() {
                    editor.window_action = Some(WindowAction::Drag);
                }
            });
    }
    if let Some(dialog) = &editor.dialog {
        let result = dialog.lock().expect("dialog receiver").try_recv();
        match result {
            Ok(path) => {
                editor.dialog = None;
                if let Some(path) = path {
                    match project_graph_bevy_canvas::project::save_path(&path) {
                        Ok(path) if tabs.path_in_other_tab(&path) => {
                            editor.report_error(
                                "此文件已在其他标签页打开，请选择其他保存位置".into(),
                            );
                            tabs.close_after_save = false;
                        }
                        Ok(path) => {
                            let result = ai
                                .as_deref_mut()
                                .map(|ai| ai.prepare_save(&mut scene.0))
                                .transpose()
                                .and_then(|_| {
                                    scene.0.save(&path).map_err(|error| error.to_string())
                                });
                            if let Err(error) = result {
                                editor.report_error(error);
                                tabs.close_after_save = false;
                            }
                        }
                        Err(error) => {
                            editor.report_error(error.to_string());
                            tabs.close_after_save = false;
                        }
                    }
                } else {
                    editor.closing = false;
                    tabs.cancel_close();
                    tabs.cancel_exit();
                }
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                editor.dialog = None;
                editor.error = Some("文件对话框意外关闭，请重试".into());
            }
            Err(mpsc::TryRecvError::Empty) => {}
        }
    }
    let active_at_start = editor.active.is_some();
    let web_input = editor.web_input;
    let mut ime_frame = false;
    if let Some(edit) = &mut editor.active {
        ime_frame = if web_input && !edit.edge {
            edit.composing
        } else {
            ctx.input(|i| composition_frame(&mut edit.composing, &i.events))
        };
    }
    let (transform, projection) = &mut *camera;
    let Projection::Orthographic(projection) = &mut **projection else {
        return Ok(());
    };
    let mut save = false;
    let mut save_copy = false;
    if !config.preview
        && !config.benchmark
        && let Err(error) = editor.home.observe(&scene.0)
    {
        editor.report_error(error);
    }
    let light = editor.light;
    let (export, error) = editor.export.show(
        ctx,
        light,
        !interaction.selected.is_empty() || !interaction.selected_edges.is_empty(),
    );
    if let Some(error) = error {
        editor.report_error(error);
    }
    if export {
        physics.finish(&mut scene.0);
        let snapshot = if editor.export.selection {
            let edges = scene
                .0
                .edges
                .iter()
                .enumerate()
                .filter(|(_, edge)| interaction.selected_edges.contains(&edge.id))
                .map(|(index, _)| index)
                .collect();
            scene
                .0
                .capture_selection_snapshot(&interaction.selected, &edges)
        } else {
            scene.0.capture_snapshot()
        };
        match snapshot {
            Ok(snapshot) => editor.export.start(snapshot, config.font.clone(), light),
            Err(error) => editor.report_error(format!("无法导出图片：{error}")),
        }
    }
    if let Some(action) = editor.recovery.show(ctx, light, tabs.backup_policy()) {
        match action {
            super::recovery_ui::Action::Restore(path) => tabs.open_recovery(path),
            super::recovery_ui::Action::Configure(policy) => {
                if let Err(error) = tabs.configure_backups(policy) {
                    editor.report_error(format!("备份设置未保存：{error}"));
                }
            }
        }
    }
    let is_open = |path: &std::path::Path| {
        scene.0.path.as_deref() == Some(path) || tabs.path_in_other_tab(path)
    };
    if let Some(action) = editor.home.show(ctx, light, is_open) {
        match action {
            home::Action::New => match Document::blank() {
                Ok(doc) => {
                    tabs.open_document(doc);
                    editor.home.visible = false;
                }
                Err(error) => editor.report_error(format!("新建草稿失败：{error}")),
            },
            home::Action::Open => {
                tabs.open_file();
            }
            home::Action::Recent(path) => match Document::open(&path) {
                Ok(doc) => {
                    tabs.open_document(doc);
                    editor.home.visible = false;
                }
                Err(e) => editor.report_error(format!("打开文件失败：{e}")),
            },
            home::Action::Settings => editor.menu.select(2),
            home::Action::Help => editor.help = true,
            home::Action::About => editor.about = true,
            home::Action::Error(error) => editor.report_error(error),
            home::Action::RemoveRecent { path, delete_file } => {
                if delete_file && is_open(&path) {
                    editor.report_error("请先关闭此文件的标签页，再删除本机文件".into());
                } else if let Err(error) = editor.home.remove_recent(&path, delete_file) {
                    editor.report_error(error);
                }
            }
        }
    }
    let light = editor.light;
    let can_save = !editor.tabs_blocked();
    effects.light = light;
    editor.menu.can_undo = scene.0.history.can_undo();
    editor.menu.can_redo = scene.0.history.can_redo();
    if let Some(action) = editor.menu.show(
        ctx,
        light,
        can_save,
        can_save && !active_at_start,
        can_save && !active_at_start && !interaction.selected.is_empty(),
        physics.enabled,
    ) {
        match action {
            dock_menu::Action::ResetZoom => projection.scale = 1.0,
            dock_menu::Action::Origin => {
                transform.translation.x = 0.0;
                transform.translation.y = 0.0;
            }
            dock_menu::Action::Fullscreen => editor.window_action = Some(WindowAction::Fullscreen),
            dock_menu::Action::Maximize => {
                editor.window_action = Some(WindowAction::ToggleMaximize)
            }
            dock_menu::Action::Minimize => editor.window_action = Some(WindowAction::Minimize),
            dock_menu::Action::Pin => editor.window_action = Some(WindowAction::Pin),
            dock_menu::Action::Home => editor.home.visible = true,
            dock_menu::Action::New => match Document::blank() {
                Ok(doc) => {
                    tabs.open_document(doc);
                    editor.home.visible = false;
                }
                Err(error) => editor.report_error(format!("新建草稿失败：{error}")),
            },
            dock_menu::Action::Open => tabs.open_file(),
            dock_menu::Action::Recover => editor.recovery.refresh(),
            dock_menu::Action::Export => editor.export.open = true,
            dock_menu::Action::ImportImage | dock_menu::Action::PasteImage => {
                physics.finish(&mut scene.0);
                scene.0.selection = project_graph_bevy_canvas::document::Selection::capture(
                    &scene.0,
                    &interaction.selected,
                    &interaction.selected_edges,
                );
                interaction.moving.clear();
                interaction.linking = None;
                let center = [transform.translation.x, -transform.translation.y];
                let size = 512.0 * projection.scale;
                editor.importing = Some(if matches!(action, dock_menu::Action::PasteImage) {
                    image_import::Pending::clipboard(center, size)
                } else {
                    image_import::Pending::start(center, size, config.font.clone())
                });
            }
            dock_menu::Action::Save => save = true,
            dock_menu::Action::Undo | dock_menu::Action::Redo => {
                physics.finish(&mut scene.0);
                if matches!(action, dock_menu::Action::Undo) {
                    scene.0.undo();
                } else {
                    scene.0.redo();
                }
                interaction.moving.clear();
                interaction.dirty = true;
            }
            dock_menu::Action::Fit => {
                let (home, scale) =
                    fit_view(&scene.0, window.width(), window.height(), ctx.zoom_factor());
                transform.translation.x = home.x;
                transform.translation.y = home.y;
                projection.scale = scale;
                editor.home.visible = false;
            }
            dock_menu::Action::Connect => {
                editor.connect_mode = !editor.connect_mode;
                interaction.linking = None;
                interaction.moving.clear();
                physics.finish(&mut scene.0);
                editor.home.visible = false;
            }
            dock_menu::Action::About => editor.about = true,
            dock_menu::Action::SaveCopy => save_copy = true,
            dock_menu::Action::Canvas(command) => {
                let point = interaction
                    .last_cursor
                    .unwrap_or(transform.translation.truncate());
                if let Err(error) = canvas_tools::execute(
                    command,
                    point,
                    &mut scene.0,
                    &mut interaction,
                    &mut editor,
                    &mut physics,
                    &mut effects,
                ) {
                    editor.report_error(error.to_string());
                }
            }
            dock_menu::Action::SelectAll => {
                interaction.selected = (0..scene.0.nodes.len()).collect()
            }
            dock_menu::Action::Group => {
                create_group(&mut scene.0, &mut interaction, &mut editor, &mut physics)
            }
            dock_menu::Action::ToggleTheme => {
                editor.light = !editor.light;
                theme::apply(ctx, editor.light);
            }
            dock_menu::Action::Zoom(factor) => ctx.set_zoom_factor(factor),
            dock_menu::Action::TogglePhysics => physics.toggle(&mut scene.0),
            dock_menu::Action::Help => editor.help = true,
        }
    }
    let controls = egui::Area::new("window-controls".into())
        .anchor(egui::Align2::RIGHT_TOP, [-16.0, theme::dock_top(ctx)])
        .show(ctx, |ui| {
            theme::chrome_frame(editor.light).show(ui, |ui| {
                theme::chrome_style(ui, editor.light);
                let compact = ctx.content_rect().width() < 900.0;
                let minimal = ctx.content_rect().width() < 500.0;
                if compact {
                    ui.spacing_mut().button_padding.x = 4.0;
                    ui.spacing_mut().item_spacing.x = 2.0;
                }
                ui.horizontal(|ui| {
                    let enabled = !editor.tabs_blocked() && !editor.closing && !tabs.closing_tab();
                    let menu = ui
                        .add_enabled(
                            enabled,
                            egui::Button::new("⋯")
                                .frame(false)
                                .min_size(egui::vec2(28.0, 32.0))
                                .selected(editor.menu.open),
                        )
                        .on_hover_text("工作区菜单");
                    editor.menu.trigger = Some(menu.rect);
                    if menu.clicked() {
                        if editor.menu.open && editor.menu.section == 7 {
                            editor.menu.open = false;
                        } else {
                            editor.menu.select(7);
                        }
                    }
                    if !minimal
                        && ui
                            .add(dock_button(&editor, ui, 22))
                            .on_hover_text("搜索最近文件")
                            .clicked()
                    {
                        editor.home.visible = true;
                    }
                    if ui
                        .add(dock_button(&editor, ui, 11))
                        .on_hover_text("切换深色 / 浅色模式")
                        .clicked()
                    {
                        editor.light = !editor.light;
                        theme::apply(ctx, editor.light);
                    }
                    if !minimal
                        && ui
                            .add(dock_button(&editor, ui, 23).selected(
                                window.window_level == bevy::window::WindowLevel::AlwaysOnTop,
                            ))
                            .on_hover_text("窗口置顶")
                            .clicked()
                    {
                        editor.window_action = Some(WindowAction::Pin);
                    }
                    if !cfg!(target_os = "macos")
                        && !compact
                        && ui
                            .add(dock_button(&editor, ui, 5))
                            .on_hover_text("最小化")
                            .clicked()
                    {
                        editor.window_action = Some(WindowAction::Minimize);
                    }
                    if !cfg!(target_os = "macos")
                        && !compact
                        && ui
                            .add(dock_button(&editor, ui, 6))
                            .on_hover_text("最大化 / 还原")
                            .clicked()
                    {
                        editor.window_action = Some(WindowAction::ToggleMaximize);
                    }
                    if !cfg!(target_os = "macos")
                        && ui
                            .add(dock_button(&editor, ui, 7))
                            .on_hover_text("关闭窗口")
                            .clicked()
                    {
                        if scene.0.dirty() || editor.active.is_some() || tabs.dirty() {
                            editor.closing = true;
                        } else {
                            exit.write(AppExit::Success);
                        }
                    }
                });
            });
        });
    editor.dock_space = Some((ctx.content_rect().left(), controls.response.rect.left()));
    tabs.bar(ctx, &scene.0, &mut editor);
    if !editor.home.visible {
        egui::Area::new("status".into())
            .anchor(egui::Align2::LEFT_BOTTOM, [12.0, -12.0])
            .show(ctx, |ui| {
                ui.set_max_width((window.width() / ctx.zoom_factor() - 48.0).max(220.0));
                egui::Frame::new()
                    .fill(ui.visuals().panel_fill.gamma_multiply(0.92))
                    .corner_radius(12)
                    .inner_margin(8.0)
                    .show(ui, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(format!(
                                "{} 节点 · {} 连线 · 已选 {}",
                                scene.0.nodes.len(),
                                scene.0.edges.len(),
                                interaction.selected.len()
                            ));
                            ui.separator();
                            ui.label(format!("{:.0}%", 100.0 / projection.scale));
                            ui.separator();
                            ui.label(if scene.0.dirty() || active_at_start {
                                "有未保存的修改"
                            } else {
                                "已保存"
                            });
                            if editor.dialog.is_some() {
                                ui.spinner();
                                ui.label("请选择保存位置…");
                            }
                            editor.export.status_button(ui);
                            if let Some(hint) = interaction.jump.hint() {
                                ui.separator();
                                ui.label(hint);
                            }
                            if physics.enabled && physics.running {
                                ui.separator();
                                ui.label(if physics.running {
                                    format!(
                                        "物理 {:.2} ms · 活动 {} / 休眠 {}",
                                        physics.step_ms, physics.active, physics.sleeping
                                    )
                                } else {
                                    "物理已停稳".into()
                                });
                            }
                        });
                    });
            });
    }
    let mut commit = false;
    if editor.about {
        let width = (ctx.content_rect().width() - 64.0).clamp(200.0, 520.0);
        let height = (ctx.content_rect().height() - 112.0).max(80.0);
        let mut close = false;
        egui::Window::new("关于 Ravomi")
            .title_bar(false)
            .resizable(false)
            .frame(theme::tool_frame(editor.light))
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .fixed_size(egui::vec2(width, height.min(600.0)))
            .show(ctx, |ui| {
                theme::tool_style(ui, editor.light);
                ui.spacing_mut().scroll.fade.strength = 0.0;
                close = theme::tool_header(ui, "关于 Ravomi", editor.light);
                egui::ScrollArea::vertical()
                    .max_height(height - 48.0)
                    .show(ui, |ui| about_content(ui, editor.light));
            });
        if close || ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
            editor.about = false;
        }
    }
    let mut enter_commit = false;
    let light = editor.light;
    let clipboard_empty = editor.clipboard.is_empty();
    if let Some(action) = editor
        .tools
        .show(ctx, light, &scene.0, &interaction, clipboard_empty)
    {
        match action {
            canvas_tools::Action::Command(command, point) => {
                if let Err(error) = canvas_tools::execute(
                    command,
                    point,
                    &mut scene.0,
                    &mut interaction,
                    &mut editor,
                    &mut physics,
                    &mut effects,
                ) {
                    editor.report_error(error.to_string());
                }
            }
            canvas_tools::Action::Undo => {
                physics.finish(&mut scene.0);
                scene.0.undo();
                interaction.moving.clear();
                interaction.dirty = true;
            }
            canvas_tools::Action::Group => {
                create_group(&mut scene.0, &mut interaction, &mut editor, &mut physics);
            }
            canvas_tools::Action::Focus(index) => {
                physics.finish(&mut scene.0);
                match scene.0.reveal_node(index) {
                    Ok(_) => {
                        let node = &scene.0.nodes[index];
                        let position = center(node);
                        transform.translation.x = position.x;
                        transform.translation.y = position.y;
                        projection.scale = projection
                            .scale
                            .max(node.rect[2] / (window.width() * 0.7))
                            .max(node.rect[3] / (window.height() * 0.7));
                        interaction.selected = HashSet::from([index]);
                        interaction.selected_edges.clear();
                        interaction.dirty = true;
                    }
                    Err(error) => editor.report_error(error.to_string()),
                }
            }
            canvas_tools::Action::Apply(ids, properties) => {
                physics.finish(&mut scene.0);
                let selected = scene
                    .0
                    .nodes
                    .iter()
                    .enumerate()
                    .filter(|(_, n)| ids.contains(&n.id))
                    .map(|(i, _)| i)
                    .collect();
                if let Err(error) = scene.0.set_node_properties(&selected, &properties) {
                    editor.report_error(error.to_string());
                }
                interaction.dirty = true;
            }
        }
    }
    let mut cancel_edit = false;
    let serial = editor.serial;
    let web_input = editor.web_input;
    if let Some(edit) = &mut editor.active {
        if !web_input || edit.edge {
            (commit, enter_commit) = ctx.input_mut(|i| edit_shortcut(i, ime_frame, window.focused));
        }
        if edit.edge {
            egui::Window::new("连线标题")
                .collapsible(false)
                .resizable(true)
                .default_width(300.0)
                .show(ctx, |ui| {
                    let response =
                        ui.add(egui::TextEdit::multiline(&mut edit.draft).desired_rows(3));
                    if edit.focus {
                        response.request_focus();
                        edit.focus = false;
                    }
                    if ui
                        .add_enabled(!ime_frame, egui::Button::new("完成"))
                        .clicked()
                    {
                        commit = true;
                    }
                    if ui
                        .add_enabled(!ime_frame, egui::Button::new("取消"))
                        .clicked()
                    {
                        cancel_edit = true;
                    }
                    ui.label("Enter 完成 · Shift+Enter 换行");
                });
        } else if web_input {
            // The native WebView owns draft input, focus and IME composition.
            if window.focused && !ime_frame && !edit.focus {
                commit |= ctx.input(|input| input.pointer.primary_clicked());
            }
        } else if let Some(node) = scene.0.nodes.iter().find(|n| n.id == edit.id) {
            let zoom = ctx.zoom_factor();
            let screen = (center(node) - transform.translation.truncate()) / projection.scale;
            let size = Vec2::new(node.rect[2], node.rect[3]) / projection.scale / zoom;
            let position = egui::pos2(
                (window.width() * 0.5 + screen.x) / zoom - size.x * 0.5,
                (window.height() * 0.5 - screen.y) / zoom - size.y * 0.5,
            );
            let scale = 1.0 / projection.scale / zoom;
            let inset = padding(node) * scale;
            let content_width = (edit.size[0] * scale - 2.0 * inset).max(1.0);
            let section = node.kind == "Section";
            let font_size = if section { 30.0 } else { node.font_size } * scale;
            let first_frame = edit.focus;
            let result = egui::Area::new(egui::Id::new(("edit", serial)))
                .fixed_pos(position + egui::vec2(inset, inset))
                .constrain(false)
                .fade_in(false)
                .order(egui::Order::Foreground)
                .show(ctx, |ui| {
                    ui.set_width(content_width);
                    let output = inline_text_edit(
                        ui,
                        &mut edit.draft,
                        egui::Id::new(("text", serial)),
                        font_size,
                        content_width,
                        !section && node.auto_size,
                    );
                    if edit.focus {
                        output.response.request_focus();
                        edit.focus = false;
                    }
                    output.response
                });
            if section {
                edit.size = [node.rect[2], node.rect[3]];
            } else if edit.draft.is_empty() {
                edit.size = [
                    node.rect[2],
                    (node.font_size * 1.5 + 2.0 * padding(node)).max(48.0),
                ];
            } else if edit.draft != node.text {
                let layout = layout_text(
                    ctx,
                    &edit.draft,
                    node.font_size,
                    text_width(node).unwrap_or(f32::INFINITY),
                    egui::Color32::WHITE,
                );
                edit.size[1] = (layout.size().y + 2.0 * padding(node)).max(48.0);
                if node.auto_size {
                    edit.size[0] = (layout.size().x + 2.0 * padding(node)).max(1.0);
                }
            } else {
                edit.size = [node.rect[2], node.rect[3]];
            }
            let rect = egui::Rect::from_min_size(
                position,
                egui::vec2(edit.size[0] * scale, edit.size[1] * scale),
            );
            let painter = ctx.layer_painter(egui::LayerId::new(
                egui::Order::Foreground,
                egui::Id::new(("edit", serial)),
            ));
            let size = Vec2::from_array(edit.size);
            let outline = theme::corners::outline_with_radius(
                Vec2::ZERO,
                size,
                if node.collapsed {
                    theme::corners::radius(size) as f32
                } else {
                    theme::corners::group_radius(size, projection.scale)
                },
            );
            painter.add(egui::Shape::line(
                outline
                    .into_iter()
                    .map(|p| rect.center() + egui::vec2(p.x * scale, -p.y * scale))
                    .collect(),
                egui::Stroke::new(
                    1.0,
                    theme::egui_color(theme::palette(light).accent.with_alpha(150.0 / 255.0)),
                ),
            ));
            painter.text(
                rect.left_bottom() + egui::vec2(0.0, 6.0),
                egui::Align2::LEFT_TOP,
                "Enter 完成 · Shift+Enter 换行 · Esc 结束编辑",
                egui::FontId::proportional(12.0),
                theme::chrome_palette(light).muted,
            );
            if !ime_frame && window.focused && !first_frame {
                ctx.input(|i| {
                    if i.pointer.primary_clicked()
                        && let Some(p) = i.pointer.interact_pos()
                        && !rect.contains(p)
                    {
                        commit = true;
                    }
                });
                commit |= result.inner.lost_focus();
            }
        } else {
            commit = true;
        }
    }
    if cancel_edit {
        editor.active = None;
        commit = false;
    }
    if commit {
        let edited_id = editor.active.as_ref().map(|edit| edit.id.clone());
        editor.commit(&mut scene.0);
        if enter_commit
            && let Some(node) = scene
                .0
                .nodes
                .iter()
                .find(|node| Some(&node.id) == edited_id.as_ref())
        {
            effects.commit_edit(
                center(node),
                Vec2::new(node.rect[2], node.rect[3]),
                projection.scale > 0.25 && projection.scale < 10.0,
            );
        }
        interaction.dirty = true;
    }
    if !active_at_start
        && !editor.closing
        && !tabs.closing_tab()
        && !editor.tabs_blocked()
        && ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::S))
    {
        save = true;
    }
    if save || save_copy {
        editor.save(&mut scene.0, &mut physics, save_copy, ai.as_deref_mut());
        interaction.moving.clear();
        interaction.drag_start = None;
        interaction.selecting = false;
        interaction.dirty = true;
    }
    if editor.help {
        egui::Window::new("快捷键").open(&mut editor.help).show(ctx, |ui| {
            ui.label("Ctrl+C / Ctrl+X / Ctrl+V：复制 / 剪切 / 粘贴选中图对象（支持跨标签页）；编辑文字时使用系统文字剪贴板。");
            ui.label("Delete：删除选中节点及连线；F2：编辑选中节点或连线标题；Ctrl+R：反转选中连线。");
            ui.label("Ctrl+F：搜索当前图；Alt+Enter：节点属性；右键单击：操作菜单（拖动仍为连线或切割）。");
            ui.label("鼠标滚轮：以鼠标为中心缩放\n触控板：双指滑动平移，双指捏合缩放\nCtrl+双指滑动：缩放（也适用于 XWayland）\n空白处右键拖动：切割；中键 / 空格拖动：平移画布\n左键：点选、框选、移动；Shift：追加选择\n空白处左键双击：新建文字节点（分组内自动归组）\nCtrl+G：创建分组；拖动标题或边框移动整个分组\n节点上右键拖动：创建连线；也可用连线按钮或 E+左键\nEsc / 失焦 / 空白释放：取消连线\n双击文字节点：原位编辑；Enter：完成；Shift+Enter：换行\nEsc / 点击别处 / 滚轮：保留修改并结束编辑\nCtrl+A：全选；F：全图；Ctrl+Z / Ctrl+Shift+Z：撤销 / 重做\nCtrl+S：保存");
        });
    }
    if tabs.close_after_save
        && editor.dialog.is_none()
        && editor.importing.is_none()
        && editor.error.is_none()
        && !scene.0.dirty()
    {
        tabs.finish_close();
    }
    if (editor.closing || tabs.closing_tab()) && editor.error.is_none() {
        let quitting = editor.closing;
        let name = scene
            .0
            .path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "未命名画布".into());
        let mut proceed =
            quitting && !scene.0.dirty() && editor.active.is_none() && !editor.tabs_blocked();
        let mut save_current = false;
        egui::Modal::new(egui::Id::new("confirm-document-close"))
            .frame(theme::tool_frame(light))
            .show(ctx, |ui| {
                theme::tool_style(ui, light);
                ui.set_width(340.0);
                ui.label(
                    egui::RichText::new(if quitting {
                        "保存修改后退出？"
                    } else {
                        "保存修改后关闭标签页？"
                    })
                    .size(16.0)
                    .strong(),
                );
                ui.add_space(4.0);
                ui.label(format!("“{name}”有未保存的修改。"));
                if quitting {
                    ui.weak("将逐个处理其他未保存的画布。");
                }
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let available = !editor.tabs_blocked();
                        if ui
                            .add_enabled(available, theme::primary_button("保存", light))
                            .clicked()
                        {
                            save_current = true;
                        }
                        if ui
                            .add_enabled(available, egui::Button::new("放弃修改"))
                            .clicked()
                        {
                            if quitting {
                                tabs.discard_on_exit();
                                proceed = true;
                            } else {
                                tabs.finish_close();
                            }
                        }
                        if ui.button("取消").clicked() {
                            editor.closing = false;
                            tabs.cancel_close();
                            tabs.cancel_exit();
                            proceed = false;
                        }
                    });
                });
            });
        if save_current {
            editor.save(&mut scene.0, &mut physics, false, ai.as_deref_mut());
            if !quitting {
                tabs.close_after_save = editor.error.is_none();
            }
            proceed =
                quitting && !scene.0.dirty() && editor.dialog.is_none() && editor.error.is_none();
        }
        if proceed && tabs.advance_exit(&scene.0) {
            exit.write(AppExit::Success);
        }
    }
    if let Some(error) = editor.error.clone() {
        egui::Modal::new(egui::Id::new("document-error"))
            .frame(
                theme::dock_frame(light)
                    .corner_radius(12)
                    .inner_margin(16.0),
            )
            .show(ctx, |ui| {
                ui.heading("操作未完成");
                ui.label(error);
                if ui.button("知道了").clicked() {
                    editor.error = None;
                }
            });
    }
    if ctx.input(|i| i.pointer.any_pressed()) && ctx.is_pointer_over_egui() {
        editor.gesture = true;
    }
    editor.capture = active_at_start
        || editor.export.open
        || editor.recovery.open
        || editor.home.visible
        || editor.about
        || editor.menu.visible()
        || editor.tools.visible()
        || tabs.modal()
        || editor.active.is_some()
        || editor.gesture
        || ctx.egui_wants_pointer_input()
        || ctx.egui_wants_keyboard_input()
        || editor.closing
        || editor.error.is_some()
        || editor.dialog.is_some();
    editor.capture |= editor.importing.is_some();
    if !ctx.input(|i| i.pointer.any_down()) {
        editor.gesture = false;
    }
    editor.ui_scale = ctx.zoom_factor();
    Ok(())
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub fn fit_text_nodes(
    mut scene: ResMut<Scene>,
    assets: Res<NodeAssets>,
    editor: Res<Editor>,
    physics: Res<physics::PhysicsEditor>,
    mut interaction: ResMut<Interaction>,
    measured: Query<
        (Entity, &NodeLabel, &bevy::text::TextLayoutInfo),
        Changed<bevy::text::TextLayoutInfo>,
    >,
    mut labels: Query<&mut Transform, With<NodeLabel>>,
    mut parts: Query<(&NodePart, &mut Sprite), Without<NodeLabel>>,
    mut pending: Local<HashMap<Entity, (usize, Vec2)>>,
) {
    for (entity, label, info) in &measured {
        if info.size.is_finite() && info.size.y > 0.0 {
            pending.insert(entity, (label.0, info.size));
        }
    }
    // Layout is derived from fonts; defer it rather than add size changes to a drag transaction.
    if scene.0.in_transaction() || physics.running {
        return;
    }
    let mut resized = HashSet::new();
    // Generated records have a fixed preview viewport; full output lives in the AI inspector.
    // The AI system reports malformed binding errors through the editor error dialog.
    let ai_bindings = project_graph_bevy_canvas::collaboration::canvas::Bindings::read(&scene.0);
    pending.retain(|entity, (index, content)| {
        if !labels.contains(*entity) {
            return false;
        }
        let Some(node) = scene.0.nodes.get(*index) else {
            return false;
        };
        if node.kind != "TextNode" {
            return false;
        }
        if ai_bindings
            .as_ref()
            .is_ok_and(|bindings| bindings.is_log(&node.id))
        {
            return false;
        }
        if editor
            .active
            .as_ref()
            .is_some_and(|edit| edit.id == node.id)
        {
            return true;
        }
        let inset = 2.0 * padding(node);
        let size = [
            if node.auto_size {
                (content.x + inset).max(1.0)
            } else {
                node.rect[2]
            },
            (content.y + inset).max(48.0),
        ];
        if scene.0.apply_layout_size(*index, size) {
            if let Ok(mut transform) = labels.get_mut(*entity) {
                let node = &scene.0.nodes[*index];
                transform.translation.y = node.rect[3] * 0.5 - padding(node);
            }
            resized.insert(*index);
            interaction.dirty = true;
        }
        false
    });
    if resized.is_empty() {
        return;
    }
    for (part, mut sprite) in &mut parts {
        if !resized.contains(&part.index) {
            continue;
        }
        let node = &scene.0.nodes[part.index];
        let size = Vec2::new(node.rect[2], node.rect[3]);
        theme::resize(&mut sprite, &assets.corners, size, part.border.is_some());
    }
}

type LabelSyncState = (u64, Option<(String, [f32; 2])>, u64);

#[allow(clippy::type_complexity)] // Bevy query data includes transient render geometry.
pub fn sync_labels(
    mut commands: Commands,
    scene: Res<Scene>,
    assets: Res<NodeAssets>,
    editor: Res<Editor>,
    mut labels: Query<(
        &NodeLabel,
        &mut MsdfText,
        &mut Visibility,
        &mut TextBounds,
        &mut TextLayout,
        &mut Transform,
    )>,
    mut parts: Query<
        (
            Entity,
            &NodePart,
            &mut Sprite,
            &mut Transform,
            Option<&NodePreviewSize>,
        ),
        Without<NodeLabel>,
    >,
    mut previous: Local<LabelSyncState>,
) {
    let active = editor.active.as_ref().map(|e| (e.id.clone(), e.size));
    if previous.0 == scene.0.revision
        && previous.1 == active
        && previous.2 == editor.overview.generation
    {
        return;
    }
    *previous = (scene.0.revision, active, editor.overview.generation);
    for (label, mut text, mut visibility, mut bounds, mut layout, mut transform) in &mut labels {
        let node = &scene.0.nodes[label.0];
        let display_text = node_display_text(node, &assets);
        if text.0 != display_text {
            text.0 = display_text.to_owned();
        }
        let hidden = editor.active.as_ref().is_some_and(|e| e.id == node.id)
            || editor.overview.active.contains(&node.id);
        let target = if hidden {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        };
        if *visibility != target {
            *visibility = target;
        }
        let width = text_width(node);
        if bounds.width != width {
            bounds.width = width;
        }
        let target_layout = node_text_layout(node);
        if layout.linebreak != target_layout.linebreak || layout.justify != target_layout.justify {
            *layout = target_layout;
        }
        let y = if node.kind == "LatexNode" {
            0.0
        } else {
            node.rect[3] * 0.5 - padding(node)
        };
        if transform.translation.y != y {
            transform.translation.y = y;
        }
    }
    for (entity, part, mut sprite, mut transform, previous_preview) in &mut parts {
        let n = &scene.0.nodes[part.index];
        let original_size = Vec2::new(n.rect[2], n.rect[3]);
        let preview = editor
            .active
            .as_ref()
            .filter(|edit| edit.id == n.id)
            .map(|edit| Vec2::from_array(edit.size));
        let size = preview.unwrap_or(original_size);
        if let Some(size) = preview
            && previous_preview.is_none_or(|previous| previous.0 != size)
        {
            commands.entity(entity).insert(NodePreviewSize(size));
        } else if preview.is_none() && previous_preview.is_some() {
            commands.entity(entity).remove::<NodePreviewSize>();
        }
        let p = (size - original_size) * Vec2::new(0.5, -0.5);
        if n.attachment.is_none() {
            theme::resize(&mut sprite, &assets.corners, size, part.border.is_some());
        } else if sprite.custom_size != Some(size) {
            sprite.custom_size = Some(size);
        }
        if transform.translation.truncate() != p {
            transform.translation.x = p.x;
            transform.translation.y = p.y;
        }
    }
}
