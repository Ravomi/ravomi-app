use super::*;
use bevy_egui::egui;
#[path = "continuous_corners.rs"]
pub(super) mod corners;

pub struct Palette {
    pub canvas: Color,
    pub surface: Color,
    pub border: Color,
    pub text: Color,
    pub accent: Color,
}

pub fn window_background(light: bool) -> Color {
    let mut color = palette(light).canvas.to_linear();
    color.alpha = 0.82;
    // Window surfaces use premultiplied alpha, including the clear color.
    color.red *= color.alpha;
    color.green *= color.alpha;
    color.blue *= color.alpha;
    color.into()
}

pub fn palette(light: bool) -> Palette {
    let rgb = |n: u32| Color::srgb_u8((n >> 16) as u8, (n >> 8) as u8, n as u8);
    if light {
        Palette {
            canvas: rgb(0xeff1f5),
            surface: rgb(0xe6e9ef),
            border: rgb(0xacb0be),
            text: rgb(0x4c4f69),
            accent: rgb(0x8839ef),
        }
    } else {
        Palette {
            canvas: rgb(0x1e1e2e),
            surface: rgb(0x313244),
            border: rgb(0x585b70),
            text: rgb(0xcdd6f4),
            accent: rgb(0xcba6f7),
        }
    }
}

pub fn node_swatches(light: bool) -> [(&'static str, egui::Color32); 10] {
    let colors = if light {
        [
            0xea76cb, 0xfe640b, 0xdf8e1d, 0x40a02b, 0x179299, 0x04a5e5, 0x209fb5, 0x1e66f5,
            0x7287fd, 0x8839ef,
        ]
    } else {
        [
            0xf5c2e7, 0xfab387, 0xf9e2af, 0xa6e3a1, 0x94e2d5, 0x89dceb, 0x74c7ec, 0x89b4fa,
            0xb4befe, 0xcba6f7,
        ]
    };
    let names = [
        "粉色",
        "桃色",
        "黄色",
        "绿色",
        "青色",
        "天蓝",
        "蓝宝石",
        "蓝色",
        "薰衣草",
        "紫色",
    ];
    std::array::from_fn(|i| {
        (
            names[i],
            egui::Color32::from_rgb(
                (colors[i] >> 16) as u8,
                (colors[i] >> 8) as u8,
                colors[i] as u8,
            ),
        )
    })
}

pub struct OverviewPalette {
    pub cover: egui::Color32,
    pub backdrop: egui::Color32,
    pub title: egui::Color32,
    pub border: egui::Color32,
}

pub fn overview_palette(light: bool, rgba: [f32; 4]) -> OverviewPalette {
    let canvas = palette(light).canvas.to_srgba().to_f32_array();
    let alpha = rgba[3].clamp(0.0, 1.0);
    let background: [f32; 3] =
        std::array::from_fn(|i| rgba[i].clamp(0.0, 1.0) * alpha + canvas[i] * (1.0 - alpha));
    let linear = Color::srgb(background[0], background[1], background[2]).to_linear();
    let luminance = linear.red * 0.2126 + linear.green * 0.7152 + linear.blue * 0.0722;
    // A custom dark group still needs light text when the application uses its
    // light theme. Keep a hint of the group color, with a bounded neutral cover
    // so underlying nodes or desktop wallpaper cannot erase the title.
    let p = palette(if alpha > 0.0 {
        luminance >= 0.28
    } else {
        light
    });
    let neutral = p.canvas.to_srgba().to_f32_array();
    let tint: [u8; 3] =
        std::array::from_fn(|i| ((neutral[i] * 0.92 + background[i] * 0.08) * 255.0).round() as u8);
    OverviewPalette {
        cover: egui::Color32::from_rgba_unmultiplied(tint[0], tint[1], tint[2], 128),
        backdrop: egui::Color32::from_rgba_unmultiplied(tint[0], tint[1], tint[2], 238),
        title: egui_color(p.text),
        border: egui_color(p.border),
    }
}
pub(super) fn egui_color(c: Color) -> egui::Color32 {
    let c = c.to_srgba().to_u8_array();
    egui::Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3])
}

