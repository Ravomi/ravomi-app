use super::*;
use bevy_egui::egui;

const CAMERA_THRESHOLD: f32 = 0.45;
const VIEWPORT_RATIO: f32 = 0.15;
const EXIT_RATIO: f32 = 1.12;
const MIN_TITLE_PIXELS: f32 = 20.0;
const TITLE_LAYOUT_SIZE: f32 = 100.0;

#[derive(Default)]
pub struct Overview {
    pub active: HashSet<String>,
    pub generation: u64,
    entries: Vec<Entry>,
    world_per_pixel: f32,
}

struct Entry {
    id: String,
    text: String,
    rect: Rect,
    ancestor_interior: Option<Rect>,
    palette: theme::OverviewPalette,
}

#[derive(Component)]
pub(super) struct OverviewVisual {
    pub(super) id: String,
    title: Entity,
    cover: Entity,
    border: Entity,
    backdrop: Entity,
}

fn eligible(side: f32, viewport: f32, zoom: f32, was_active: bool) -> bool {
    let hysteresis = if was_active { EXIT_RATIO } else { 1.0 };
    zoom <= CAMERA_THRESHOLD * hysteresis && side * zoom < viewport * VIEWPORT_RATIO * hysteresis
}

fn title_size(size: egui::Vec2, measured_width: f32) -> f32 {
    (0.9 * (size.x / (measured_width / 100.0).max(0.001)).min(size.y))
        .min(size.max_elem() * 0.8)
        .max(0.1)
}

impl Overview {
    pub fn clear(&mut self) {
        if !self.active.is_empty() {
            self.active.clear();
            self.generation = self.generation.wrapping_add(1);
        }
        self.entries.clear();
    }
    pub fn hit(&self, doc: &Document, point: Vec2) -> Option<usize> {
        doc.nodes
            .iter()
            .enumerate()
            .filter(|(_, node)| self.active.contains(&node.id) && contains(node, point))
            .max_by(|(_, a), (_, b)| (a.rect[2] * a.rect[3]).total_cmp(&(b.rect[2] * b.rect[3])))
            .map(|(index, _)| index)
    }
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &mut self,
        ctx: &egui::Context,
        doc: &Document,
        window: &Window,
        camera: &Transform,
        projection: &OrthographicProjection,
        light: bool,
        editing: Option<&str>,
    ) {
        let zoom = 1.0 / projection.scale;
        let viewport = window.width().max(window.height());
        let mut blocked = editing
            .into_iter()
            .map(str::to_owned)
            .collect::<HashSet<_>>();
        // Reveal the edited node and its ancestors, including nested groups.
        loop {
            let before = blocked.len();
            for node in &doc.nodes {
                if node.children.iter().any(|id| blocked.contains(id)) {
                    blocked.insert(node.id.clone());
                }
            }
            if blocked.len() == before {
                break;
            }
        }
        let active: HashSet<_> = doc
            .nodes
            .iter()
            .filter(|node| {
                node.kind == "Section"
                    && !node.text.trim().is_empty()
                    && doc.section_overview_allowed(&node.id)
                    && !blocked.contains(&node.id)
                    && eligible(
                        node.rect[2].max(node.rect[3]),
                        viewport,
                        zoom,
                        self.active.contains(&node.id),
                    )
            })
            .map(|node| node.id.clone())
            .collect();
        if active != self.active {
            self.active = active;
            self.generation = self.generation.wrapping_add(1);
        }
        self.entries.clear();
        self.world_per_pixel = projection.scale;
        let ui_scale = ctx.zoom_factor();
        let mut sections = doc
            .nodes
            .iter()
            .filter(|n| self.active.contains(&n.id))
            .collect::<Vec<_>>();
        // Paint descendants first; larger containing groups retain the last cover.
        sections.sort_by(|a, b| (a.rect[2] * a.rect[3]).total_cmp(&(b.rect[2] * b.rect[3])));
        let parents: HashMap<_, _> = doc
            .nodes
            .iter()
            .flat_map(|parent| {
                parent
                    .children
                    .iter()
                    .map(move |child| (child.as_str(), parent))
            })
            .collect();
        for node in sections {
            let p = theme::overview_palette(light, node.color);
            let size = egui::vec2(node.rect[2], node.rect[3]) * zoom / ui_scale;
            let offset = (center(node) - camera.translation.truncate()) * zoom;
            let center = egui::pos2(
                (window.width() * 0.5 + offset.x) / ui_scale,
                (window.height() * 0.5 - offset.y) / ui_scale,
            );
            let rect = egui::Rect::from_center_size(center, size);
            if !rect.intersects(ctx.content_rect()) {
                continue;
            }
            let mut ancestor_interior: Option<Rect> = None;
            let mut id = node.id.as_str();
            let mut visited = HashSet::new();
            while let Some(parent) = parents.get(id) {
                if !visited.insert(parent.id.as_str()) {
                    break;
                }
                let size = Vec2::new(parent.rect[2], parent.rect[3]);
                let radius = theme::corners::group_radius(size, projection.scale);
                // Inscribed rectangle of the shared continuous corner curve.
                // Keep opaque title plates inside every ancestor's rounded edge.
                let inset = radius * 0.207 + 2.0 * projection.scale;
                let safe = Rect::from_center_size(
                    super::center(parent),
                    (size - 2.0 * inset).max(Vec2::ZERO),
                );
                ancestor_interior =
                    Some(ancestor_interior.map_or(safe, |rect| rect.intersect(safe)));
                id = parent.id.as_str();
            }
            self.entries.push(Entry {
                id: node.id.clone(),
                text: node.text.clone(),
                rect: Rect::from_center_size(
                    super::center(node),
                    Vec2::new(node.rect[2], node.rect[3]),
                ),
                palette: p,
                ancestor_interior,
            });
        }
    }
}

