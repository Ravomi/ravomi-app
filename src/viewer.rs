use bevy::math::bounding::{Aabb2d, BoundingVolume, IntersectsVolume};
use bevy::{
    asset::RenderAssetUsages,
    input::mouse::MouseScrollUnit,
    prelude::*,
    render::{
        render_resource::{Extent3d, TextureDimension, TextureFormat},
        view::screenshot::{Screenshot, save_to_disk},
    },
    sprite::Anchor,
    text::{FontSize, LineBreak, TextBounds},
    window::{PresentMode, PrimaryWindow, WindowResizeConstraints, WindowResolution},
    winit::WinitSettings,
};
use bevy_egui::{EguiPlugin, EguiPostUpdateSet, EguiPrimaryContextPass};
use project_graph_bevy_canvas::document::Document;
use project_graph_bevy_canvas::{Graph, GraphNode, Result};
#[path = "canvas_ai.rs"]
mod canvas_ai;
#[path = "canvas_tools.rs"]
mod canvas_tools;
#[path = "creation.rs"]
mod creation;
#[path = "cutting.rs"]
mod cutting;
#[path = "edges.rs"]
mod edges;
#[path = "effects.rs"]
mod effects;

#[cfg(test)]
#[path = "camera_tests.rs"]
mod camera_tests;
#[path = "jump.rs"]
mod jump;
#[path = "navigation.rs"]
mod navigation;
#[cfg(test)]
#[path = "navigation_tests.rs"]
mod navigation_tests;
#[cfg(test)]
use bevy::input::mouse::AccumulatedMouseScroll;
#[path = "dock_menu.rs"]
mod dock_menu;
#[path = "editor.rs"]
mod editor;
#[path = "export_image.rs"]
mod export_image;
#[path = "export_ui.rs"]
mod export_ui;
#[path = "group_clip.rs"]
mod group_clip;
#[path = "home.rs"]
mod home;
#[path = "image_import.rs"]
mod image_import;
#[path = "image_resize.rs"]
mod image_resize;
#[path = "math_text.rs"]
mod math_text;
#[path = "msdf_text.rs"]
mod msdf_text;
#[path = "physics.rs"]
mod physics;
#[path = "recovery_ui.rs"]
mod recovery_ui;
#[path = "section_overview.rs"]
mod section_overview;
#[path = "sections.rs"]
mod sections;
#[path = "stress.rs"]
mod stress;
#[path = "tabs.rs"]
mod tabs;
#[path = "theme.rs"]
mod theme;
#[path = "url_open.rs"]
mod url_open;
use msdf_text::MsdfText;
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::PathBuf,
    time::Instant,
};

#[derive(Resource)]
struct Scene(Document);
#[derive(Component)]
struct NodeLabel(usize);
#[derive(Component)]
struct NodePart {
    index: usize,
    border: Option<usize>,
}
/// Transient geometry supplied by the input adapter, never persisted to Document.
/// Render systems consume this without depending on an egui/React editor.
#[derive(Component, Clone, Copy)]
struct NodePreviewSize(Vec2);
#[derive(Component)]
struct NodeVisual(usize);
#[derive(Component)]
struct NodeIdentity(String, String);
#[derive(Component)]
struct TrailPosition(Vec2);
#[derive(Resource)]
struct NodeAssets {
    font: Handle<Font>,
    corners: theme::NodeCorners,
    textures: HashMap<String, Handle<Image>>,
}
#[derive(Resource, Default)]
struct Interaction {
    cutting: cutting::Gesture,
    creation: creation::Gesture,
    jump: jump::Gesture,
    linking: Option<(usize, [f32; 2])>,
    link_start: Vec2,
    link_right: bool,
    selected: HashSet<usize>,
    selected_edges: HashSet<String>,
    selection_revision: u64,
    moving: HashSet<usize>,
    drag_start: Option<Vec2>,
    last_cursor: Option<Vec2>,
    screen_cursor: Option<Vec2>,
    selecting: bool,
    dirty: bool,
    last_click: Option<(usize, Instant, Vec2)>,
    context_press: Option<Vec2>,
}
#[derive(Resource)]
struct Config {
    font: Vec<u8>,
    benchmark: bool,
    output: PathBuf,
    screenshot: Option<PathBuf>,
    light: bool,
    ui_scale: f32,
    physics: bool,
    no_ui: bool,
    preview: bool,
    edit_node: Option<usize>,
    diagnostics: bool,
}
#[derive(Resource)]
struct Measurement {
    start: Instant,
    warmup: stress::WarmupClock,
    previous: Instant,
    phase: usize,
    samples: Vec<f64>,
    results: Vec<serde_json::Value>,
    home: Vec2,
    home_scale: f32,
    screenshot_taken: bool,
}
fn arg(args: &[String], name: &str) -> Option<String> {
    args.windows(2)
        .find(|pair| pair[0] == name)
        .map(|pair| pair[1].clone())
}
fn idle_settings() -> WinitSettings {
    WinitSettings {
        focused_mode: bevy::winit::UpdateMode::reactive(std::time::Duration::from_millis(250)),
        unfocused_mode: bevy::winit::UpdateMode::reactive_low_power(
            std::time::Duration::from_secs(1),
        ),
    }
}
pub fn run_home(args: &[String]) -> Result<()> {
    run(Document::blank()?, args)
}

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
struct CanvasVisuals;

fn configure_visual_order(app: &mut App) {
    // Slice computation must consume current radii before a reactive window sleeps.
    app.configure_sets(
        PostUpdate,
        CanvasVisuals.before(bevy::sprite_render::SpriteSystems::ComputeSlices),
    );
}