pub const DOCK_TOP: f32 = 16.0;
pub const DOCK_ITEM: f32 = 40.0;

pub struct DockPalette {
    pub surface: egui::Color32,
    pub border: egui::Color32,
    pub muted: egui::Color32,
    pub foreground: egui::Color32,
    pub active: egui::Color32,
    pub ink: egui::Color32,
}

pub fn dock_palette(light: bool) -> DockPalette {
    chrome_palette(light)
}

pub fn dock_frame(light: bool) -> egui::Frame {
    let p = dock_palette(light);
    egui::Frame::new()
        .fill(p.surface)
        .stroke(egui::Stroke::new(1.0, p.border))
        .corner_radius(28)
        .inner_margin(7.0)
}

pub fn dock_style(ui: &mut egui::Ui, light: bool) {
    let p = dock_palette(light);
    let visuals = ui.visuals_mut();
    visuals.override_text_color = Some(p.foreground);
    visuals.selection.bg_fill = p.foreground.gamma_multiply(0.13);
    visuals.selection.stroke = egui::Stroke::new(1.0, p.foreground);
    visuals.widgets.active.bg_stroke = egui::Stroke::NONE;
    visuals.widgets.hovered.bg_stroke = egui::Stroke::NONE;
    for widget in [
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
        &mut visuals.widgets.open,
    ] {
        widget.corner_radius = egui::CornerRadius::same(20);
        widget.expansion = 0.0;
    }
    visuals.widgets.hovered.weak_bg_fill = p.foreground.gamma_multiply(0.08);
    visuals.widgets.active.weak_bg_fill = p.foreground.gamma_multiply(0.13);
    visuals.widgets.open.weak_bg_fill = visuals.widgets.active.weak_bg_fill;
    ui.spacing_mut().interact_size.y = DOCK_ITEM;
    ui.spacing_mut().item_spacing.x = 4.0;
}

pub fn chrome_frame(light: bool) -> egui::Frame {
    dock_frame(light)
        .fill(if light {
            egui::Color32::from_rgb(230, 233, 239)
        } else {
            egui::Color32::from_rgb(24, 24, 37)
        })
        .stroke(egui::Stroke::new(1.0, egui::Color32::TRANSPARENT))
}

pub fn chrome_palette(light: bool) -> DockPalette {
    let rgb = |n: u32| egui::Color32::from_rgb((n >> 16) as u8, (n >> 8) as u8, n as u8);
    if light {
        DockPalette {
            surface: rgb(0xe6e9ef),
            border: rgb(0xccd0da),
            muted: rgb(0x626579),
            foreground: rgb(0x4c4f69),
            active: rgb(0x8839ef),
            ink: rgb(0xeff1f5),
        }
    } else {
        DockPalette {
            surface: rgb(0x181825),
            border: rgb(0x313244),
            muted: rgb(0xa6adc8),
            foreground: rgb(0xcdd6f4),
            active: rgb(0xcba6f7),
            ink: rgb(0x1e1e2e),
        }
    }
}

pub fn chrome_style(ui: &mut egui::Ui, light: bool) {
    dock_style(ui, light);
    let p = chrome_palette(light);
    let visuals = ui.visuals_mut();
    visuals.override_text_color = Some(p.foreground);
    visuals.widgets.inactive.weak_bg_fill = p.surface;
}

pub fn tool_frame(light: bool) -> egui::Frame {
    let p = chrome_palette(light);
    egui::Frame::new()
        .fill(egui_color(palette(light).canvas))
        .stroke(egui::Stroke::new(1.0, p.border))
        .corner_radius(12)
        .inner_margin(16)
        .shadow(egui::epaint::Shadow {
            offset: [0, 4],
            blur: 16,
            spread: 0,
            color: egui::Color32::from_black_alpha(if light { 18 } else { 40 }),
        })
}

