use super::*;
use project_graph_bevy_canvas::GraphEdge;
use std::io::Cursor;

#[derive(Default)]
pub struct WarmupClock {
    ready_since: Option<f64>,
    pub load_ms: Option<f64>,
}
impl WarmupClock {
    pub fn elapsed(&mut self, now: f64, status: &msdf_text::MsdfStatus) -> Option<f64> {
        if !status.ready() {
            self.ready_since = None;
            return None;
        }
        let since = *self.ready_since.get_or_insert_with(|| {
            self.load_ms = Some(now * 1000.0);
            now
        });
        Some(now - since)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum RedrawMode {
    Idle,
    Loading,
    Continuous,
}
#[allow(clippy::too_many_arguments)] // Independent activity resources determine the wake interval.
pub(super) fn redraw(
    config: Res<Config>,
    editor: Res<editor::Editor>,
    physics: Res<physics::PhysicsEditor>,
    cache: Res<msdf_text::MsdfCache>,
    effects: Res<effects::Effects>,
    zoom: Option<Res<navigation::WheelZoom>>,
    mut settings: ResMut<WinitSettings>,
    mut previous: Local<Option<RedrawMode>>,
) {
    let text = cache.status();
    let mode = if config.benchmark
        || config.preview
        || physics.running
        || effects.active()
        || zoom.is_some_and(|zoom| zoom.active())
    {
        RedrawMode::Continuous
    } else if text.pending_glyphs > 0
        || text.pending_labels > 0
        || (editor.web_input && editor.active.is_some())
    {
        RedrawMode::Loading
    } else {
        RedrawMode::Idle
    };
    if *previous == Some(mode) {
        return;
    }
    *previous = Some(mode);
    *settings = match mode {
        RedrawMode::Idle => idle_settings(),
        RedrawMode::Continuous => WinitSettings::continuous(),
        RedrawMode::Loading => WinitSettings {
            focused_mode: bevy::winit::UpdateMode::reactive(std::time::Duration::from_millis(16)),
            unfocused_mode: bevy::winit::UpdateMode::reactive_low_power(
                std::time::Duration::from_millis(16),
            ),
        },
    };
}

pub fn msdf_status(status: &msdf_text::MsdfStatus) -> serde_json::Value {
    serde_json::json!({
        "pending_glyphs": status.pending_glyphs, "pending_labels": status.pending_labels,
        "failed_labels": status.failed_labels, "generated_glyphs": status.generated_glyphs,
        "color_labels": status.color_labels,
        "mesh_rebuilds": status.mesh_rebuilds, "atlas_pages": status.atlas_pages,
        "atlas_bytes_per_copy": status.atlas_bytes,
    })
}

pub fn write_report(path: &std::path::Path, value: &serde_json::Value) -> std::io::Result<()> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_vec_pretty(value)?)
}

pub fn synthetic(count: usize, long_text: bool, images: bool, dense: bool) -> Result<Document> {
    if !(1..=20000).contains(&count) {
        return Err("--synthetic requires 1..20000 nodes".into());
    }
    let mut graph = Graph {
        nodes: vec![],
        edges: vec![],
        attachments: Default::default(),
        counts: Default::default(),
    };
    let columns = (count as f32).sqrt().ceil() as usize;
    if images {
        let pixels = image::RgbaImage::from_fn(64, 64, |x, y| {
            image::Rgba([x as u8 * 4, y as u8 * 4, 192, 255])
        });
        let mut bytes = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(pixels).write_to(&mut bytes, image::ImageFormat::Png)?;
        graph
            .attachments
            .insert("stress.png".into(), bytes.into_inner());
    }
    for i in 0..count {
        let image = images && i % 10 == 0;
        let text = if image {
            String::new()
        } else if long_text {
            format!(
                "节点 {i} · 中文 English 🙂\n{}",
                "这是一段用于验证换行和缓存的长文本。Abc 123，标点与组合字符 é。".repeat(12)
            )
        } else {
            format!("节点 {i} · Bevy")
        };
        let height = if long_text && !image { 720.0 } else { 96.0 };
        let gap = if dense { 0.5 } else { 32.0 };
        let kind = if image { "ImageNode" } else { "TextNode" };
        graph.nodes.push(GraphNode {
            id: format!("stress-{i}"),
            kind: kind.into(),
            rect: [
                (i % columns) as f32 * (240.0 + gap),
                (i / columns) as f32 * (if long_text { 720.0 } else { 96.0 } + gap),
                240.0,
                height,
            ],
            text,
            url: None,
            font_size: 24.0,
            auto_size: false,
            color: [0.0; 4],
            attachment: image.then(|| "stress.png".into()),
            source_name: None,
            children: vec![],
            locked: false,
            collapsed: false,
            stroke: vec![],
        });
        *graph.counts.entry(kind.into()).or_default() += 1;
        if i > 0 && i % columns != 0 {
            graph.edges.push(GraphEdge {
                id: format!("edge-{i}"),
                members: vec![i - 1, i],
                rates: vec![[1.0, 0.5], [0.0, 0.5]],
                text: String::new(),
                color: [0.0; 4],
                arrow: true,
                ..default()
            });
        }
    }
    Ok(Document::memory(graph))
}