pub fn run(graph: Document, args: &[String]) -> Result<()> {
    let mut graph = if let Some(count) = arg(args, "--synthetic") {
        stress::synthetic(
            count.parse()?,
            args.iter().any(|a| a == "--long-text"),
            args.iter().any(|a| a == "--images"),
            args.iter().any(|a| a == "--dense"),
        )?
    } else {
        graph
    };
    if args.iter().any(|a| a == "--example") {
        // Examples open as unsaved drafts so their source remains untouched.
        graph.path = None;
    }
    let font_path = arg(args, "--font").unwrap_or_else(|| {
        if cfg!(target_os = "macos") {
            "/System/Library/Fonts/Hiragino Sans GB.ttc".into()
        } else {
            "/usr/share/fonts/google-noto-sans-cjk-fonts/NotoSansCJK-Regular.ttc".into()
        }
    });
    let font = fs::read(&font_path)
        .map_err(|e| format!("Cannot read Chinese font {font_path}: {e}. Pass --font FONT.ttf"))?;
    let export = export_image::ExportImage::from_args(args, &graph)?;
    let mut config = Config {
        font,
        benchmark: args.iter().any(|a| a == "--benchmark"),
        output: arg(args, "--output")
            .map(PathBuf::from)
            .unwrap_or_else(|| "results/bevy.json".into()),
        screenshot: arg(args, "--screenshot").map(PathBuf::from),
        light: !args.iter().any(|a| a == "--dark"),
        ui_scale: arg(args, "--ui-scale")
            .and_then(|s| s.parse().ok())
            .unwrap_or(1.0),
        physics: physics::enabled_from_args(args),
        no_ui: args.iter().any(|a| a == "--no-ui"),
        preview: args.iter().any(|a| a == "--preview"),
        edit_node: arg(args, "--edit-node").map(|s| s.parse()).transpose()?,
        diagnostics: args.iter().any(|a| a == "--diagnostics"),
    };
    if export.is_some() {
        config.no_ui = true;
        config.preview = false;
        config.benchmark = false;
        config.physics = false;
        config.edit_node = None;
    }
    if let Some(i) = config.edit_node {
        if config.no_ui {
            return Err("--edit-node cannot be combined with --no-ui".into());
        }
        if graph
            .nodes
            .get(i)
            .is_none_or(|n| n.kind != "TextNode" || n.locked)
        {
            return Err("--edit-node must name an unlocked text node".into());
        }
    }
    let benchmark_mode = config.benchmark || config.preview || export.is_some();
    let exporting = export.is_some();
    let diagnostics = config.diagnostics;
    let mut app = App::new();
    configure_visual_order(&mut app);
    let tabs = tabs::Tabs::from_args(&graph, args)?;
    let mut editor = editor::Editor::with_startup(
        args.iter().any(|arg| arg == "--preview-menu"),
        args.iter().any(|arg| arg == "--home"),
    );
    editor.light = config.light;
    #[cfg(feature = "react-editor")]
    if args.iter().any(|arg| arg == "--react-editor") {
        editor::react_editor::initialize()?;
        editor.web_input = true;
    }
    let mut interaction = Interaction::default();
    if let Some(mode) = arg(args, "--preview-tool") {
        editor.hide_home();
        if mode == "collapsed" {
            let group = if let Some(index) = graph
                .nodes
                .iter()
                .position(|node| node.kind == "Section" && !node.children.is_empty())
            {
                index
            } else {
                for point in [[0.0, 0.0], [300.0, 0.0], [650.0, 0.0]] {
                    graph.create_text_node(point)?;
                }
                graph.add_edge(project_graph_bevy_canvas::GraphEdge {
                    id: uuid::Uuid::new_v4().to_string(),
                    members: vec![1, 2],
                    rates: vec![[0.5; 2]; 2],
                    text: "外部关系".into(),
                    ..default()
                })?;
                graph.create_group(&HashSet::from([0, 1]))?
            };
            graph.set_sections_collapsed(&HashSet::from([group]), true)?;
        } else if mode == "about" {
            editor.about = true;
        } else if mode == "export" {
            editor.export.open = true;
        } else if mode == "recovery" || mode == "backup-settings" {
            editor.recovery.refresh();
            editor.recovery.settings = mode == "backup-settings";
        } else if mode == "exit" {
            graph.create_text_node([0.0; 2])?;
            editor.closing = true;
        } else {
            editor.tools.preview(&mode, &graph, &mut interaction)?;
        }
    }
    app.insert_resource(ClearColor(theme::window_background(config.light)))
        .insert_resource(Scene(graph))
        .insert_resource(config)
        .insert_resource(if benchmark_mode {
            WinitSettings::continuous()
        } else {
            idle_settings()
        })
        .insert_resource(interaction)
        .init_resource::<navigation::WheelZoom>()
        .insert_resource(editor)
        .insert_resource(tabs)
        .init_resource::<stress::Counters>()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            close_when_requested: false,
            primary_window: Some(Window {
                visible: !exporting,
                transparent: true,
                composite_alpha_mode: if cfg!(target_os = "macos") {
                    bevy::window::CompositeAlphaMode::PostMultiplied
                } else {
                    bevy::window::CompositeAlphaMode::PreMultiplied
                },
                title: "Ravomi".into(),
                // Keep AppKit's traffic lights, but paint the titlebar with our canvas.
                fullsize_content_view: cfg!(target_os = "macos"),
                titlebar_transparent: cfg!(target_os = "macos"),
                titlebar_show_title: !cfg!(target_os = "macos"),
                name: Some("io.github.graphif.Graplu".into()),
                resolution: if benchmark_mode && !args.iter().any(|arg| arg == "--native-scale") {
                    WindowResolution::new(1280, 800).with_scale_factor_override(1.0)
                } else {
                    WindowResolution::new(1280, 800)
                },
                resizable: !benchmark_mode,
                resize_constraints: if benchmark_mode {
                    WindowResizeConstraints {
                        min_width: 1280.0,
                        max_width: 1280.0,
                        min_height: 800.0,
                        max_height: 800.0,
                    }
                } else {
                    default()
                },
                present_mode: PresentMode::AutoNoVsync,
                ..default()
            }),
            ..default()
        }))
        .add_plugins(EguiPlugin::default())
        .init_resource::<image_resize::Resize>()
        .init_resource::<canvas_ai::State>()
        .insert_resource(sections::Display::new(
            arg(args, "--preview-tool").as_deref() == Some("collapsed"),
        ))
        .add_systems(Startup, setup)
        .add_systems(Update, editor::set_brand_window_icon)
        .add_systems(
            EguiPrimaryContextPass,
            (
                editor::ui,
                (
                    editor::apply_window_action,
                    tabs::apply,
                    canvas_ai::ui,
                    image_resize::ui,
                    navigation::navigate,
                    sections::prepare,
                    input,
                    image_import::dropped_files,
                    benchmark,
                    refresh_groups,
                )
                    .chain()
                    .run_if(first_canvas_pass),
                editor::draw_overview,
            )
                .chain(),
        )
        .add_systems(Update, editor::close_request)
        .add_systems(First, stress::frame_start)
        .add_systems(Last, stress::counters)
        .add_systems(Last, tabs::backup)
        .add_systems(PostUpdate, stress::redraw.after(msdf_text::update))
        .add_systems(
            PostUpdate,
            (
                sync_nodes,
                editor::sync_labels,
                theme::sync,
                section_overview::sync,
                msdf_text::update,
                section_overview::position,
                editor::fit_text_nodes,
                refresh_groups,
                fit_groups,
                image_resize::sync,
                sections::sync,
                sections::stack_groups,
                sections::sync_corners,
                edges::sync,
                edges::sync_strokes,
                update_visuals,
                edges::label_backdrops,
                selection_overlay,
                status,
            )
                .chain()
                .after(EguiPostUpdateSet::EndPass)
                .after(bevy::text::load_font_assets_into_font_collection)
                .before(bevy::sprite::update_text2d_layout)
                .before(bevy::text::detect_text_needs_rerender)
                .in_set(CanvasVisuals)
                .before(bevy::camera::CameraUpdateSystems)
                .before(bevy::transform::TransformSystems::Propagate),
        );
    physics::install(&mut app);
    effects::install(&mut app);
    group_clip::install(&mut app);
    if let Some(export) = export {
        app.insert_resource(export)
            .add_systems(Startup, export_image::setup.after(setup))
            .add_systems(
                PostUpdate,
                export_image::render
                    .after(msdf_text::update)
                    .after(fit_groups)
                    .before(bevy::camera::CameraUpdateSystems),
            );
    }
    app.add_systems(PreUpdate, effects::update).add_systems(
        PostUpdate,
        (movement_effects, effects::draw)
            .chain()
            .after(update_visuals)
            .after(selection_overlay)
            .before(stress::redraw)
            .before(bevy::transform::TransformSystems::Propagate),
    );
    msdf_text::install(&mut app);
    edges::install(&mut app);
    #[cfg(feature = "react-editor")]
    if args.iter().any(|arg| arg == "--react-editor") {
        editor::react_editor::install(&mut app);
    }
    if diagnostics {
        app.add_plugins(bevy::render::diagnostic::RenderDiagnosticsPlugin);
    }
    let outcome = app.run();
    if exporting && !matches!(outcome, AppExit::Success) {
        return Err("PNG 离屏渲染失败，请查看渲染错误信息".into());
    }
    Ok(())
}
fn center(node: &GraphNode) -> Vec2 {
    Vec2::new(
        node.rect[0] + node.rect[2] * 0.5,
        -node.rect[1] - node.rect[3] * 0.5,
    )
}
fn color(rgba: [f32; 4]) -> Color {
    Color::srgba(rgba[0], rgba[1], rgba[2], rgba[3])
}
fn padding(node: &GraphNode) -> f32 {
    if node.kind == "Section" {
        8.0
    } else {
        node.font_size * 14.0 / 32.0
    }
}
fn text_width(node: &GraphNode) -> Option<f32> {
    (!node.auto_size && !node.collapsed).then(|| (node.rect[2] - 2.0 * padding(node)).max(1.0))
}
fn node_text_layout(node: &GraphNode) -> TextLayout {
    TextLayout::new(
        Justify::Center,
        if node.auto_size || node.collapsed {
            LineBreak::NoWrap
        } else {
            LineBreak::AnyCharacter
        },
    )
}
fn fit_view(graph: &Graph, width: f32, height: f32, ui_scale: f32) -> (Vec2, f32) {
    let rect = graph.bounds();
    let top = 96.0 * ui_scale.clamp(0.75, 2.0);
    let bottom = 56.0 * ui_scale.clamp(0.75, 2.0);
    let scale = (rect[2] / (width - 128.0).max(100.0))
        .max(rect[3] / (height - top - bottom).max(100.0))
        .max(0.01);
    (
        Vec2::new(
            rect[0] + rect[2] * 0.5,
            -rect[1] - rect[3] * 0.5 + (top - bottom) * 0.5 * scale,
        ),
        scale,
    )
}

fn attachment_svg_options(font: &[u8]) -> resvg::usvg::Options<'static> {
    use resvg::usvg::fontdb::{Family, Query};
    let mut options = resvg::usvg::Options::default();
    let db = options.fontdb_mut();
    db.load_font_data(font.to_vec());
    let fallback = db
        .faces()
        .next()
        .and_then(|face| face.families.first())
        .map(|(name, _)| name.clone());
    db.load_system_fonts();
    if let Some(fallback) = fallback {
        // usvg appends Serif when an SVG's named font is unavailable. Fontdb's
        // platform defaults may name uninstalled fonts, leaving all text empty.
        for family in [
            Family::Serif,
            Family::SansSerif,
            Family::Monospace,
            Family::Cursive,
            Family::Fantasy,
        ] {
            if db
                .query(&Query {
                    families: &[family],
                    ..default()
                })
                .is_some()
            {
                continue;
            }
            match family {
                Family::Serif => db.set_serif_family(&fallback),
                Family::SansSerif => db.set_sans_serif_family(&fallback),
                Family::Monospace => db.set_monospace_family(&fallback),
                Family::Cursive => db.set_cursive_family(&fallback),
                Family::Fantasy => db.set_fantasy_family(&fallback),
                Family::Name(_) => unreachable!(),
            }
        }
        options.font_family = fallback;
    }
    options
}

fn check_attachment_size(width: u32, height: u32) -> Result<()> {
    if width == 0
        || height == 0
        || width > 8192
        || height > 8192
        || u64::from(width) * u64::from(height) > 16 * 1024 * 1024
    {
        return Err(format!("图片尺寸 {width}×{height} 超出显示限制（单边 8192、总计 16777216 像素），请缩小图片后重试").into());
    }
    Ok(())
}

fn attachment_pixels(
    name: &str,
    bytes: &[u8],
    svg_options: &mut Option<resvg::usvg::Options<'static>>,
    font: &[u8],
) -> Result<(u32, u32, Vec<u8>)> {
    if name
        .rsplit('.')
        .next()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("svg"))
    {
        let options = svg_options.get_or_insert_with(|| attachment_svg_options(font));
        let tree = resvg::usvg::Tree::from_data(bytes, options)?;
        let size = tree.size().to_int_size();
        check_attachment_size(size.width(), size.height())?;
        let mut pixmap = resvg::tiny_skia::Pixmap::new(size.width(), size.height())
            .ok_or("无法分配图片像素缓冲区")?;
        resvg::render(
            &tree,
            resvg::tiny_skia::Transform::identity(),
            &mut pixmap.as_mut(),
        );
        // tiny-skia is premultiplied; Bevy's sprite shader expects straight alpha.
        let mut rgba = pixmap.take();
        for pixel in rgba.as_chunks_mut::<4>().0 {
            if pixel[3] > 0 {
                for c in 0..3 {
                    pixel[c] = ((pixel[c] as u32 * 255) / pixel[3] as u32).min(255) as u8;
                }
            }
        }
        Ok((size.width(), size.height(), rgba))
    } else {
        let dimensions = image::ImageReader::new(std::io::Cursor::new(bytes))
            .with_guessed_format()?
            .into_dimensions()?;
        check_attachment_size(dimensions.0, dimensions.1)?;
        let image = image::load_from_memory(bytes)?.into_rgba8();
        Ok((image.width(), image.height(), image.into_raw()))
    }
}