pub fn tool_style(ui: &mut egui::Ui, light: bool) {
    let p = chrome_palette(light);
    chrome_style(ui, light);
    ui.spacing_mut().interact_size.y = 32.0;
    ui.spacing_mut().button_padding = egui::vec2(12.0, 7.0);
    ui.spacing_mut().item_spacing = egui::vec2(8.0, 8.0);
    let visuals = ui.visuals_mut();
    visuals.extreme_bg_color = p.surface;
    for widget in [
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
        &mut visuals.widgets.open,
    ] {
        widget.corner_radius = egui::CornerRadius::same(8);
        widget.bg_stroke = egui::Stroke::NONE;
        widget.expansion = 0.0;
    }
    visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0, p.active);
}

pub fn close_button(ui: &mut egui::Ui, light: bool, label: &str) -> egui::Response {
    let p = chrome_palette(light);
    let response = ui.add(
        egui::Button::new("")
            .frame(false)
            .corner_radius(8)
            .min_size(egui::vec2(32.0, 32.0)),
    );
    let center = response.rect.center();
    let stroke = egui::Stroke::new(1.5, p.foreground);
    ui.painter().line_segment(
        [
            center + egui::vec2(-4.5, -4.5),
            center + egui::vec2(4.5, 4.5),
        ],
        stroke,
    );
    ui.painter().line_segment(
        [
            center + egui::vec2(4.5, -4.5),
            center + egui::vec2(-4.5, 4.5),
        ],
        stroke,
    );
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), label)
    });
    response.on_hover_text(label)
}

pub fn tool_header(ui: &mut egui::Ui, title: &str, light: bool) -> bool {
    let mut close = false;
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(title).size(16.0));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            close = close_button(ui, light, "关闭").clicked();
        });
    });
    ui.add_space(4.0);
    close
}

pub fn primary_button<'a>(label: &'a str, light: bool) -> egui::Button<'a> {
    let p = chrome_palette(light);
    egui::Button::new(egui::RichText::new(label).color(p.ink))
        .fill(p.active)
        .stroke(egui::Stroke::NONE)
        .corner_radius(8)
        .min_size(egui::vec2(80.0, 32.0))
}
pub fn apply(ctx: &egui::Context, light: bool) {
    let egui_theme = if light {
        egui::Theme::Light
    } else {
        egui::Theme::Dark
    };
    ctx.set_theme(egui_theme);
    let p = palette(light);
    let mut visuals = if light {
        egui::Visuals::light()
    } else {
        egui::Visuals::dark()
    };
    visuals.window_fill = egui_color(p.surface);
    visuals.panel_fill = egui_color(p.surface);
    let chrome = chrome_palette(light);
    visuals.window_stroke = egui::Stroke::new(1.0, egui_color(p.border));
    visuals.extreme_bg_color = egui_color(p.canvas);
    visuals.faint_bg_color = chrome.surface;
    visuals.weak_text_color = Some(chrome.muted);
    for widget in [
        &mut visuals.widgets.noninteractive,
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
        &mut visuals.widgets.open,
    ] {
        widget.bg_fill = chrome.surface;
        widget.weak_bg_fill = chrome.surface;
        widget.bg_stroke = egui::Stroke::new(1.0, egui_color(p.border));
        widget.fg_stroke = egui::Stroke::new(1.0, chrome.foreground);
    }
    visuals.widgets.hovered.weak_bg_fill = chrome.border;
    visuals.widgets.active.weak_bg_fill = chrome.border;
    visuals.override_text_color = Some(egui_color(p.text));
    visuals.selection.bg_fill = egui_color(p.accent).gamma_multiply(0.35);
    visuals.selection.stroke = egui::Stroke::new(1.0, egui_color(p.accent));
    visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0, egui_color(p.accent));
    visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, egui_color(p.accent));
    visuals.window_corner_radius = egui::CornerRadius::same(12);
    ctx.set_visuals(visuals);
    ctx.style_mut_of(egui_theme, |style| {
        style.spacing.item_spacing = egui::vec2(8.0, 8.0);
        style.spacing.button_padding = egui::vec2(8.0, 4.0);
        style
            .text_styles
            .insert(egui::TextStyle::Body, egui::FontId::proportional(14.0));
        style
            .text_styles
            .insert(egui::TextStyle::Button, egui::FontId::proportional(14.0));
        style
            .text_styles
            .insert(egui::TextStyle::Heading, egui::FontId::proportional(16.0));
    });
}