pub fn memory_kib() -> Option<usize> {
    fs::read_to_string("/proc/self/status")
        .ok()?
        .lines()
        .find(|l| l.starts_with("VmRSS:"))?
        .split_whitespace()
        .nth(1)?
        .parse()
        .ok()
}

pub fn proportional_memory_kib() -> Option<usize> {
    fs::read_to_string("/proc/self/smaps_rollup")
        .ok()?
        .lines()
        .find(|line| line.starts_with("Pss:"))?
        .split_whitespace()
        .nth(1)?
        .parse()
        .ok()
}

pub fn summary(values: &[f64]) -> serde_json::Value {
    if values.is_empty() {
        return serde_json::Value::Null;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    serde_json::json!({"steps":sorted.len(),"mean_ms":sorted.iter().sum::<f64>()/sorted.len() as f64,
        "p95_ms":sorted[(sorted.len() as f64*0.95) as usize],"p99_ms":sorted[(sorted.len() as f64*0.99) as usize]})
}

#[derive(Resource, Default)]
pub struct Counters {
    pub text_layouts: usize,
    pub max_active: usize,
    pub render: std::collections::BTreeMap<String, Vec<f64>>,
    pub app_ms: Vec<f64>,
    pub msdf_mesh_rebuilds: usize,
    pub msdf_generated_glyphs: usize,
    pub edge_mesh_rebuilds: usize,
    start: Option<Instant>,
}
impl Counters {
    pub fn clear_phase(&mut self) {
        self.text_layouts = 0;
        self.max_active = 0;
        self.render.clear();
        self.app_ms.clear();
        self.msdf_mesh_rebuilds = 0;
        self.msdf_generated_glyphs = 0;
        self.edge_mesh_rebuilds = 0;
    }
}
pub fn frame_start(mut stats: ResMut<Counters>, config: Res<Config>) {
    if config.benchmark && config.diagnostics {
        stats.start = Some(Instant::now());
    }
}
#[allow(clippy::too_many_arguments)]
pub fn counters(
    mut stats: ResMut<Counters>,
    physics: Res<physics::PhysicsEditor>,
    layouts: Query<(), Changed<bevy::text::TextLayoutInfo>>,
    diagnostics: Res<bevy::diagnostic::DiagnosticsStore>,
    config: Res<Config>,
    mut last: Local<HashMap<String, bevy::platform::time::Instant>>,
    cache: Res<msdf_text::MsdfCache>,
    mut previous_msdf: Local<msdf_text::MsdfStatus>,
    edges: Res<edges::EdgeStats>,
    mut previous_edges: Local<usize>,
) {
    stats.edge_mesh_rebuilds += edges.rebuilds.saturating_sub(*previous_edges);
    *previous_edges = edges.rebuilds;
    let msdf = cache.status();
    stats.msdf_mesh_rebuilds += msdf
        .mesh_rebuilds
        .saturating_sub(previous_msdf.mesh_rebuilds);
    stats.msdf_generated_glyphs += msdf
        .generated_glyphs
        .saturating_sub(previous_msdf.generated_glyphs);
    *previous_msdf = msdf;
    stats.text_layouts += layouts.iter().count();
    if physics.running {
        stats.max_active = stats.max_active.max(physics.active);
    }
    if config.benchmark && config.diagnostics {
        if let Some(start) = stats.start {
            stats.app_ms.push(start.elapsed().as_secs_f64() * 1000.0);
        }
        for diagnostic in diagnostics.iter() {
            let name = diagnostic.path().to_string();
            if name.starts_with("render/")
                && diagnostic.suffix == "ms"
                && let Some(sample) = diagnostic.measurement()
                && last.get(&name) != Some(&sample.time)
            {
                last.insert(name.clone(), sample.time);
                stats.render.entry(name).or_default().push(sample.value);
            }
        }
    }
}

pub fn environment(
    window: &Window,
    handle: Option<&bevy::window::RawHandleWrapper>,
    adapter: Option<&bevy::render::renderer::RenderAdapterInfo>,
) -> serde_json::Value {
    use raw_window_handle::RawDisplayHandle;
    let backend = handle.map(|h| match h.get_display_handle() {
        RawDisplayHandle::Wayland(_) => "Wayland",
        RawDisplayHandle::Xlib(_) | RawDisplayHandle::Xcb(_) => "X11",
        RawDisplayHandle::Windows(_) => "Windows",
        RawDisplayHandle::AppKit(_) => "AppKit",
        _ => "other",
    });
    serde_json::json!({
        "window_backend": backend,
        "logical_size": [window.width(), window.height()],
        "physical_size": [window.physical_width(), window.physical_height()],
        "base_scale_factor": window.resolution.base_scale_factor(),
        "effective_scale_factor": window.scale_factor(),
        "scale_factor_override": window.resolution.scale_factor_override(),
        "focused": window.focused,
        "requested_present_mode": format!("{:?}", window.present_mode),
        "actual_present_mode": null,
        "adapter": adapter.map(|a| serde_json::json!({
            "name": a.name, "backend": format!("{:?}", a.backend),
            "device_type": format!("{:?}", a.device_type),
            "driver": a.driver, "driver_info": a.driver_info,
        })),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Resource, Default)]
    struct EmitEdit(bool);

    fn emit_edit(mut next: ResMut<EmitEdit>, mut effects: ResMut<effects::Effects>) {
        if std::mem::take(&mut next.0) {
            effects.begin_edit(Vec2::ZERO, Vec2::splat(100.0), true);
        }
    }

    #[test]
    fn wakeup_effect_survives_old_delta_and_redraw_returns_to_idle_after_cleanup() {
        use bevy::{asset::AssetPlugin, text::TextPlugin, time::TimeUpdateStrategy};
        use std::time::Duration;

        // No window, GPU, fonts or glyph jobs: an empty MSDF update clears its
        // startup pending status through the same path as an empty document.
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default(), TextPlugin))
            .init_resource::<Assets<Image>>()
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<ColorMaterial>>()
            .init_resource::<Assets<msdf_text::MsdfMaterial>>()
            .init_resource::<msdf_text::MsdfCache>()
            .init_resource::<editor::Editor>()
            .init_resource::<physics::PhysicsEditor>()
            .init_resource::<EmitEdit>()
            .insert_resource(idle_settings())
            .insert_resource(Config {
                font: vec![],
                benchmark: false,
                output: "unused".into(),
                screenshot: None,
                light: false,
                ui_scale: 1.0,
                physics: false,
                no_ui: false,
                preview: false,
                edit_node: None,
                diagnostics: false,
            })
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::ZERO))
            .add_systems(PreUpdate, effects::update)
            .add_systems(Update, emit_edit)
            .add_systems(
                PostUpdate,
                (msdf_text::update, effects::draw, redraw).chain(),
            );
        effects::install(&mut app);
        app.update();
        assert!(
            app.world()
                .resource::<msdf_text::MsdfCache>()
                .status()
                .ready()
        );
        assert_eq!(
            app.world().resource::<WinitSettings>().focused_mode,
            idle_settings().focused_mode
        );

        // A long reactive-window sleep belongs to the previous frame. A new
        // effect emitted after PreUpdate must receive its full 200 ms lifetime.
        app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs(20)));
        app.world_mut().resource_mut::<EmitEdit>().0 = true;
        app.update();
        assert_eq!(
            app.world().resource::<Time<Real>>().delta(),
            Duration::from_secs(20)
        );
        assert!(app.world().resource::<effects::Effects>().active());
        assert_eq!(
            app.world().resource::<WinitSettings>().focused_mode,
            bevy::winit::UpdateMode::Continuous
        );

        app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
            80,
        )));
        app.update();
        app.update();
        assert!(app.world().resource::<effects::Effects>().active());
        app.update();
        assert!(!app.world().resource::<effects::Effects>().active());
        let settings = app.world().resource::<WinitSettings>();
        assert_eq!(settings.focused_mode, idle_settings().focused_mode);
        assert_eq!(settings.unfocused_mode, idle_settings().unfocused_mode);
        let world = app.world_mut();
        let mut meshes = world.query_filtered::<&Visibility, With<Mesh2d>>();
        assert_eq!(*meshes.single(world).unwrap(), Visibility::Hidden);

        app.update();
        assert!(!app.world().resource::<effects::Effects>().active());
        assert_eq!(
            app.world().resource::<WinitSettings>().focused_mode,
            idle_settings().focused_mode
        );
    }

    #[test]
    fn msdf_loading_time_is_excluded_and_new_work_restarts_warmup() {
        let mut clock = WarmupClock::default();
        let mut status = msdf_text::MsdfStatus::default();
        assert_eq!(clock.elapsed(0.0, &status), None);
        assert_eq!(clock.elapsed(10.0, &status), None);
        status.pending_labels = 0;
        assert_eq!(clock.elapsed(12.0, &status), Some(0.0));
        assert_eq!(clock.load_ms, Some(12000.0));
        assert_eq!(clock.elapsed(16.0, &status), Some(4.0));
        status.pending_glyphs = 1;
        assert_eq!(clock.elapsed(17.0, &status), None);
        status.pending_glyphs = 0;
        assert_eq!(clock.elapsed(18.0, &status), Some(0.0));
        status.failed_labels = 1;
        assert_eq!(clock.elapsed(22.0, &status), None);
    }

    #[test]
    fn report_writer_supports_nested_paths_and_propagates_io_failure() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("nested/report.json");
        let report = serde_json::json!({"valid":false,"error":"glyph preparation failed"});
        write_report(&path, &report).unwrap();
        let loaded: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(loaded, report);
        assert!(write_report(&path.join("invalid.json"), &report).is_err());
    }
}