#[allow(clippy::too_many_arguments)]
fn setup(
    mut commands: Commands,
    scene: Res<Scene>,
    config: Res<Config>,
    existing_assets: Option<Res<NodeAssets>>,
    mut fonts: ResMut<Assets<Font>>,
    mut images: ResMut<Assets<Image>>,
    mut edge_materials: ResMut<Assets<edges::EdgeMaterial>>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut editor: ResMut<editor::Editor>,
    mut cameras: Query<(&mut Transform, &mut Projection), With<Camera2d>>,
) {
    let graph = &scene.0;
    let (mut home, mut scale) = fit_view(graph, window.width(), window.height(), config.ui_scale);
    if let Some(i) = config.edit_node
        && let Some(node) = graph.nodes.get(i)
        && node.kind == "TextNode"
        && !node.locked
    {
        home = center(node);
        scale = (node.rect[2] / 400.0).clamp(0.1, 1.0);
        editor.begin(node);
    }
    let mut projection = OrthographicProjection::default_2d();
    projection.scale = scale;
    if let Ok((mut transform, mut existing)) = cameras.single_mut() {
        *transform = canvas_camera_transform(home);
        *existing = Projection::Orthographic(projection);
    } else {
        commands.spawn((
            Camera2d,
            Projection::Orthographic(projection),
            canvas_camera_transform(home),
        ));
    }
    let font = existing_assets
        .as_ref()
        .map(|assets| assets.font.clone())
        .filter(|font| fonts.contains(font.id()))
        .unwrap_or_else(|| fonts.add(Font::from_bytes(config.font.clone())));
    edges::setup(&mut commands, font.clone(), &mut edge_materials);
    let corners = theme::NodeCorners::new(&mut images);
    let mut textures = HashMap::new();
    let mut svg_options = None;
    let mut attachment_errors = Vec::new();
    for (name, bytes) in &graph.attachments {
        if !graph
            .nodes
            .iter()
            .any(|node| node.attachment.as_ref() == Some(name))
        {
            continue;
        }
        let decoded = match attachment_pixels(name, bytes, &mut svg_options, &config.font) {
            Ok(decoded) => decoded,
            Err(error) => {
                attachment_errors.push(format!("{name}：{error}"));
                continue;
            }
        };
        let image = Image::new(
            Extent3d {
                width: decoded.0,
                height: decoded.1,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            decoded.2,
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::default(),
        );
        textures.insert(name.clone(), images.add(image));
    }
    if !attachment_errors.is_empty() {
        editor.report_error(format!(
            "部分图片无法显示，原始附件仍保留，其他内容可以继续编辑和保存。请检查图片是否损坏或格式是否受支持。\n\n{}",
            attachment_errors.join("\n")
        ));
    }
    let assets = NodeAssets {
        font,
        corners,
        textures,
    };
    for (index, node) in graph.nodes.iter().enumerate() {
        spawn_node(&mut commands, index, node, &assets, editor.light);
    }
    commands.insert_resource(assets);
    commands.insert_resource(Measurement {
        start: Instant::now(),
        warmup: default(),
        previous: Instant::now(),
        phase: 0,
        samples: vec![],
        results: vec![],
        home,
        home_scale: scale,
        screenshot_taken: false,
    });
}
fn node_display_text<'a>(node: &'a GraphNode, assets: &NodeAssets) -> &'a str {
    if node
        .attachment
        .as_ref()
        .is_some_and(|name| !assets.textures.contains_key(name))
    {
        "图片无法显示"
    } else {
        &node.text
    }
}

fn spawn_node(
    commands: &mut Commands,
    index: usize,
    node: &GraphNode,
    assets: &NodeAssets,
    light: bool,
) {
    let NodeAssets {
        font,
        corners,
        textures,
    } = assets;
    let palette = theme::palette(light);
    let size = Vec2::new(node.rect[2], node.rect[3]);
    let section = node.kind == "Section";
    let root = commands
        .spawn((
            NodeVisual(index),
            NodeIdentity(node.id.clone(), node.kind.clone()),
            TrailPosition(center(node)),
            Transform::from_translation(center(node).extend(if section { -5.0 } else { 1.0 })),
            Visibility::default(),
        ))
        .id();
    if node.kind == "PenStroke" {
        return;
    }
    let fill = if node.color[3] > 0.0 {
        color(node.color)
    } else if section {
        palette.surface.with_alpha(0.45)
    } else {
        palette.surface
    };
    commands.entity(root).with_children(|parent| {
        if let Some(texture) = node.attachment.as_ref().and_then(|name| textures.get(name)) {
            parent.spawn((
                Sprite {
                    image: texture.clone(),
                    custom_size: Some(size),
                    ..default()
                },
                NodePart {
                    index,
                    border: None,
                },
                Transform::default(),
            ));
        } else {
            parent.spawn((
                theme::sprite(corners, fill, size, false),
                Transform::from_xyz(0.0, 0.0, 0.1),
                NodePart {
                    index,
                    border: None,
                },
            ));
        }
        let border = palette.border;
        if node.attachment.is_none() {
            parent.spawn((
                NodePart {
                    index,
                    border: Some(0),
                },
                theme::sprite(corners, border, size, true),
                Transform::from_xyz(0.0, 0.0, 0.15),
            ));
        }
        let label = node_display_text(node, assets);
        if !label.is_empty() || matches!(node.kind.as_str(), "TextNode" | "Section") {
            let math = node.kind == "LatexNode";
            let y = if math {
                0.0
            } else {
                size.y * 0.5 - padding(node)
            };
            let mut label_entity = parent.spawn((
                NodeLabel(index),
                MsdfText::new(label),
                TextFont {
                    font: font.clone().into(),
                    font_size: FontSize::Px(if section { 30.0 } else { node.font_size }),
                    ..default()
                },
                TextColor(palette.text),
                node_text_layout(node),
                bevy::text::LineHeight::RelativeToFont(1.5),
                TextBounds {
                    width: text_width(node),
                    height: None,
                },
                if math {
                    Anchor::CENTER
                } else {
                    Anchor::TOP_CENTER
                },
                Transform::from_xyz(0.0, y, 0.2),
            ));
            if math {
                label_entity.insert(math_text::MathText);
            }
        }
    });
}
#[allow(clippy::too_many_arguments)]
fn sync_nodes(
    mut commands: Commands,
    mut scene: ResMut<Scene>,
    mut assets: ResMut<NodeAssets>,
    mut editor: ResMut<editor::Editor>,
    mut images: ResMut<Assets<Image>>,
    config: Res<Config>,
    mut interaction: ResMut<Interaction>,
    visuals: Query<(Entity, &NodeVisual, &NodeIdentity)>,
    mut previous: Local<(u64, Vec<(String, String)>)>,
) {
    if let Some(selection) = scene.0.take_restored_selection() {
        interaction.selected = scene
            .0
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| selection.nodes.contains(&n.id))
            .map(|(i, _)| i)
            .collect();
        interaction.selected_edges = selection.edges;
        interaction.selection_revision = scene.0.revision;
    }
    if previous.0 == scene.0.revision && previous.1.len() == scene.0.nodes.len() {
        return;
    }
    previous.0 = scene.0.revision;
    if previous
        .1
        .iter()
        .map(|(id, kind)| (id.as_str(), kind.as_str()))
        .eq(scene
            .0
            .nodes
            .iter()
            .map(|n| (n.id.as_str(), n.kind.as_str())))
    {
        return;
    }
    let selected: HashSet<_> = interaction
        .selected
        .iter()
        .filter_map(|i| previous.1.get(*i).map(|(id, _)| id))
        .cloned()
        .collect();
    previous.1 = scene
        .0
        .nodes
        .iter()
        .map(|n| (n.id.clone(), n.kind.clone()))
        .collect();
    let mut existing = HashSet::new();
    for (entity, visual, identity) in &visuals {
        if scene
            .0
            .nodes
            .get(visual.0)
            .is_none_or(|node| node.id != identity.0 || node.kind != identity.1)
        {
            commands.entity(entity).despawn();
        } else {
            existing.insert(visual.0);
        }
    }
    for (index, node) in scene.0.nodes.iter().enumerate() {
        if !existing.contains(&index) {
            if let Some(name) = &node.attachment
                && !assets.textures.contains_key(name)
            {
                let decoded = scene
                    .0
                    .attachments
                    .get(name)
                    .ok_or_else(|| "图片附件不存在".into())
                    .and_then(|bytes| attachment_pixels(name, bytes, &mut None, &config.font));
                match decoded {
                    Ok((width, height, rgba)) => {
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
                        assets.textures.insert(name.clone(), images.add(image));
                    }
                    Err(error) => editor
                        .report_error(format!("图片无法显示（{name}）：{error}。原始附件仍保留。")),
                }
            }
            spawn_node(&mut commands, index, node, &assets, editor.light);
        }
    }
    // Appending a group keeps existing indices; preserve the new selection made
    // by Ctrl+G. Removing/reordering nodes must not select a successor by index.
    if interaction.selection_revision != scene.0.revision && !selected.is_empty() {
        interaction.selected = scene
            .0
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| selected.contains(&n.id))
            .map(|(i, _)| i)
            .collect();
    }
    interaction.selected.retain(|i| *i < scene.0.nodes.len());
    interaction
        .selected_edges
        .retain(|id| scene.0.edges.iter().any(|e| &e.id == id));
    interaction.selection_revision = scene.0.revision;
    interaction.moving.clear();
    interaction.linking = None;
    interaction.last_click = None;
    interaction.dirty = true;
}
fn refresh_groups(
    mut scene: ResMut<Scene>,
    mut interaction: ResMut<Interaction>,
    mut editor: ResMut<editor::Editor>,
    mut revision: Local<Option<u64>>,
) {
    if !interaction.dirty && *revision == Some(scene.0.revision) {
        return;
    }
    let changed = scene.0.refresh_group_bounds();
    *revision = Some(scene.0.revision);
    match changed {
        Ok(changed) => interaction.dirty |= changed,
        Err(error) => editor.report_error(error.to_string()),
    }
}
fn fit_groups(
    scene: Res<Scene>,
    assets: Res<NodeAssets>,
    interaction: Res<Interaction>,
    mut parts: Query<(&NodePart, &mut Sprite)>,
    mut labels: Query<(&NodeLabel, &mut Transform, &mut TextBounds)>,
    mut revision: Local<Option<u64>>,
) {
    if !interaction.dirty && *revision == Some(scene.0.revision) {
        return;
    }
    *revision = Some(scene.0.revision);
    // Visual indices are valid only after sync_nodes has reconciled deletions.
    for (part, mut sprite) in &mut parts {
        let node = &scene.0.nodes[part.index];
        if node.kind != "Section" {
            continue;
        }
        let size = Vec2::new(node.rect[2], node.rect[3]);
        theme::resize(&mut sprite, &assets.corners, size, part.border.is_some());
    }
    for (label, mut transform, mut bounds) in &mut labels {
        let node = &scene.0.nodes[label.0];
        if node.kind != "Section" {
            continue;
        }
        let y = node.rect[3] * 0.5 - padding(node);
        if transform.translation.y != y {
            transform.translation.y = y;
        }
        let width = text_width(node);
        if bounds.width != width {
            bounds.width = width;
        }
    }
}
fn world_cursor(cursor: Vec2, window: &Window, transform: &Transform, scale: f32) -> Vec2 {
    transform.translation.truncate()
        + Vec2::new(
            cursor.x - window.width() * 0.5,
            window.height() * 0.5 - cursor.y,
        ) * scale
}