const RASTER_SCALE: f32 = 4.0;

/// Shared textures for every integer radius allowed by Godot's fitted profile.
/// Resizing switches handles rather than rasterizing per node or per frame.
pub struct NodeCorners {
    fills: [Handle<Image>; corners::MAX_RADIUS + 1],
    borders: [Handle<Image>; corners::MAX_RADIUS + 1],
    overview_border: Handle<Image>,
}

impl NodeCorners {
    pub(super) fn clip_mask(&self) -> Handle<Image> {
        self.fills[corners::MAX_RADIUS].clone()
    }

    pub fn new(images: &mut Assets<Image>) -> Self {
        Self {
            fills: std::array::from_fn(|radius| rounded(images, radius, false, 1.0)),
            borders: std::array::from_fn(|radius| rounded(images, radius, true, 1.0)),
            overview_border: rounded(images, corners::MAX_RADIUS, true, 2.0),
        }
    }

    fn texture(&self, radius: usize, border: bool) -> &Handle<Image> {
        if border {
            &self.borders[radius]
        } else {
            &self.fills[radius]
        }
    }
}

fn rounded(images: &mut Assets<Image>, radius: usize, border: bool, width: f32) -> Handle<Image> {
    // One transparent unit accommodates the centered 1-unit outline. The
    // four-unit straight section matches fitted_radius(..., border_width=0).
    let extent = if radius == 0 {
        3.0
    } else {
        radius as f32 * 2.0 + 6.0
    };
    let (path, fill, stroke) = if radius == 0 {
        // Square tiny nodes must retain their bounds even when nine-slicing
        // scales down. Keep the zero-radius stroke inside those bounds.
        (
            corners::path(
                Vec2::splat(if border { 2.0 } else { 3.0 }),
                0.0,
                if border { 0.5 } else { 0.0 },
            ),
            if border { "none" } else { "white" },
            if border { "white" } else { "none" },
        )
    } else {
        (
            corners::path(Vec2::splat(extent - 2.0), radius as f32, 1.0),
            if border { "none" } else { "white" },
            if border { "white" } else { "none" },
        )
    };
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{extent}" height="{extent}"><path d="{path}" fill="{fill}" stroke="{stroke}" stroke-width="{width}"/></svg>"#
    );
    let tree = resvg::usvg::Tree::from_data(svg.as_bytes(), &resvg::usvg::Options::default())
        .expect("internally generated continuous corner SVG");
    let pixels_size = (extent * RASTER_SCALE) as u32;
    let mut pixels = resvg::tiny_skia::Pixmap::new(pixels_size, pixels_size)
        .expect("bounded continuous corner texture dimensions");
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(RASTER_SCALE, RASTER_SCALE),
        &mut pixels.as_mut(),
    );
    let mut data = pixels.take();
    for pixel in data.as_chunks_mut::<4>().0 {
        pixel[0] = 255;
        pixel[1] = 255;
        pixel[2] = 255;
    }
    let mut image = Image::new(
        Extent3d {
            width: pixels_size,
            height: pixels_size,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    image.sampler = bevy::image::ImageSampler::linear();
    images.add(image)
}

fn image_mode(radius: usize, scale: f32) -> bevy::sprite::SpriteImageMode {
    bevy::sprite::SpriteImageMode::Sliced(bevy::sprite::TextureSlicer {
        border: bevy::sprite::BorderRect::all((radius as f32 + 1.0) * RASTER_SCALE),
        center_scale_mode: bevy::sprite::SliceScaleMode::Stretch,
        sides_scale_mode: bevy::sprite::SliceScaleMode::Stretch,
        max_corner_scale: scale / RASTER_SCALE,
    })
}

fn draw_size(size: Vec2, radius: usize) -> Vec2 {
    if radius == 0 {
        size
    } else {
        size + Vec2::splat(2.0)
    }
}

pub fn sprite(corners: &NodeCorners, color: Color, size: Vec2, border: bool) -> Sprite {
    let radius = corners::radius(size);
    Sprite {
        image: corners.texture(radius, border).clone(),
        color,
        custom_size: Some(draw_size(size, radius)),
        image_mode: image_mode(radius, 1.0),
        ..default()
    }
}

pub fn resize(sprite: &mut Mut<'_, Sprite>, corners: &NodeCorners, size: Vec2, border: bool) {
    let radius = corners::radius(size);
    let texture = corners.texture(radius, border);
    if sprite.image != *texture {
        sprite.image = texture.clone();
    }
    let mode = image_mode(radius, 1.0);
    if sprite.image_mode != mode {
        sprite.image_mode = mode;
    }
    let size = draw_size(size, radius);
    if sprite.custom_size != Some(size) {
        sprite.custom_size = Some(size);
    }
}

/// Scale the cached continuous profile rather than rasterising a new texture
/// every time the camera moves. Fill, border and selection use the same radius.
pub enum RoundedStyle {
    Fill,
    Border,
    OverviewBorder,
}

pub fn resize_rounded(
    sprite: &mut Mut<'_, Sprite>,
    corners: &NodeCorners,
    size: Vec2,
    radius: f32,
    style: RoundedStyle,
) {
    let scale = radius / corners::MAX_RADIUS as f32;
    let texture = match style {
        RoundedStyle::Fill => &corners.fills[corners::MAX_RADIUS],
        RoundedStyle::Border => &corners.borders[corners::MAX_RADIUS],
        RoundedStyle::OverviewBorder => &corners.overview_border,
    };
    let mode = image_mode(corners::MAX_RADIUS, scale);
    if sprite.image != *texture {
        sprite.image = texture.clone();
    }
    if sprite.image_mode != mode {
        sprite.image_mode = mode;
    }
    let draw_size = size + Vec2::splat(2.0 * scale);
    if sprite.custom_size != Some(draw_size) {
        sprite.custom_size = Some(draw_size);
    }
}

pub fn group_scale(projection: Option<&Projection>, exporting: bool) -> Option<f32> {
    if !exporting
        && let Some(Projection::Orthographic(projection)) = projection
        && projection.scale.is_finite()
        && projection.scale > 0.0
    {
        Some(projection.scale)
    } else {
        None
    }
}

/// Same continuous profile as the fill and border, used by selection and previews.
pub fn outline(gizmos: &mut Gizmos, center: Vec2, size: Vec2, color: Color) {
    gizmos.linestrip_2d(corners::outline(center, size), color);
}

#[allow(clippy::too_many_arguments)]
pub fn sync(
    editor: Res<editor::Editor>,
    scene: Res<Scene>,
    mut background: ResMut<ClearColor>,
    mut parts: Query<(&NodePart, &mut Sprite)>,
    mut text: Query<(&NodeLabel, &mut TextColor)>,
    mut previous: Local<Option<(bool, u64)>>,
) {
    let state = (editor.light, scene.0.revision);
    if *previous == Some(state) {
        return;
    }
    *previous = Some(state);
    let p = palette(editor.light);
    background.0 = window_background(editor.light);
    for (part, mut sprite) in &mut parts {
        let node = &scene.0.nodes[part.index];
        if part.border.is_some() {
            sprite.color = p.border;
        } else if node.attachment.is_none() {
            sprite.color = if node.color[3] > 0.0 {
                color(node.color)
            } else {
                p.surface
                    .with_alpha(if node.kind == "Section" { 0.45 } else { 1.0 })
            };
        }
    }
    for (label, mut text_color) in &mut text {
        let n = &scene.0.nodes[label.0];
        text_color.0 = if n.color[3] > 0.6 {
            let brightness = n.color[0] * 0.2126 + n.color[1] * 0.7152 + n.color[2] * 0.0722;
            palette(brightness > 0.6).text
        } else {
            p.text
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overview_titles_remain_readable_over_custom_colors_and_busy_content() {
        let luminance = |rgb: [f32; 3]| {
            let c = Color::srgb(rgb[0], rgb[1], rgb[2]).to_linear();
            c.red * 0.2126 + c.green * 0.7152 + c.blue * 0.0722
        };
        for light in [false, true] {
            for r in [0.0, 0.5, 1.0] {
                for g in [0.0, 0.5, 1.0] {
                    for b in [0.0, 0.5, 1.0] {
                        for alpha in [0.0, 0.5, 1.0] {
                            let p = overview_palette(light, [r, g, b, alpha]);
                            let fg = luminance(
                                [p.title.r(), p.title.g(), p.title.b()].map(|v| v as f32 / 255.0),
                            );
                            // Canvas sprites blend in linear light; egui's
                            // premultiplied sRGB blend is no longer used here.
                            assert_eq!(p.cover.a(), 128, "group contents must remain translucent");
                            let cover = p.backdrop.to_srgba_unmultiplied();
                            let cover_luminance =
                                luminance([cover[0], cover[1], cover[2]].map(|v| v as f32 / 255.0));
                            for underneath in [0.0, 1.0] {
                                let opacity = cover[3] as f32 / 255.0;
                                let bg = cover_luminance * opacity + underneath * (1.0 - opacity);
                                let contrast = (fg.max(bg) + 0.05) / (fg.min(bg) + 0.05);
                                assert!(
                                    contrast >= 4.5,
                                    "theme {light}, group {:?}, content {underneath}: {contrast}",
                                    [r, g, b, alpha]
                                );
                            }
                        }
                    }
                }
            }
        }
        assert_eq!(
            overview_palette(true, [0.05, 0.12, 0.2, 1.0]).title,
            egui_color(palette(false).text)
        );
        assert_eq!(
            overview_palette(false, [1.0, 0.9, 0.7, 1.0]).title,
            egui_color(palette(true).text)
        );
    }

    #[test]
    fn node_color_updates_without_a_theme_toggle_and_tracks_undo() {
        let mut doc = Document::blank().unwrap();
        doc.create_text_node([0.0; 2]).unwrap();
        let mut app = App::new();
        app.insert_resource(Scene(doc))
            .insert_resource(editor::Editor::default())
            .insert_resource(ClearColor::default())
            .add_systems(Update, sync);
        let entity = app
            .world_mut()
            .spawn((
                NodePart {
                    index: 0,
                    border: None,
                },
                Sprite::default(),
            ))
            .id();
        app.update();
        let rgba = [0.9, 0.2, 0.3, 1.0];
        app.world_mut()
            .resource_mut::<Scene>()
            .0
            .set_node_properties(
                &HashSet::from([0]),
                &project_graph_bevy_canvas::document::NodeProperties {
                    color: Some(rgba),
                    ..default()
                },
            )
            .unwrap();
        app.update();
        assert_eq!(
            app.world().get::<Sprite>(entity).unwrap().color,
            color(rgba)
        );
        app.world_mut().resource_mut::<Scene>().0.undo();
        app.update();
        assert_eq!(
            app.world().get::<Sprite>(entity).unwrap().color,
            palette(false).surface
        );
        app.world_mut().resource_mut::<Scene>().0.redo();
        app.update();
        assert_eq!(
            app.world().get::<Sprite>(entity).unwrap().color,
            color(rgba)
        );
    }

    #[test]
    fn readable_text_meets_contrast_target_on_theme_surfaces() {
        let luminance = |color: egui::Color32| {
            let channels = [color.r(), color.g(), color.b()].map(|v| {
                let v = f64::from(v) / 255.0;
                if v <= 0.04045 {
                    v / 12.92
                } else {
                    ((v + 0.055) / 1.055).powf(2.4)
                }
            });
            channels[0] * 0.2126 + channels[1] * 0.7152 + channels[2] * 0.0722
        };
        for light in [false, true] {
            let p = palette(light);
            let chrome = chrome_palette(light);
            for (foreground, background) in [
                (chrome.foreground, chrome.surface),
                (chrome.muted, chrome.surface),
                (chrome.foreground, egui_color(p.surface)),
                (chrome.muted, egui_color(p.surface)),
                (chrome.ink, chrome.active),
            ] {
                let a = luminance(foreground);
                let b = luminance(background);
                let contrast = (a.max(b) + 0.05) / (a.min(b) + 0.05);
                assert!(contrast >= 4.5, "light={light}, contrast={contrast}");
            }
        }
    }

    #[test]
    fn cached_profiles_rasterize_fill_and_single_unit_outline() {
        let mut images = Assets::default();
        let corners = NodeCorners::new(&mut images);
        assert_eq!(images.len(), 51);
        for radius in 0..=24 {
            let fill = images.get(corners.texture(radius, false)).unwrap();
            let border = images.get(corners.texture(radius, true)).unwrap();
            let width = fill.texture_descriptor.size.width as usize;
            let alpha = |image: &Image, x: usize, y: usize| {
                image.data.as_ref().unwrap()[(y * width + x) * 4 + 3]
            };
            assert_eq!(alpha(fill, width / 2, width / 2), 255);
            assert_eq!(alpha(border, width / 2, width / 2), 0);
            // Straight top edge is exactly four raster pixels (one world unit).
            let coverage: u32 = (0..width / 2)
                .map(|y| u32::from(alpha(border, width / 2, y)))
                .sum();
            assert_eq!(coverage, 4 * 255);
        }
    }

    #[test]
    fn resize_reuses_profiles_and_does_not_mark_unchanged_sprites() {
        let mut images = Assets::default();
        let corners = NodeCorners::new(&mut images);
        let mut world = World::new();
        let entity = world
            .spawn(sprite(&corners, Color::WHITE, Vec2::splat(20.0), false))
            .id();
        world.clear_trackers();
        {
            let mut sprite = world.get_mut::<Sprite>(entity).unwrap();
            resize(&mut sprite, &corners, Vec2::splat(20.0), false);
            assert!(!sprite.is_changed());
            resize(&mut sprite, &corners, Vec2::new(120.0, 80.0), false);
            assert_eq!(sprite.image, corners.fills[24]);
            assert_eq!(sprite.custom_size, Some(Vec2::new(122.0, 82.0)));
            assert!(sprite.is_changed());
        }
        assert_eq!(images.len(), 51, "resizing must not allocate textures");
    }

    #[test]
    fn group_zoom_reuses_textures_and_restores_document_profile() {
        let mut images = Assets::default();
        let corners = NodeCorners::new(&mut images);
        let size = Vec2::new(2400.0, 1200.0);
        let mut world = World::new();
        let entity = world.spawn(sprite(&corners, Color::WHITE, size, true)).id();
        for scale in [0.25, 1.0, 4.0, 10.0] {
            let radius = corners::group_radius(size, scale);
            let mut sprite = world.get_mut::<Sprite>(entity).unwrap();
            resize_rounded(&mut sprite, &corners, size, radius, RoundedStyle::Border);
            let bevy::sprite::SpriteImageMode::Sliced(slicer) = &sprite.image_mode else {
                panic!("group lost its cached corner slices");
            };
            assert!((24.0 * RASTER_SCALE * slicer.max_corner_scale / scale - 24.0).abs() < 0.001);
            assert_eq!(sprite.image, corners.borders[24]);
            world.clear_trackers();
            let mut sprite = world.get_mut::<Sprite>(entity).unwrap();
            resize_rounded(&mut sprite, &corners, size, radius, RoundedStyle::Border);
            assert!(
                !sprite.is_changed(),
                "idle groups must not invalidate sprite batches"
            );
        }
        let projection = Projection::Orthographic(OrthographicProjection {
            scale: 10.0,
            ..OrthographicProjection::default_2d()
        });
        assert_eq!(group_scale(Some(&projection), false), Some(10.0));
        assert_eq!(group_scale(Some(&projection), true), None);
        // Export / collapsed cards can reuse the same texture but must reset slicing.
        let mut sprite = world.get_mut::<Sprite>(entity).unwrap();
        resize(&mut sprite, &corners, size, true);
        assert_eq!(sprite.image_mode, image_mode(24, 1.0));
        assert_eq!(sprite.custom_size, Some(size + Vec2::splat(2.0)));
        assert_eq!(images.len(), 51);
    }
}