fn canvas_color(color: egui::Color32) -> Color {
    let [r, g, b, a] = color.to_srgba_unmultiplied();
    Color::srgba_u8(r, g, b, a)
}

/// Keep one fixed-size MSDF layout per visible title. Camera/DPI/UI zoom never
/// changes its font size or allocates a second copy of the canvas glyphs.
pub fn sync(
    mut commands: Commands,
    editor: Res<editor::Editor>,
    assets: Res<NodeAssets>,
    visuals: Query<(Entity, &OverviewVisual)>,
    mut titles: Query<(&mut MsdfText, &mut TextFont, &mut TextColor)>,
) {
    let entries = &editor.overview.entries;
    let existing: HashMap<_, _> = visuals
        .iter()
        .map(|(entity, visual)| (visual.id.as_str(), (entity, visual)))
        .collect();
    for entry in entries {
        let font = TextFont {
            font: assets.font.clone().into(),
            font_size: FontSize::Px(TITLE_LAYOUT_SIZE),
            ..default()
        };
        let color = TextColor(canvas_color(entry.palette.title));
        if let Some((_, visual)) = existing.get(entry.id.as_str()) {
            if let Ok((mut text, mut old_font, mut old_color)) = titles.get_mut(visual.title) {
                if text.0 != entry.text {
                    text.0.clone_from(&entry.text);
                }
                old_font.set_if_neq(font);
                old_color.set_if_neq(color);
            }
            continue;
        }
        let root = commands
            .spawn((Transform::default(), Visibility::default()))
            .id();
        let title = commands
            .spawn((
                MsdfText::new(&entry.text),
                font,
                color,
                TextLayout::no_wrap(),
                bevy::text::LineHeight::RelativeToFont(1.0),
                Anchor::CENTER,
                Visibility::Hidden,
                ChildOf(root),
            ))
            .id();
        let mut rectangle = || {
            commands
                .spawn((
                    Sprite::from_color(Color::WHITE, Vec2::ONE),
                    Visibility::Hidden,
                    ChildOf(root),
                ))
                .id()
        };
        let cover = rectangle();
        let border = rectangle();
        let backdrop = rectangle();
        commands.entity(root).insert(OverviewVisual {
            id: entry.id.clone(),
            title,
            cover,
            border,
            backdrop,
        });
    }
    for (entity, visual) in &visuals {
        if !entries.iter().any(|entry| entry.id == visual.id) {
            commands.entity(entity).despawn();
        }
    }
}