fn first_canvas_pass(
    frame: Res<bevy::diagnostic::FrameCount>,
    mut previous: Local<Option<u32>>,
) -> bool {
    let first = *previous != Some(frame.0);
    *previous = Some(frame.0);
    first
}
fn canvas_camera_transform(home: Vec2) -> Transform {
    // Camera2d's default depth range is -1000..1000; keep background groups inside it.
    Transform::from_translation(home.extend(0.0))
}
fn contains(node: &GraphNode, p: Vec2) -> bool {
    Aabb2d::new(center(node), Vec2::new(node.rect[2], node.rect[3]) * 0.5)
        .contains(&Aabb2d::new(p, Vec2::ZERO))
}
#[allow(clippy::too_many_arguments)]
fn input(
    window: Single<&Window, With<PrimaryWindow>>,
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    mut camera: Single<(&mut Transform, &mut Projection), With<Camera2d>>,
    mut scene: ResMut<Scene>,
    display: Option<Res<sections::Display>>,
    mut interaction: ResMut<Interaction>,
    config: Res<Config>,
    mut editor: ResMut<editor::Editor>,
    mut physics: ResMut<physics::PhysicsEditor>,
    paths: Res<edges::EdgePaths>,
    mut effects: ResMut<effects::Effects>,
) {
    if config.benchmark {
        return;
    }
    interaction.selected.retain(|i| *i < scene.0.nodes.len());
    if !window.focused {
        effects.clear();
        interaction.context_press = None;
        interaction.creation.cancel();
        interaction.cutting.cancel();
        interaction.jump.stop(&mut scene.0);
        interaction.linking = None;
        interaction.screen_cursor = None;
        if physics.running {
            physics.finish(&mut scene.0);
        } else if !physics.running {
            scene.0.finish();
        }
        interaction.moving.clear();
        interaction.drag_start = None;
        interaction.selecting = false;
        return;
    }
    if !mouse.pressed(MouseButton::Left) && (physics.dragging || !interaction.moving.is_empty()) {
        if physics.enabled {
            physics.release();
        } else {
            scene.0.finish();
        }
        interaction.moving.clear();
        interaction.drag_start = None;
    }
    if editor.capture {
        interaction.context_press = None;
        interaction.creation.cancel();
        interaction.cutting.cancel();
        interaction.jump.stop(&mut scene.0);
        if mouse.just_released(MouseButton::Left) || mouse.just_released(MouseButton::Right) {
            interaction.linking = None;
        }
        interaction.screen_cursor = None;
        if mouse.just_released(MouseButton::Left) {
            interaction.selecting = false;
            interaction.drag_start = None;
        }
        return;
    }
    let (transform, projection) = &mut *camera;
    let Projection::Orthographic(projection) = &mut **projection else {
        return;
    };
    physics.drag_scale = projection.scale;
    if keys.just_pressed(KeyCode::KeyF)
        && !keys.any_pressed([
            KeyCode::ControlLeft,
            KeyCode::ControlRight,
            KeyCode::SuperLeft,
            KeyCode::SuperRight,
        ])
    {
        let (home, scale) = fit_view(&scene.0, window.width(), window.height(), editor.ui_scale);
        transform.translation.x = home.x;
        transform.translation.y = home.y;
        projection.scale = scale;
    }
    if keys.just_pressed(KeyCode::KeyA)
        && keys.any_pressed([
            KeyCode::ControlLeft,
            KeyCode::ControlRight,
            KeyCode::SuperLeft,
            KeyCode::SuperRight,
        ])
    {
        interaction.selected = (0..scene.0.nodes.len())
            .filter(|index| {
                display
                    .as_deref()
                    .is_none_or(|display| display.visible(*index))
            })
            .collect();
    }
    if keys.just_pressed(KeyCode::Escape) {
        interaction.creation.cancel();
        interaction.cutting.cancel();
        interaction.jump = jump::Gesture::cancelled();
        interaction.linking = None;
        editor.connect_mode = false;
        physics.cancel(&mut scene.0);
        interaction.dirty = true;
        interaction.selected.clear();
        interaction.selected_edges.clear();
        interaction.moving.clear();
        interaction.drag_start = None;
        return;
    }
    let command = keys.pressed(KeyCode::ControlLeft)
        || keys.pressed(KeyCode::ControlRight)
        || keys.pressed(KeyCode::SuperLeft)
        || keys.pressed(KeyCode::SuperRight);
    let tool = if command && keys.just_pressed(KeyCode::KeyF) {
        Some(canvas_tools::Command::Search)
    } else if keys.just_pressed(KeyCode::Enter)
        && keys.any_pressed([KeyCode::AltLeft, KeyCode::AltRight])
    {
        Some(canvas_tools::Command::Properties)
    } else if keys.just_pressed(KeyCode::F2) {
        Some(canvas_tools::Command::Edit)
    } else if keys.just_pressed(KeyCode::Delete) {
        Some(canvas_tools::Command::Delete)
    } else if command && keys.just_pressed(KeyCode::KeyR) {
        Some(canvas_tools::Command::Reverse)
    } else if command && keys.just_pressed(KeyCode::KeyL) {
        Some(canvas_tools::Command::ToggleLock)
    } else if command && keys.just_pressed(KeyCode::KeyE) {
        Some(canvas_tools::Command::OpenUrl)
    } else if command && keys.just_pressed(KeyCode::KeyC) {
        Some(canvas_tools::Command::Copy)
    } else if command && keys.just_pressed(KeyCode::KeyX) {
        Some(canvas_tools::Command::Cut)
    } else if command && keys.just_pressed(KeyCode::KeyV) {
        Some(canvas_tools::Command::Paste)
    } else {
        None
    };
    if let Some(tool) = tool {
        let point = window
            .cursor_position()
            .map(|cursor| world_cursor(cursor, &window, transform, projection.scale))
            .unwrap_or(transform.translation.truncate());
        if matches!(tool, canvas_tools::Command::Paste) {
            physics.finish(&mut scene.0);
            interaction.creation.cancel();
            interaction.cutting.cancel();
            interaction.jump.stop(&mut scene.0);
            scene.0.selection = project_graph_bevy_canvas::document::Selection::capture(
                &scene.0,
                &interaction.selected,
                &interaction.selected_edges,
            );
            editor.importing = Some(image_import::Pending::paste(
                [point.x, -point.y],
                512.0 * projection.scale,
                config.font.clone(),
            ));
            return;
        }
        if let Err(error) = canvas_tools::execute(
            tool,
            point,
            &mut scene.0,
            &mut interaction,
            &mut editor,
            &mut physics,
            &mut effects,
        ) {
            editor.report_error(error.to_string());
        }
        return;
    }
    if command && keys.just_pressed(KeyCode::KeyG) {
        interaction.creation.cancel();
        interaction.cutting.cancel();
        interaction.jump.stop(&mut scene.0);
        create_group(&mut scene.0, &mut interaction, &mut editor, &mut physics);
        return;
    }
    if command && keys.just_pressed(KeyCode::KeyZ) {
        interaction.creation.cancel();
        interaction.cutting.cancel();
        interaction.jump.stop(&mut scene.0);
        interaction.linking = None;
        physics.finish(&mut scene.0);
        if keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight) {
            scene.0.redo();
        } else {
            scene.0.undo();
        }
        interaction.dirty = true;
        interaction.moving.clear();
        interaction.drag_start = None;
        return;
    }
    let Some(cursor) = window.cursor_position() else {
        interaction.creation.cancel();
        interaction.cutting.cancel();
        interaction.jump.stop(&mut scene.0);
        interaction.linking = None;
        interaction.last_cursor = None;
        interaction.screen_cursor = None;
        return;
    };
    let previous_screen = interaction.screen_cursor.replace(cursor);
    let p = world_cursor(cursor, &window, transform, projection.scale);
    if mouse.just_pressed(MouseButton::Right) {
        interaction.context_press = Some(cursor);
    }
    if interaction
        .context_press
        .is_some_and(|start| start.distance_squared(cursor) > 16.0)
    {
        interaction.context_press = None;
    }
    if mouse.just_released(MouseButton::Right) && interaction.context_press.take().is_some() {
        if let Some(index) = editor
            .overview
            .hit(&scene.0, p)
            .filter(|index| {
                display
                    .as_deref()
                    .is_none_or(|display| display.visible(*index))
            })
            .or_else(|| sections::hit(display.as_deref(), &scene, p, false))
        {
            if !interaction.selected.contains(&index) {
                interaction.selected = HashSet::from([index]);
                interaction.selected_edges.clear();
            }
        } else if let Some(index) = paths.nearest(&scene.0, p, 6.0 * projection.scale) {
            let id = scene.0.edges[index].id.clone();
            if !interaction.selected_edges.contains(&id) {
                interaction.selected.clear();
                interaction.selected_edges = HashSet::from([id]);
            }
        } else {
            interaction.selected.clear();
            interaction.selected_edges.clear();
        }
        interaction.cutting.cancel();
        interaction.linking = None;
        let scale = editor.ui_scale.max(0.75);
        editor.tools.context = Some((bevy_egui::egui::pos2(cursor.x / scale, cursor.y / scale), p));
        editor.capture = true;
        return;
    }
    let view = Vec3::new(
        transform.translation.x,
        transform.translation.y,
        projection.scale,
    );
    interaction.creation.observe(cursor, view);
    let create_allowed = !editor.connect_mode
        && editor.overview.hit(&scene.0, p).is_none()
        && !mouse.pressed(MouseButton::Right)
        && !mouse.pressed(MouseButton::Middle)
        && !keys.any_pressed([
            KeyCode::Space,
            KeyCode::KeyE,
            KeyCode::AltLeft,
            KeyCode::AltRight,
            KeyCode::ControlLeft,
            KeyCode::ControlRight,
            KeyCode::SuperLeft,
            KeyCode::SuperRight,
            KeyCode::ShiftLeft,
            KeyCode::ShiftRight,
        ]);
    if !create_allowed {
        interaction.creation.cancel();
    }
    if cutting::input(
        &mouse,
        &keys,
        p,
        projection.scale,
        &mut scene.0,
        display.as_deref(),
        &mut interaction,
        &mut physics,
        &mut editor,
        &paths,
        &mut effects,
    ) {
        return;
    }
    if jump::input(
        &keys,
        &mouse,
        p,
        &mut scene.0,
        display.as_deref(),
        &mut interaction,
        &mut physics,
        &mut editor,
    ) {
        return;
    }
    let right_link = mouse.just_pressed(MouseButton::Right)
        && sections::hit(display.as_deref(), &scene, p, false).is_some();
    let left_link = (editor.connect_mode || keys.pressed(KeyCode::KeyE))
        && mouse.just_pressed(MouseButton::Left)
        && !keys.pressed(KeyCode::Space);
    if interaction.linking.is_some() || right_link || left_link {
        if interaction.linking.is_none() && (right_link || left_link) {
            physics.finish(&mut scene.0);
            interaction.link_start = cursor;
            interaction.link_right = right_link;
            if let Some(hit) = sections::hit(display.as_deref(), &scene, p, false) {
                interaction.linking = Some((
                    hit,
                    sections::connection_rate(display.as_deref(), &scene, hit, p),
                ));
            }
            interaction.moving.clear();
            interaction.selecting = false;
            interaction.drag_start = None;
            interaction.last_click = None;
        }
        let button = if interaction.link_right {
            MouseButton::Right
        } else {
            MouseButton::Left
        };
        if mouse.just_released(button) {
            if let Some((from, rate)) = interaction.linking.take()
                && cursor.distance_squared(interaction.link_start) >= 16.0
                && let Some(to) = sections::hit(display.as_deref(), &scene, p, false)
            {
                let edge = project_graph_bevy_canvas::GraphEdge {
                    id: uuid::Uuid::new_v4().to_string(),
                    members: vec![from, to],
                    rates: vec![
                        rate,
                        sections::connection_rate(display.as_deref(), &scene, to, p),
                    ],
                    ..default()
                };
                if let Err(e) = scene.0.add_edge(edge) {
                    editor.report_error(e.to_string());
                } else {
                    match edges::connection_points(
                        &scene.0.graph,
                        display.as_deref().map(|display| display.graph(&scene)),
                        scene.0.edges.len() - 1,
                        projection.scale,
                    ) {
                        Ok(points) => effects.connected(points),
                        Err(error) => editor.report_error(format!("连线反馈生成失败：{error}")),
                    }
                    interaction.dirty = true;
                }
            }
        } else if !mouse.pressed(button) {
            interaction.linking = None;
        }
        interaction.last_cursor = Some(p);
        return;
    }
    let pan = mouse.pressed(MouseButton::Middle)
        || (interaction.selected.is_empty()
            && keys.pressed(KeyCode::Space)
            && mouse.pressed(MouseButton::Left));
    if pan && let Some(previous) = previous_screen {
        let delta = cursor - previous;
        transform.translation += Vec2::new(-delta.x, delta.y).extend(0.0) * projection.scale;
    }
    if mouse.just_pressed(MouseButton::Left) && !keys.pressed(KeyCode::Space) {
        interaction.drag_start = Some(p);
        let hit = editor
            .overview
            .hit(&scene.0, p)
            .filter(|index| {
                display
                    .as_deref()
                    .is_none_or(|display| display.visible(*index))
            })
            .or_else(|| sections::hit(display.as_deref(), &scene, p, true));
        if let Some(hit) = hit {
            if !keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]) {
                interaction.selected_edges.clear();
            }
            interaction.creation.cancel();
            if interaction.last_click.is_some_and(|(i, t, screen)| {
                i == hit && t.elapsed().as_millis() < 350 && screen.distance_squared(cursor) < 25.0
            }) && scene.0.nodes[hit].kind == "UrlNode"
            {
                physics.finish(&mut scene.0);
                if let Err(error) = editor.open_urls(&scene.0, &HashSet::from([hit])) {
                    editor.report_error(error.to_string());
                }
                interaction.moving.clear();
                interaction.drag_start = None;
                interaction.last_click = None;
                return;
            }
            if interaction.last_click.is_some_and(|(i, t, screen)| {
                i == hit && t.elapsed().as_millis() < 350 && screen.distance_squared(cursor) < 25.0
            }) && scene.0.can_edit_text(hit)
            {
                physics.finish(&mut scene.0);
                editor.begin(&scene.0.nodes[hit]);
                let node = &scene.0.nodes[hit];
                effects.begin_edit(
                    center(node),
                    Vec2::new(node.rect[2], node.rect[3]),
                    projection.scale > 0.25 && projection.scale < 10.0,
                );
                interaction.moving.clear();
                interaction.drag_start = None;
                interaction.last_click = None;
                return;
            }
            interaction.last_click = Some((hit, Instant::now(), cursor));
            if keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight) {
                if !interaction.selected.insert(hit) {
                    interaction.selected.remove(&hit);
                }
            } else if !interaction.selected.contains(&hit) {
                interaction.selected.clear();
                interaction.selected.insert(hit);
            }
            interaction.moving = scene.0.expand_selection(&interaction.selected);
            if physics.enabled {
                physics.begin(&mut scene.0, &interaction.selected);
            } else if !interaction.moving.is_empty() {
                scene.0.begin("移动节点");
            }
            interaction.selecting = false;
            interaction.last_cursor = Some(p);
        } else if let Some(index) = paths.nearest(&scene.0, p, 6.0 * projection.scale) {
            if !keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]) {
                interaction.selected.clear();
                interaction.selected_edges.clear();
            }
            let id = scene.0.edges[index].id.clone();
            if !interaction.selected_edges.insert(id.clone()) {
                interaction.selected_edges.remove(&id);
            }
            interaction.creation.cancel();
            interaction.selecting = false;
            interaction.drag_start = None;
            interaction.last_click = None;
            interaction.dirty = true;
        } else {
            interaction.last_click = None;
            if create_allowed
                && !cutting::occupied(&scene.0, display.as_deref(), &paths, p, projection.scale)
            {
                interaction.creation.press(cursor, view);
            } else {
                interaction.creation.cancel();
            }
            if !keys.pressed(KeyCode::ShiftLeft) && !keys.pressed(KeyCode::ShiftRight) {
                interaction.selected.clear();
                interaction.selected_edges.clear();
            }
            interaction.selecting = true;
        }
    }
    if mouse.pressed(MouseButton::Left)
        && !pan
        && !interaction.selecting
        && !interaction.moving.is_empty()
        && let Some(previous) = interaction.last_cursor
    {
        let delta = p - previous;
        if physics.enabled {
            physics.target += delta;
        } else {
            move_nodes(&mut scene.0, &interaction.moving, delta);
            interaction.dirty |= delta.length_squared() > 0.0;
        }
    }
    if mouse.just_released(MouseButton::Left) {
        if physics.dragging {
            physics.release();
        } else if !physics.running {
            scene.0.finish();
        }
        if interaction.selecting
            && let Some(start) = interaction.drag_start
        {
            let graph = display
                .as_deref()
                .map_or(&scene.0.graph, |display| display.graph(&scene));
            for (i, n) in graph.nodes.iter().enumerate() {
                if display.as_deref().is_none_or(|display| display.visible(i))
                    && marquee_hit(n, start, p)
                {
                    interaction.selected.insert(i);
                }
            }
        }
        interaction.drag_start = None;
        interaction.moving.clear();
        interaction.selecting = false;
        if interaction.creation.release(
            cursor,
            view,
            Instant::now(),
            create_allowed
                && !cutting::occupied(&scene.0, display.as_deref(), &paths, p, projection.scale),
        ) {
            creation::create(
                p,
                &mut scene.0,
                &mut interaction,
                &mut editor,
                &mut physics,
                &mut effects,
            );
        }
    }
    interaction.last_cursor = Some(p);
}
fn marquee_hit(node: &GraphNode, start: Vec2, end: Vec2) -> bool {
    let lo = start.min(end);
    let hi = start.max(end);
    let marquee = Aabb2d::new((lo + hi) * 0.5, (hi - lo) * 0.5);
    let bounds = Aabb2d::new(center(node), Vec2::new(node.rect[2], node.rect[3]) * 0.5);
    if node.kind == "Section" && !node.collapsed {
        // Enclosing a group selects its frame too, preserving nested membership
        // when regrouping. A marquee inside a group selects only its contents.
        marquee.contains(&bounds)
    } else {
        marquee.intersects(&bounds)
    }
}
fn create_group(
    doc: &mut Document,
    interaction: &mut Interaction,
    editor: &mut editor::Editor,
    physics: &mut physics::PhysicsEditor,
) {
    doc.selection = project_graph_bevy_canvas::document::Selection::capture(
        doc,
        &interaction.selected,
        &interaction.selected_edges,
    );
    physics.finish(doc);
    match doc.create_group(&interaction.selected) {
        Ok(index) => {
            interaction.selected = HashSet::from([index]);
            interaction.moving.clear();
            interaction.drag_start = None;
            interaction.selecting = false;
            interaction.linking = None;
            interaction.last_click = None;
            interaction.dirty = true;
        }
        Err(error) => editor.report_error(error.to_string()),
    }
}
fn move_nodes(graph: &mut Graph, selected: &HashSet<usize>, delta: Vec2) {
    for i in selected {
        let n = &mut graph.nodes[*i];
        n.rect[0] += delta.x;
        n.rect[1] -= delta.y;
    }
}
fn update_visuals(
    scene: Res<Scene>,
    display: Option<Res<sections::Display>>,
    physics: Res<physics::PhysicsEditor>,
    mut interaction: ResMut<Interaction>,
    mut nodes: Query<(&NodeVisual, &mut Transform)>,
) {
    if !interaction.dirty {
        return;
    }
    for (node, mut transform) in &mut nodes {
        let graph = display
            .as_deref()
            .map_or(&scene.0.graph, |display| display.graph(&scene));
        let c = physics::visual_center(&physics, node.0, &graph.nodes[node.0]);
        let translation = c.extend(transform.translation.z);
        if transform.translation != translation {
            transform.translation = translation;
        }
    }
    interaction.dirty = false;
}
fn movement_effects(
    scene: Res<Scene>,
    interaction: Res<Interaction>,
    physics: Res<physics::PhysicsEditor>,
    mut effects: ResMut<effects::Effects>,
    mut nodes: Query<(&NodeVisual, &Transform, &mut TrailPosition), Changed<Transform>>,
) {
    let moving = physics.running || !interaction.moving.is_empty();
    for (visual, transform, mut previous) in &mut nodes {
        let position = transform.translation.truncate();
        let delta = position - previous.0;
        previous.0 = position;
        if moving
            && let Some(node) = scene.0.nodes.get(visual.0)
            && matches!(node.kind.as_str(), "TextNode" | "Section" | "UrlNode")
        {
            effects.move_trail(position, Vec2::new(node.rect[2], node.rect[3]), delta);
        }
    }
}
#[allow(clippy::too_many_arguments)]
fn selection_overlay(
    scene: Res<Scene>,
    display: Option<Res<sections::Display>>,
    interaction: Res<Interaction>,
    editor: Res<editor::Editor>,
    physics: Res<physics::PhysicsEditor>,
    paths: Res<edges::EdgePaths>,
    camera: Query<&Projection, With<Camera2d>>,
    export: Option<Res<export_image::ExportImage>>,
    mut effects: ResMut<effects::Effects>,
    mut gizmos: Gizmos,
) {
    cutting::preview(
        &interaction.cutting,
        &scene.0,
        display.as_deref(),
        &paths,
        &mut effects,
    );
    for id in &interaction.selected_edges {
        if let Some(path) = paths.0.get(id) {
            for [a, b] in &path.segments {
                gizmos.line_2d(
                    *a + path.anchor,
                    *b + path.anchor,
                    theme::palette(editor.light).accent,
                );
            }
        }
    }
    jump::preview(&interaction.jump, &mut gizmos);
    if let (Some((from, rate)), Some(cursor)) = (interaction.linking, interaction.last_cursor) {
        edges::preview(
            display
                .as_deref()
                .map_or(&scene.0.graph, |display| display.graph(&scene)),
            from,
            rate,
            cursor,
            sections::hit(display.as_deref(), &scene, cursor, false),
            &mut gizmos,
            theme::palette(editor.light).accent,
            theme::group_scale(camera.single().ok(), export.is_some()).unwrap_or(1.0),
        );
    }
    for i in &interaction.selected {
        if display
            .as_deref()
            .is_some_and(|display| !display.visible(*i))
        {
            continue;
        }
        let graph = display
            .as_deref()
            .map_or(&scene.0.graph, |display| display.graph(&scene));
        let n = &graph.nodes[*i];
        if editor.is_editing(n) {
            continue;
        }
        if n.attachment.is_some() {
            gizmos.rect_2d(
                Isometry2d::from_translation(physics::visual_center(&physics, *i, n)),
                Vec2::new(n.rect[2], n.rect[3]),
                theme::palette(editor.light).accent,
            );
            continue;
        }
        let size = Vec2::new(n.rect[2], n.rect[3]);
        let position = physics::visual_center(&physics, *i, n);
        let accent = theme::palette(editor.light).accent;
        if matches!(
            n.kind.as_str(),
            "Section" | "TextNode" | "UrlNode" | "LatexNode"
        ) && !n.collapsed
            && let Some(scale) = theme::group_scale(camera.single().ok(), export.is_some())
        {
            gizmos.linestrip_2d(
                theme::corners::outline_with_radius(
                    position,
                    size,
                    theme::corners::group_radius(size, scale),
                ),
                accent,
            );
        } else {
            theme::outline(&mut gizmos, position, size, accent);
        }
    }
    if interaction.selecting
        && let (Some(start), Some(end)) = (interaction.drag_start, interaction.last_cursor)
    {
        gizmos.rect_2d(
            Isometry2d::from_translation((start + end) * 0.5),
            (end - start).abs(),
            theme::palette(editor.light).accent,
        );
    }
}
fn status(
    time: Res<Time>,
    mut window: Single<&mut Window, With<PrimaryWindow>>,
    scene: Res<Scene>,
    mut elapsed: Local<f32>,
) {
    *elapsed += time.delta_secs();
    if *elapsed < 0.5 {
        return;
    }
    *elapsed = 0.0;
    window.title = format!(
        "{}{} · Ravomi",
        if scene.0.dirty() { "● " } else { "" },
        scene
            .0
            .path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|p| p.to_string_lossy())
            .unwrap_or_else(|| "未命名工作区".into()),
    );
}
#[allow(clippy::too_many_arguments)]
fn benchmark(
    mut commands: Commands,
    config: Res<Config>,
    mut measurement: ResMut<Measurement>,
    mut scene: ResMut<Scene>,
    mut interaction: ResMut<Interaction>,
    mut camera: Single<(&mut Transform, &mut Projection), With<Camera2d>>,
    mut exit: MessageWriter<AppExit>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut physics: ResMut<physics::PhysicsEditor>,
    mut counters: ResMut<stress::Counters>,
    handles: Query<&bevy::window::RawHandleWrapper, With<PrimaryWindow>>,
    adapter: Option<Res<bevy::render::renderer::RenderAdapterInfo>>,
    cache: Res<msdf_text::MsdfCache>,
) {
    let since_start = measurement.start.elapsed().as_secs_f64();
    let msdf = cache.status();
    let invalid = if msdf.failed_labels > 0 {
        Some("MSDF text could not be prepared completely")
    } else if !msdf.ready() && (!measurement.samples.is_empty() || !measurement.results.is_empty())
    {
        Some("MSDF text became unready during steady-state sampling")
    } else if !msdf.ready() && since_start > 120.0 {
        Some("MSDF text readiness timed out after 120 seconds")
    } else {
        None
    };
    if (config.benchmark || config.preview)
        && let Some(reason) = invalid
    {
        let report = serde_json::json!({"valid":false,"error":reason,"load_ms":since_start*1000.0,"msdf":stress::msdf_status(&msdf),"phases":[],"environment":stress::environment(&window, handles.single().ok(), adapter.as_deref())});
        if let Err(error) = stress::write_report(&config.output, &report) {
            error!("Cannot write failed benchmark report: {error}");
        }
        error!("{reason}");
        exit.write(AppExit::error());
        return;
    }
    let Some(elapsed) = measurement.warmup.elapsed(since_start, &msdf) else {
        measurement.previous = Instant::now();
        counters.clear_phase();
        physics.samples.clear();
        return;
    };
    let elapsed = elapsed as f32;
    let screenshot_after = if config.benchmark { 14.0 } else { 3.0 };
    if !measurement.screenshot_taken && elapsed > screenshot_after {
        if let Some(path) = &config.screenshot {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).expect("Screenshot directory");
            }
            commands
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk(path.clone()));
        }
        measurement.screenshot_taken = true;
    }
    if !config.benchmark {
        if config.preview && elapsed > 4.0 {
            exit.write(AppExit::Success);
        }
        return;
    }
    // Warm up font atlases, pipeline compilation and texture uploads before sampling.
    if elapsed < 4.0 {
        measurement.previous = Instant::now();
        counters.clear_phase();
        physics.samples.clear();
        return;
    }
    let phase = if config.edit_node.is_some() {
        // Editing captures navigation. Measure a stationary editor for 16 seconds
        // instead of bypassing that contract with scripted camera movement.
        if elapsed >= 20.0 { 4 } else { 0 }
    } else {
        (((elapsed - 4.0) / 4.0).floor() as usize).min(4)
    };
    if phase != measurement.phase {
        let mut samples = std::mem::take(&mut measurement.samples);
        samples.sort_by(f64::total_cmp);
        if !samples.is_empty() {
            let names = [
                if config.edit_node.is_some() {
                    "edit_idle"
                } else {
                    "idle"
                },
                "pan",
                "zoom",
                if config.physics {
                    "physics_drag"
                } else {
                    "drag_all"
                },
            ];
            let mean = samples.iter().sum::<f64>() / samples.len() as f64;
            let render: std::collections::BTreeMap<_, _> = counters
                .render
                .iter()
                .map(|(name, values)| (name, stress::summary(values)))
                .collect();
            let result = serde_json::json!({"phase":names[measurement.phase],"frames":samples.len(),"mean_ms":mean,"p95_ms":samples[(samples.len() as f64*0.95) as usize],"p99_ms":samples[(samples.len() as f64*0.99) as usize],"fps":1000.0/mean,"text_layouts":counters.text_layouts,"physics_steps":stress::summary(&physics.samples),"max_active":counters.max_active,"sleeping":physics.sleeping,"candidate_pairs":physics.candidate_pairs,"contacts":physics.contacts,"rss_kib":stress::memory_kib(),"render_spans_ms":render});
            let mut result = result;
            result["pss_kib"] = stress::proportional_memory_kib().into();
            result["app_schedule_ms"] = stress::summary(&counters.app_ms);
            result["msdf_mesh_rebuilds"] = counters.msdf_mesh_rebuilds.into();
            result["edge_mesh_rebuilds"] = counters.edge_mesh_rebuilds.into();
            result["msdf_generated_glyphs"] = counters.msdf_generated_glyphs.into();
            println!("BENCHMARK {result}");
            measurement.results.push(result);
            counters.clear_phase();
            physics.samples.clear();
        }
        measurement.phase = phase;
        if phase == 4 {
            let report = serde_json::json!({"valid":true,"renderer":"Bevy 0.19.1 / wgpu 29.0.4 / MSDF","load_ms":measurement.warmup.load_ms,"msdf":stress::msdf_status(&msdf),"window":[window.physical_width(),window.physical_height()],"environment":stress::environment(&window, handles.single().ok(), adapter.as_deref()),"timing":"wall-clock frame interval, not GPU time or input latency","continuous_redraw":true,"diagnostics":config.diagnostics,"ui_scale":config.ui_scale,"ui":!config.no_ui,"physics":config.physics,"editing":config.edit_node,"nodes":scene.0.nodes.len(),"edges":scene.0.edges.len(),"attachments":scene.0.attachments.len(),"counts":scene.0.counts,"phases":measurement.results});
            if let Err(error) = stress::write_report(&config.output, &report) {
                error!("Cannot write benchmark report: {error}");
                exit.write(AppExit::error());
                return;
            }
            exit.write(AppExit::Success);
            return;
        }
        if phase == 3 {
            interaction.selected = (0..scene.0.nodes.len())
                .filter(|i| scene.0.nodes[*i].kind != "Section" && !scene.0.nodes[*i].locked)
                .take(if config.physics { 8 } else { usize::MAX })
                .collect();
            if config.physics {
                physics.begin(&mut scene.0, &interaction.selected);
            }
        }
    }
    if phase >= 4 {
        return;
    }
    let now = Instant::now();
    let dt = now.duration_since(measurement.previous).as_secs_f64();
    measurement.samples.push(dt * 1000.0);
    measurement.previous = now;
    let (transform, projection) = &mut *camera;
    let Projection::Orthographic(projection) = &mut **projection else {
        return;
    };
    let t = elapsed - 4.0 - phase as f32 * 4.0;
    match phase {
        1 => {
            transform.translation.x = measurement.home.x + (t * 2.0).sin() * 700.0;
            transform.translation.y = measurement.home.y + (t * 2.0).cos() * 400.0;
        }
        2 => {
            projection.scale = measurement.home_scale * (0.55 + 0.45 * (t * 2.0).cos().abs());
        }
        3 => {
            let delta = Vec2::new((t * 2.0).cos(), (t * 2.0).sin()) * (dt as f32 * 160.0);
            if config.physics {
                physics.target += delta;
            } else {
                move_nodes(&mut scene.0, &interaction.selected, delta);
            }
            interaction.dirty = true;
        }
        _ => {}
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cut_last_node_refreshes_groups_before_stale_visuals_are_removed() {
        let mut doc = stress::synthetic(1, false, false, false).unwrap();
        let mut interaction = Interaction::default();
        let mut editor = editor::Editor::default();
        let mut physics = physics::PhysicsEditor::default();
        let mut effects = effects::Effects::default();
        let mut mouse = ButtonInput::default();
        let node = doc.nodes[0].clone();
        let origin = center(&node) - Vec2::X * (node.rect[2] + 100.0);
        for (pressed, cursor) in [(true, origin), (false, center(&node))] {
            mouse.clear();
            if pressed {
                mouse.press(MouseButton::Right);
            } else {
                mouse.release(MouseButton::Right);
            }
            assert!(cutting::input(
                &mouse,
                &default(),
                cursor,
                1.0,
                &mut doc,
                None,
                &mut interaction,
                &mut physics,
                &mut editor,
                &default(),
                &mut effects,
            ));
        }
        assert!(doc.nodes.is_empty());
        let mut images = Assets::<Image>::default();
        let mut app = App::new();
        app.insert_resource(Scene(doc))
            .insert_resource(interaction)
            .insert_resource(editor)
            .insert_resource(NodeAssets {
                font: default(),
                corners: theme::NodeCorners::new(&mut images),
                textures: default(),
            })
            .add_systems(Update, refresh_groups);
        // Input changes the document before PostUpdate reconciles these entities.
        app.world_mut().spawn((
            NodePart {
                index: 0,
                border: None,
            },
            Sprite::default(),
        ));
        app.world_mut()
            .spawn((NodeLabel(0), Transform::default(), TextBounds::default()));
        app.update();
        let mut scene = app.world_mut().resource_mut::<Scene>();
        scene.0.undo();
        assert_eq!(scene.0.nodes.len(), 1);
        scene.0.redo();
        assert!(scene.0.nodes.is_empty());
    }

    #[test]
    fn failed_attachment_label_survives_document_sync_without_changing_content() {
        let mut doc = stress::synthetic(1, false, false, false).unwrap();
        doc.nodes[0].kind = "ImageNode".into();
        doc.nodes[0].attachment = Some("broken.png".into());
        doc.nodes[0].text = "original title".into();
        doc.revision += 1;
        let mut images = Assets::<Image>::default();
        let assets = NodeAssets {
            font: Handle::default(),
            corners: theme::NodeCorners::new(&mut images),
            textures: HashMap::new(),
        };
        let mut app = App::new();
        app.insert_resource(Scene(doc))
            .insert_resource(assets)
            .init_resource::<editor::Editor>()
            .add_systems(Update, editor::sync_labels);
        let label = app
            .world_mut()
            .spawn((NodeLabel(0), MsdfText::new("stale")))
            .id();
        app.update();
        assert_eq!(
            app.world().get::<MsdfText>(label).unwrap().0,
            "图片无法显示"
        );
        assert_eq!(
            app.world().resource::<Scene>().0.nodes[0].text,
            "original title"
        );
        app.world_mut().resource_mut::<Scene>().0.revision += 1;
        app.update();
        assert_eq!(
            app.world().get::<MsdfText>(label).unwrap().0,
            "图片无法显示"
        );
    }

    #[test]
    fn attachment_decode_reports_corrupt_unsupported_and_oversized_images() {
        let mut options = Some(resvg::usvg::Options::default());
        for name in ["broken.png", "broken.svg", "unsupported.bin"] {
            assert!(attachment_pixels(name, b"not an image", &mut options, &[]).is_err());
        }
        let huge = br#"<svg xmlns="http://www.w3.org/2000/svg" width="9000" height="1"/>"#;
        assert!(
            attachment_pixels("huge.svg", huge, &mut options, &[])
                .unwrap_err()
                .to_string()
                .contains("超出显示限制")
        );
        let dense = br#"<svg xmlns="http://www.w3.org/2000/svg" width="8192" height="8192"/>"#;
        assert!(attachment_pixels("dense.svg", dense, &mut options, &[]).is_err());
        let mut png = std::io::Cursor::new(Vec::new());
        image::RgbaImage::new(8193, 1)
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        assert!(
            attachment_pixels("wide.png", png.get_ref(), &mut options, &[])
                .unwrap_err()
                .to_string()
                .contains("超出显示限制")
        );
    }

    #[test]
    fn attachment_decode_preserves_raster_pixels_and_unpremultiplies_svg() {
        let mut options = Some(resvg::usvg::Options::default());
        let mut png = std::io::Cursor::new(Vec::new());
        image::RgbaImage::from_pixel(2, 2, image::Rgba([32, 128, 200, 128]))
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        let decoded = attachment_pixels("test.png", png.get_ref(), &mut options, &[]).unwrap();
        assert_eq!((decoded.0, decoded.1), (2, 2));
        assert!(
            decoded
                .2
                .as_chunks::<4>()
                .0
                .iter()
                .all(|pixel| *pixel == [32, 128, 200, 128])
        );
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="2" height="2"><rect width="2" height="2" fill="red" opacity="0.5"/></svg>"#;
        let decoded = attachment_pixels("test.SVG", svg, &mut options, &[]).unwrap();
        assert_eq!((decoded.0, decoded.1), (2, 2));
        assert!(
            decoded
                .2
                .as_chunks::<4>()
                .0
                .iter()
                .all(|pixel| *pixel == [255, 0, 0, 128])
        );
    }

    #[test]
    #[ignore = "requires Noto Sans CJK; writes results/svg-font-fallback.png"]
    fn svg_attachment_renders_missing_and_generic_font_families() {
        let font =
            std::fs::read("/usr/share/fonts/google-noto-sans-cjk-fonts/NotoSansCJK-Regular.ttc")
                .unwrap();
        let options = attachment_svg_options(&font);
        let mut svg =
            String::from(r#"<svg xmlns="http://www.w3.org/2000/svg" width="240" height="240">"#);
        for (row, family) in [
            "GrapluMissingFontForRegression",
            "serif",
            "sans-serif",
            "monospace",
            "cursive",
            "fantasy",
        ]
        .iter()
        .enumerate()
        {
            svg.push_str(&format!(
                r#"<text x="8" y="{}" font-family="{family}" font-size="24" fill="white">中文 Abc</text>"#,
                row * 40 + 30,
            ));
        }
        svg.push_str("</svg>");
        let tree = resvg::usvg::Tree::from_data(svg.as_bytes(), &options).unwrap();
        let mut pixmap = resvg::tiny_skia::Pixmap::new(240, 240).unwrap();
        resvg::render(
            &tree,
            resvg::tiny_skia::Transform::identity(),
            &mut pixmap.as_mut(),
        );
        for row in 0..6 {
            let ink = pixmap.pixels()[row * 40 * 240..(row + 1) * 40 * 240]
                .iter()
                .filter(|pixel| pixel.alpha() > 128)
                .count();
            assert!(ink > 200, "SVG text row {row} disappeared: {ink} pixels");
        }
        std::fs::create_dir_all("results").unwrap();
        pixmap.save_png("results/svg-font-fallback.png").unwrap();
    }

    #[test]
    fn repeated_ui_passes_apply_canvas_input_only_once_per_frame() {
        #[derive(Resource, Default)]
        struct Calls(u32);
        let mut app = App::new();
        app.init_resource::<bevy::diagnostic::FrameCount>()
            .init_resource::<Calls>()
            .add_systems(
                EguiPrimaryContextPass,
                (|mut calls: ResMut<Calls>| calls.0 += 1).run_if(first_canvas_pass),
            );
        app.world_mut().run_schedule(EguiPrimaryContextPass);
        app.world_mut().run_schedule(EguiPrimaryContextPass);
        assert_eq!(app.world().resource::<Calls>().0, 1);
        app.world_mut()
            .resource_mut::<bevy::diagnostic::FrameCount>()
            .0 += 1;
        app.world_mut().run_schedule(EguiPrimaryContextPass);
        assert_eq!(app.world().resource::<Calls>().0, 2);
    }

    #[test]
    fn right_drag_connects_once_and_ui_capture_or_focus_loss_cancels() {
        let doc = stress::synthetic(2, false, false, false).unwrap();
        let mut app = App::new();
        app.insert_resource(Scene(doc))
            .init_resource::<editor::Editor>()
            .init_resource::<physics::PhysicsEditor>()
            .init_resource::<effects::Effects>()
            .init_resource::<edges::EdgePaths>()
            .init_resource::<Interaction>()
            .init_resource::<ButtonInput<MouseButton>>()
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<AccumulatedMouseScroll>()
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
            .add_systems(Update, input);
        let window = app
            .world_mut()
            .spawn((
                Window {
                    resolution: (1280, 800).into(),
                    ..default()
                },
                PrimaryWindow,
            ))
            .id();
        let camera = app
            .world_mut()
            .spawn((
                Camera2d,
                Projection::Orthographic(OrthographicProjection::default_2d()),
                Transform::from_xyz(0.0, 0.0, 1000.0),
            ))
            .id();
        let start = Vec2::new(760.0, 448.0);
        let end = Vec2::new(1032.0, 448.0);
        fn press(app: &mut App, window: Entity, p: Vec2) {
            app.world_mut()
                .resource_mut::<ButtonInput<MouseButton>>()
                .reset_all();
            app.world_mut()
                .get_mut::<Window>(window)
                .unwrap()
                .set_cursor_position(Some(p));
            app.world_mut()
                .resource_mut::<ButtonInput<MouseButton>>()
                .press(MouseButton::Right);
            app.update();
            app.world_mut()
                .resource_mut::<ButtonInput<MouseButton>>()
                .clear();
        }
        fn release(app: &mut App, window: Entity, p: Vec2) {
            app.world_mut()
                .get_mut::<Window>(window)
                .unwrap()
                .set_cursor_position(Some(p));
            app.world_mut()
                .resource_mut::<ButtonInput<MouseButton>>()
                .release(MouseButton::Right);
            app.update();
        }
        press(&mut app, window, start);
        release(&mut app, window, end);
        assert_eq!(app.world().resource::<Scene>().0.edges.len(), 2);
        assert_eq!(app.world().resource::<Scene>().0.edges[1].members, [0, 1]);
        assert!(app.world().resource::<effects::Effects>().active());
        assert_eq!(
            app.world().get::<Transform>(camera).unwrap().translation,
            Vec3::new(0.0, 0.0, 1000.0)
        );
        app.world_mut().resource_mut::<Scene>().0.undo();
        assert_eq!(app.world().resource::<Scene>().0.edges.len(), 1);
        app.world_mut().resource_mut::<Scene>().0.redo();
        assert_eq!(app.world().resource::<Scene>().0.edges.len(), 2);
        press(&mut app, window, start);
        release(&mut app, window, start);
        assert_eq!(app.world().resource::<Scene>().0.edges.len(), 2);
        press(&mut app, window, start);
        app.world_mut().resource_mut::<editor::Editor>().capture = true;
        release(&mut app, window, end);
        assert!(app.world().resource::<Interaction>().linking.is_none());
        assert_eq!(app.world().resource::<Scene>().0.edges.len(), 2);
        app.world_mut().resource_mut::<editor::Editor>().capture = false;
        press(&mut app, window, start);
        app.world_mut().get_mut::<Window>(window).unwrap().focused = false;
        app.update();
        assert!(app.world().resource::<Interaction>().linking.is_none());
        assert_eq!(app.world().resource::<Scene>().0.edges.len(), 2);
    }
    #[test]
    fn dragging_does_not_mark_stationary_visuals_changed() {
        #[derive(Resource, Default)]
        struct ChangedVisuals(Vec<Entity>);
        fn record(query: Query<Entity, Changed<Transform>>, mut changed: ResMut<ChangedVisuals>) {
            changed.0 = query.iter().collect();
        }
        let document = stress::synthetic(4, false, false, false).unwrap();
        let mut app = App::new();
        app.init_resource::<Interaction>()
            .init_resource::<physics::PhysicsEditor>()
            .init_resource::<ChangedVisuals>()
            .add_systems(Update, (update_visuals, record).chain());
        let mut visuals = Vec::new();
        for (i, node) in document.nodes.iter().enumerate() {
            visuals.push(
                app.world_mut()
                    .spawn((
                        NodeVisual(i),
                        Transform::from_translation(center(node).extend(1.0)),
                    ))
                    .id(),
            );
        }
        app.insert_resource(Scene(document));
        app.update();
        app.world_mut().resource_mut::<Scene>().0.nodes[0].rect[0] += 10.0;
        app.world_mut().resource_mut::<Interaction>().dirty = true;
        app.update();
        let changed = &app.world().resource::<ChangedVisuals>().0;
        assert_eq!(changed.len(), 1, "only the moved node should change");
        assert!(changed.contains(&visuals[0]));
        app.world_mut().resource_mut::<Interaction>().dirty = true;
        app.update();
        assert!(app.world().resource::<ChangedVisuals>().0.is_empty());
    }
    #[test]
    fn incomplete_msdf_load_exits_with_invalid_report_instead_of_sampling() {
        let directory = tempfile::tempdir().unwrap();
        let output = directory.path().join("benchmark.json");
        let mut app = App::new();
        app.insert_resource(Scene(stress::synthetic(1, false, false, false).unwrap()))
            .init_resource::<Interaction>()
            .init_resource::<physics::PhysicsEditor>()
            .init_resource::<stress::Counters>()
            .init_resource::<msdf_text::MsdfCache>()
            .insert_resource(Config {
                font: vec![],
                benchmark: true,
                output: output.clone(),
                screenshot: None,
                light: false,
                ui_scale: 1.0,
                physics: false,
                no_ui: false,
                preview: false,
                edit_node: None,
                diagnostics: false,
            })
            .insert_resource(Measurement {
                start: Instant::now() - std::time::Duration::from_secs(121),
                warmup: default(),
                previous: Instant::now(),
                phase: 0,
                samples: vec![],
                results: vec![],
                home: Vec2::ZERO,
                home_scale: 1.0,
                screenshot_taken: false,
            })
            .add_message::<AppExit>()
            .add_systems(Update, benchmark);
        app.world_mut().spawn((Window::default(), PrimaryWindow));
        app.world_mut().spawn((
            Camera2d,
            Projection::Orthographic(OrthographicProjection::default_2d()),
            Transform::default(),
        ));
        app.update();
        let report: serde_json::Value = serde_json::from_slice(&fs::read(output).unwrap()).unwrap();
        assert_eq!(report["valid"], false);
        assert_eq!(report["phases"], serde_json::json!([]));
        assert!(
            app.world_mut()
                .resource_mut::<Messages<AppExit>>()
                .drain()
                .any(|event| !matches!(event, AppExit::Success))
        );
    }

    #[test]
    fn zoom_keeps_cursor_anchor() {
        for dpi in [1.0, 1.5, 1.75, 2.0] {
            let mut window = Window::default();
            window.resolution.set_scale_factor(dpi);
            window.resolution.set(1280.0, 800.0);
            let cursor = Vec2::new(100.0, 200.0);
            let mut transform = Transform::from_xyz(20.0, 30.0, 1000.0);
            let before = world_cursor(cursor, &window, &transform, 4.0);
            let after = world_cursor(cursor, &window, &transform, 2.0);
            transform.translation += (before - after).extend(0.0);
            assert!((world_cursor(cursor, &window, &transform, 2.0) - before).length() < 0.001);
        }
    }

    #[test]
    fn camera_depth_includes_groups_edges_and_nodes() {
        use bevy::camera::CameraProjection;
        let camera = canvas_camera_transform(Vec2::new(700.0, -900.0));
        let projection = OrthographicProjection::default_2d();
        for depth in [-5.0, -4.8, 0.4, 1.0, 1.2, 2.4] {
            let position = Vec3::new(700.0, -900.0, depth);
            let clip = projection.get_clip_from_view()
                * camera.to_matrix().inverse()
                * position.extend(1.0);
            assert!(
                (0.0..=1.0).contains(&(clip.z / clip.w)),
                "depth {depth} clipped"
            );
        }
    }

    #[test]
    fn input_selects_and_drags_multiple_nodes_and_updates_visuals() {
        let node = |id: &str, x: f32| GraphNode {
            id: id.into(),
            kind: "TextNode".into(),
            rect: [x, -20.0, 40.0, 40.0],
            text: id.into(),
            url: None,
            font_size: 32.0,
            auto_size: false,
            color: [0.0; 4],
            attachment: None,
            source_name: None,
            children: vec![],
            locked: false,
            collapsed: false,
            stroke: vec![],
        };
        let graph = Graph {
            nodes: vec![node("a", -20.0), node("b", 100.0)],
            edges: vec![],
            attachments: Default::default(),
            counts: Default::default(),
        };
        let mut app = App::new();
        app.insert_resource(Scene(Document::memory(graph)))
            .init_resource::<editor::Editor>()
            .init_resource::<physics::PhysicsEditor>()
            .init_resource::<effects::Effects>()
            .init_resource::<edges::EdgePaths>()
            .init_resource::<Interaction>()
            .init_resource::<ButtonInput<MouseButton>>()
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<AccumulatedMouseScroll>()
            .insert_resource(Config {
                font: vec![],
                benchmark: false,
                output: "unused.json".into(),
                screenshot: None,
                light: false,
                ui_scale: 1.0,
                physics: false,
                no_ui: false,
                diagnostics: false,
                preview: false,
                edit_node: None,
            })
            .insert_resource(Measurement {
                start: Instant::now(),
                warmup: default(),
                previous: Instant::now(),
                phase: 0,
                samples: vec![],
                results: vec![],
                home: Vec2::ZERO,
                home_scale: 1.0,
                screenshot_taken: false,
            })
            .add_systems(Update, (input, update_visuals).chain());
        let mut window = Window {
            resolution: (1280, 800).into(),
            ..default()
        };
        window.resolution.set_scale_factor(1.75);
        window.resolution.set(1280.0, 800.0);
        window.set_cursor_position(Some(Vec2::new(640.0, 400.0)));
        let window_entity = app.world_mut().spawn((window, PrimaryWindow)).id();
        app.world_mut().spawn((
            Camera2d,
            Projection::Orthographic(OrthographicProjection::default_2d()),
            Transform::from_xyz(0.0, 0.0, 1000.0),
        ));
        let visual = app
            .world_mut()
            .spawn((NodeVisual(0), Transform::default()))
            .id();
        let keys = &mut app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keys.press(KeyCode::ControlLeft);
        keys.press(KeyCode::KeyA);
        app.update();
        assert_eq!(app.world().resource::<Interaction>().selected.len(), 2);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .clear();
        app.world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .press(MouseButton::Left);
        app.update();
        app.world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .clear();
        app.world_mut()
            .get_mut::<Window>(window_entity)
            .unwrap()
            .set_cursor_position(Some(Vec2::new(660.0, 410.0)));
        app.update();
        let scene = app.world().resource::<Scene>();
        assert_eq!(scene.0.nodes[0].rect[0], 0.0);
        assert_eq!(scene.0.nodes[1].rect[0], 120.0);
        assert_eq!(scene.0.nodes[0].rect[1], -10.0);
        assert_eq!(
            app.world()
                .get::<Transform>(visual)
                .unwrap()
                .translation
                .truncate(),
            Vec2::new(20.0, -10.0)
        );
        app.world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .release(MouseButton::Left);
        app.update();
        app.world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .press(MouseButton::Middle);
        app.world_mut()
            .get_mut::<Window>(window_entity)
            .unwrap()
            .set_cursor_position(Some(Vec2::new(670.0, 415.0)));
        app.update();
        let mut query = app
            .world_mut()
            .query_filtered::<&Transform, With<Camera2d>>();
        assert_eq!(
            query.single(app.world()).unwrap().translation.truncate(),
            Vec2::new(-10.0, 5.0)
        );
        assert_eq!(app.world().resource::<Scene>().0.nodes[0].rect[0], 0.0);
        // One gesture creates one document transaction and restores both nodes.
        app.world_mut().resource_mut::<Scene>().0.undo();
        assert_eq!(app.world().resource::<Scene>().0.nodes[0].rect[0], -20.0);
        assert_eq!(app.world().resource::<Scene>().0.nodes[1].rect[0], 100.0);
        let before = app.world().resource::<Scene>().0.nodes.clone();
        app.world_mut().resource_mut::<editor::Editor>().capture = true;
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyF);
        app.world_mut()
            .resource_mut::<AccumulatedMouseScroll>()
            .delta
            .y = 10.0;
        app.update();
        assert_eq!(app.world().resource::<Scene>().0.nodes, before);
        let mut query = app
            .world_mut()
            .query_filtered::<&Projection, With<Camera2d>>();
        let Projection::Orthographic(projection) = query.single(app.world()).unwrap() else {
            panic!()
        };
        assert_eq!(projection.scale, 1.0);
        {
            let mut interaction = app.world_mut().resource_mut::<Interaction>();
            interaction.selecting = true;
            interaction.drag_start = Some(Vec2::ZERO);
        }
        {
            let mut mouse = app.world_mut().resource_mut::<ButtonInput<MouseButton>>();
            mouse.press(MouseButton::Left);
            mouse.clear();
            mouse.release(MouseButton::Left);
        }
        app.update();
        assert!(!app.world().resource::<Interaction>().selecting);
        assert!(app.world().resource::<Interaction>().drag_start.is_none());
    }
}