/// Composite covers, then accepted title backdrops and MSDF text, above the
/// canvas and below egui. All positions use the same world-space camera path.
#[allow(clippy::type_complexity)]
pub fn position(
    editor: Res<editor::Editor>,
    assets: Res<NodeAssets>,
    visuals: Query<&OverviewVisual>,
    mut titles: Query<
        (&bevy::text::TextLayoutInfo, &mut Transform, &mut Visibility),
        With<MsdfText>,
    >,
    mut rectangles: Query<(&mut Sprite, &mut Transform, &mut Visibility), Without<MsdfText>>,
) {
    let overview = &editor.overview;
    let existing: HashMap<_, _> = visuals
        .iter()
        .map(|visual| (visual.id.as_str(), visual))
        .collect();
    let depth_step = 40.0 / overview.entries.len().max(1) as f32;
    let mut paint = |entity, rect: Rect, radius, style, color, depth, visible| {
        if let Ok((mut sprite, mut transform, mut visibility)) = rectangles.get_mut(entity) {
            sprite.color = color;
            theme::resize_rounded(&mut sprite, &assets.corners, rect.size(), radius, style);
            *transform = Transform::from_translation(rect.center().extend(depth));
            *visibility = if visible {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
        }
    };
    let mut occupied: Vec<Rect> = Vec::new();
    for (index, entry) in overview.entries.iter().enumerate().rev() {
        let Some(visual) = existing.get(entry.id.as_str()) else {
            continue;
        };
        let size = entry.rect.size();
        let border = (2.0 * overview.world_per_pixel).min(size.min_element() * 0.5);
        let radius = theme::corners::group_radius(size, overview.world_per_pixel);
        let inner = Rect::from_corners(entry.rect.min + border, entry.rect.max - border);
        let cover_depth = 100.0 + index as f32 * depth_step;
        let show = size.max_element() / overview.world_per_pixel >= MIN_TITLE_PIXELS;
        let cover = canvas_color(entry.palette.cover);
        paint(
            visual.cover,
            entry.rect,
            radius,
            theme::RoundedStyle::Fill,
            cover,
            cover_depth,
            show,
        );
        paint(
            visual.border,
            entry.rect,
            radius,
            theme::RoundedStyle::OverviewBorder,
            canvas_color(entry.palette.border),
            cover_depth + depth_step * 0.1,
            true,
        );
        let Ok((layout, mut transform, mut visibility)) = titles.get_mut(visual.title) else {
            continue;
        };
        *visibility = Visibility::Hidden;
        paint(
            visual.backdrop,
            inner,
            radius,
            theme::RoundedStyle::Fill,
            cover,
            200.0,
            false,
        );
        if !show || layout.size.min_element() <= 0.0 {
            continue;
        }
        let title_area = entry
            .ancestor_interior
            .map_or(inner, |safe| inner.intersect(safe));
        if title_area.is_empty() {
            continue;
        }
        let available = title_area.size();
        let scale = (title_size(egui::vec2(available.x, available.y), layout.size.x)
            / TITLE_LAYOUT_SIZE)
            .min(0.9 * available.y / layout.size.y);
        let text_rect = Rect::from_center_size(title_area.center(), layout.size * scale);
        if occupied
            .iter()
            .any(|used| !used.intersect(text_rect).is_empty())
        {
            continue;
        }
        let title_depth = 200.0 + (overview.entries.len() - index) as f32 * depth_step;
        let backdrop = Rect::from_corners(text_rect.min - border, text_rect.max + border)
            .intersect(title_area);
        let inset = (backdrop.min - entry.rect.min).min_element();
        paint(
            visual.backdrop,
            backdrop,
            (radius - inset).max(0.0),
            theme::RoundedStyle::Fill,
            canvas_color(entry.palette.backdrop),
            title_depth,
            true,
        );
        // Mesh2d retains its transparent sort key between specializations.
        // Keep visible text at a stable depth when entry ranks/counts change.
        *transform = Transform::from_translation(title_area.center().extend(300.0))
            .with_scale(Vec3::new(scale, scale, 1.0));
        *visibility = Visibility::Inherited;
        occupied.push(text_rect);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render_app() -> App {
        let mut app = msdf_text::tests::app();
        let corners = theme::NodeCorners::new(&mut app.world_mut().resource_mut::<Assets<Image>>());
        app.insert_resource(NodeAssets {
            font: Handle::default(),
            corners,
            textures: HashMap::new(),
        })
        .add_systems(PostUpdate, sync.before(msdf_text::update))
        .add_systems(PostUpdate, position.after(msdf_text::update));
        app
    }

    #[test]
    fn overview_does_not_magnify_egui_bitmap_text() {
        let mut doc = Document::blank().unwrap();
        let child = doc.create_text_node([0.0; 2]).unwrap();
        let group = doc.create_group(&HashSet::from([child])).unwrap();
        doc.nodes[group].text = "Keyboard".into();
        let ctx = egui::Context::default();
        let mut overview = Overview::default();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            overview.draw(
                ui.ctx(),
                &doc,
                &Window::default(),
                &Transform::default(),
                &OrthographicProjection {
                    scale: 3.0,
                    ..OrthographicProjection::default_2d()
                },
                false,
                None,
            );
        });
        assert!(overview.active.contains(&doc.nodes[group].id));
        assert!(
            output
                .shapes
                .iter()
                .all(|shape| !matches!(shape.shape, egui::Shape::Text(_))),
            "overview titles must use the canvas MSDF renderer, not a magnified bitmap"
        );
        output.textures_delta.clear();
    }

    #[test]
    #[ignore = "requires GRAPLU_FONT_CACHE_DOC and installed Noto Sans CJK font"]
    fn tutorial_titles_share_canvas_glyphs_at_fractional_and_high_dpi() {
        let path = std::env::var("GRAPLU_FONT_CACHE_DOC").unwrap();
        let mut doc = Document::open(std::path::Path::new(&path)).unwrap();
        for node in &mut doc.nodes {
            if node.kind == "Section" {
                node.rect = [-300.0, -150.0, 600.0, 300.0];
            }
        }
        let bytes =
            std::fs::read("/usr/share/fonts/google-noto-sans-cjk-fonts/NotoSansCJK-Regular.ttc")
                .unwrap();
        let mut app = render_app();
        let font = app
            .world_mut()
            .resource_mut::<Assets<Font>>()
            .add(Font::from_bytes(bytes));
        app.world_mut().resource_mut::<NodeAssets>().font = font.clone();
        app.world_mut().spawn((
            MsdfText::new(
                doc.nodes
                    .iter()
                    .map(|node| node.text.as_str())
                    .collect::<Vec<_>>()
                    .join("\n"),
            ),
            TextFont {
                font: font.into(),
                ..default()
            },
        ));
        msdf_text::tests::settle(&mut app);
        let baseline = app.world().resource::<msdf_text::MsdfCache>().status();
        for dpi in [1.0, 1.5, 1.75, 2.0, 3.5] {
            let ctx = egui::Context::default();
            let mut overview = Overview::default();
            for step in 0..6 {
                ctx.set_zoom_factor([1.0, 1.5, 2.0][step % 3]);
                let mut input = egui::RawInput {
                    max_texture_side: Some(2048),
                    ..default()
                };
                input
                    .viewports
                    .get_mut(&egui::ViewportId::ROOT)
                    .unwrap()
                    .native_pixels_per_point = Some(dpi);
                let mut output = ctx.run_ui(input, |ui| {
                    overview.draw(
                        ui.ctx(),
                        &doc,
                        &Window::default(),
                        &Transform::default(),
                        &OrthographicProjection {
                            scale: 5.0 + step as f32 * 0.2,
                            ..OrthographicProjection::default_2d()
                        },
                        step >= 3,
                        None,
                    );
                });
                assert!(output.shapes.is_empty());
                output.textures_delta.clear();
                assert!(!overview.entries.is_empty());
                std::mem::swap(
                    &mut app.world_mut().resource_mut::<editor::Editor>().overview,
                    &mut overview,
                );
                msdf_text::tests::settle(&mut app);
                let status = app.world().resource::<msdf_text::MsdfCache>().status();
                assert_eq!(
                    status.generated_glyphs, baseline.generated_glyphs,
                    "DPI {dpi}, frame {step}: duplicated canvas glyphs"
                );
                assert_eq!(status.atlas_pages, baseline.atlas_pages);
                std::mem::swap(
                    &mut app.world_mut().resource_mut::<editor::Editor>().overview,
                    &mut overview,
                );
            }
        }
    }

    #[test]
    fn continuous_zoom_reuses_the_same_glyph_atlas_entries() {
        let mut doc = Document::blank().unwrap();
        let child = doc.create_text_node([0.0; 2]).unwrap();
        let group = doc.create_group(&HashSet::from([child])).unwrap();
        doc.nodes[group].text = "Zoom Test".into();
        let mut overview = Overview::default();
        let ctx = egui::Context::default();
        let window = Window::default();
        let mut projection = OrthographicProjection::default_2d();
        let mut app = render_app();
        let mut baseline = None;
        for step in 0..120 {
            projection.scale = 3.0 + step as f32 * 0.031;
            let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                overview.draw(
                    ui.ctx(),
                    &doc,
                    &window,
                    &Transform::default(),
                    &projection,
                    false,
                    None,
                );
            });
            assert!(overview.active.contains(&doc.nodes[group].id));
            std::mem::swap(
                &mut app.world_mut().resource_mut::<editor::Editor>().overview,
                &mut overview,
            );
            msdf_text::tests::settle(&mut app);
            let status = app.world().resource::<msdf_text::MsdfCache>().status();
            let counts = (
                status.generated_glyphs,
                status.mesh_rebuilds,
                status.atlas_pages,
            );
            if let Some(baseline) = baseline {
                assert_eq!(counts, baseline, "zoom step {step} rebuilt MSDF text");
            } else {
                assert!(status.generated_glyphs > 0);
                let world = app.world_mut();
                let mut query = world.query::<&OverviewVisual>();
                let visual = query.single(world).unwrap();
                assert_eq!(
                    *world.get::<Visibility>(visual.title).unwrap(),
                    Visibility::Inherited
                );
                assert!(
                    world
                        .get::<Children>(visual.title)
                        .unwrap()
                        .iter()
                        .any(|entity| world.get::<Mesh2d>(entity).is_some())
                );
                baseline = Some(counts);
            }
            std::mem::swap(
                &mut app.world_mut().resource_mut::<editor::Editor>().overview,
                &mut overview,
            );
            if step > 1 {
                assert!(
                    output.textures_delta.set.is_empty(),
                    "zoom step {step} rerasterized glyphs"
                );
            }
            output.textures_delta.clear();
        }
        app.world_mut()
            .resource_mut::<editor::Editor>()
            .overview
            .clear();
        app.update();
        let world = app.world_mut();
        assert_eq!(world.query::<&OverviewVisual>().iter(world).count(), 0);
        assert_eq!(world.query::<&MsdfText>().iter(world).count(), 0);
    }

    #[test]
    fn overview_picks_group_and_reveals_an_edited_child_without_mutating_document() {
        let mut doc = Document::blank().unwrap();
        let child = doc.create_text_node([0.0; 2]).unwrap();
        let group = doc.create_group(&HashSet::from([child])).unwrap();
        doc.nodes[group].text = "Group".into();
        let revision = doc.revision;
        let mut overview = Overview::default();
        let ctx = egui::Context::default();
        let window = Window::default();
        let mut projection = OrthographicProjection::default_2d();
        projection.scale = 3.0;
        for editing in [None, Some(doc.nodes[child].id.as_str()), None] {
            let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                overview.draw(
                    ui.ctx(),
                    &doc,
                    &window,
                    &Transform::default(),
                    &projection,
                    false,
                    editing,
                );
            });
            output.textures_delta.clear();
            assert_eq!(
                overview.hit(&doc, center(&doc.nodes[child])),
                editing.is_none().then_some(group)
            );
            assert_eq!(doc.revision, revision);
        }
    }

    #[test]
    fn overview_keeps_inner_content_visible_across_zoom_levels() {
        let mut doc = Document::blank().unwrap();
        let leaf = doc.create_text_node([0.0; 2]).unwrap();
        let child = doc.create_group(&HashSet::from([leaf])).unwrap();
        let parent = doc.create_group(&HashSet::from([child])).unwrap();
        doc.nodes[child].text = "Child".into();
        doc.nodes[parent].text = "Parent".into();
        doc.nodes[child].rect = [-100.0, -50.0, 100.0, 100.0];
        doc.nodes[parent].rect = [-350.0, -200.0, 700.0, 400.0];
        let child_id = doc.nodes[child].id.clone();
        let parent_id = doc.nodes[parent].id.clone();
        let revision = doc.revision;
        let mut app = render_app();
        app.insert_resource(Scene(doc))
            .add_systems(Update, editor::sync_labels);
        let label = app
            .world_mut()
            .spawn((NodeLabel(leaf), MsdfText::new("Leaf")))
            .id();
        let ctx = egui::Context::default();
        for (scale, expected) in [
            (10.0, vec![&child_id, &parent_id]),
            (2.0, vec![&child_id]),
            (1.0, vec![]),
        ] {
            app.world_mut()
                .resource_scope(|world, mut editor: Mut<editor::Editor>| {
                    let doc = &world.resource::<Scene>().0;
                    let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                        editor.overview.draw(
                            ui.ctx(),
                            doc,
                            &Window::default(),
                            &Transform::default(),
                            &OrthographicProjection {
                                scale,
                                ..OrthographicProjection::default_2d()
                            },
                            false,
                            None,
                        );
                    });
                    output.textures_delta.clear();
                    assert_eq!(
                        editor
                            .overview
                            .entries
                            .iter()
                            .map(|entry| &entry.id)
                            .collect::<Vec<_>>(),
                        expected
                    );
                });
            app.update();
            assert_eq!(
                *app.world().get::<Visibility>(label).unwrap(),
                Visibility::Inherited
            );
            assert_eq!(app.world().resource::<Scene>().0.revision, revision);
        }
    }

    #[test]
    fn nested_title_plate_stays_inside_ancestor_corner_interior() {
        let mut app = render_app();
        let safe = Rect::from_corners(Vec2::new(-80.0, -80.0), Vec2::new(80.0, 80.0));
        app.world_mut().resource_mut::<editor::Editor>().overview = Overview {
            entries: vec![Entry {
                id: "corner-child".into(),
                text: "UI".into(),
                rect: Rect::from_center_size(Vec2::new(70.0, 80.0), Vec2::new(80.0, 40.0)),
                ancestor_interior: Some(safe),
                palette: theme::overview_palette(false, [1.0; 4]),
            }],
            world_per_pixel: 1.0,
            ..default()
        };
        msdf_text::tests::settle(&mut app);
        let world = app.world_mut();
        let visual = world.query::<&OverviewVisual>().single(world).unwrap();
        assert_eq!(
            *world.get::<Visibility>(visual.title).unwrap(),
            Visibility::Inherited
        );
        let center = world
            .get::<Transform>(visual.backdrop)
            .unwrap()
            .translation
            .truncate();
        let sprite = world.get::<Sprite>(visual.backdrop).unwrap();
        let bevy::sprite::SpriteImageMode::Sliced(slicer) = &sprite.image_mode else {
            panic!("sliced corner")
        };
        // The atlas includes one transparent logical pixel around each edge.
        let size = sprite.custom_size.unwrap() - Vec2::splat(8.0 * slicer.max_corner_scale);
        let plate = Rect::from_center_size(center, size);
        assert_eq!(plate.intersect(safe), plate);
        let transform = world.get::<Transform>(visual.title).unwrap();
        let info = world
            .get::<bevy::text::TextLayoutInfo>(visual.title)
            .unwrap();
        let title = Rect::from_center_size(
            transform.translation.truncate(),
            info.size * transform.scale.truncate(),
        );
        assert_eq!(title.intersect(safe), title);
    }

    #[test]
    fn visible_title_depth_stays_above_masks_when_overview_membership_changes() {
        let mut app = render_app();
        let entry = |id: &str, x, size| Entry {
            id: id.into(),
            text: "Title".into(),
            rect: Rect::from_center_size(Vec2::new(x, 0.0), Vec2::splat(size)),
            ancestor_interior: None,
            palette: theme::overview_palette(false, [0.0; 4]),
        };
        app.world_mut().resource_mut::<editor::Editor>().overview = Overview {
            entries: vec![entry("first", -500.0, 100.0), entry("second", 0.0, 200.0)],
            world_per_pixel: 1.0,
            ..default()
        };
        msdf_text::tests::settle(&mut app);
        let title = {
            let world = app.world_mut();
            world
                .query::<&OverviewVisual>()
                .iter(world)
                .find(|visual| visual.id == "first")
                .unwrap()
                .title
        };
        let original_depth = app.world().get::<Transform>(title).unwrap().translation.z;
        app.world_mut()
            .resource_mut::<editor::Editor>()
            .overview
            .entries
            .push(entry("third", 500.0, 300.0));
        msdf_text::tests::settle(&mut app);
        let world = app.world_mut();
        // Mesh2d's retained transparent phase caches the initial sort depth.
        // A still-visible title must not move behind its newly positioned mask.
        assert_eq!(
            world.get::<Transform>(title).unwrap().translation.z,
            original_depth
        );
        for visual in world.query::<&OverviewVisual>().iter(world) {
            assert!(
                world
                    .get::<Transform>(visual.backdrop)
                    .unwrap()
                    .translation
                    .z
                    < original_depth
            );
        }
    }

    #[test]
    fn nested_msdf_titles_keep_cover_order_and_larger_title_priority() {
        let mut app = render_app();
        app.world_mut().resource_mut::<editor::Editor>().overview = Overview {
            entries: vec![
                Entry {
                    id: "child".into(),
                    text: "Child".into(),
                    rect: Rect::from_center_size(Vec2::ZERO, Vec2::splat(100.0)),
                    ancestor_interior: None,
                    palette: theme::overview_palette(true, [1.0; 4]),
                },
                Entry {
                    id: "parent".into(),
                    text: "Parent".into(),
                    rect: Rect::from_center_size(Vec2::ZERO, Vec2::splat(200.0)),
                    ancestor_interior: None,
                    palette: theme::overview_palette(false, [0.0; 4]),
                },
            ],
            world_per_pixel: 1.0,
            ..default()
        };
        msdf_text::tests::settle(&mut app);
        let world = app.world_mut();
        let mut query = world.query::<&OverviewVisual>();
        let child = query
            .iter(world)
            .find(|visual| visual.id == "child")
            .unwrap();
        let parent = query
            .iter(world)
            .find(|visual| visual.id == "parent")
            .unwrap();
        let depth = |entity| world.get::<Transform>(entity).unwrap().translation.z;
        for visual in [child, parent] {
            let cover = world.get::<Sprite>(visual.cover).unwrap();
            let border = world.get::<Sprite>(visual.border).unwrap();
            assert_eq!(cover.custom_size, border.custom_size);
            assert_eq!(cover.image_mode, border.image_mode);
            let bevy::sprite::SpriteImageMode::Sliced(slicer) = &cover.image_mode else {
                panic!("overview masks must share the group's rounded profile");
            };
            assert_eq!(slicer.max_corner_scale, 0.25);
            assert_ne!(cover.image, Handle::<Image>::default());
        }
        assert!(depth(child.cover) < depth(parent.cover));
        assert!(depth(parent.cover) < depth(parent.backdrop));
        assert!(depth(parent.backdrop) < depth(parent.title));
        assert_eq!(
            *world.get::<Visibility>(parent.title).unwrap(),
            Visibility::Inherited
        );
        assert_eq!(
            *world.get::<Visibility>(child.title).unwrap(),
            Visibility::Hidden
        );
        assert_eq!(
            *world.get::<Visibility>(child.backdrop).unwrap(),
            Visibility::Hidden
        );
        let layout = world
            .get::<bevy::text::TextLayoutInfo>(parent.title)
            .unwrap();
        let scale = world.get::<Transform>(parent.title).unwrap().scale;
        assert!(
            (layout.size * scale.truncate())
                .cmple(Vec2::splat(180.0))
                .all()
        );
    }

    #[test]
    fn master_thresholds_with_godot_exit_hysteresis() {
        assert!(eligible(200.0, 1000.0, 0.45, false));
        assert!(!eligible(200.0, 1000.0, 0.46, false));
        assert!(!eligible(400.0, 1000.0, 0.4, false));
        assert!(eligible(400.0, 1000.0, 0.4, true));
        assert!(eligible(200.0, 1000.0, 0.5, true));
        assert!(!eligible(200.0, 1000.0, 0.51, true));
    }

    #[test]
    fn heading_fits_width_height_and_master_maximum() {
        assert_eq!(title_size(egui::vec2(200.0, 100.0), 400.0), 45.0);
        assert_eq!(title_size(egui::vec2(100.0, 100.0), 10.0), 80.0);
        assert_eq!(title_size(egui::vec2(500.0, 50.0), 100.0), 45.0);
    }
}
