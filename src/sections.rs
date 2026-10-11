use super::*;

#[derive(Resource, Default)]
pub struct Display {
    pub enabled: bool,
    pub graph: Option<Graph>,
    pub visible: Vec<bool>,
    revision: Option<u64>,
    measured: HashMap<String, Vec2>,
}

impl Display {
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            ..default()
        }
    }
    pub fn graph<'a>(&'a self, scene: &'a Scene) -> &'a Graph {
        self.graph.as_ref().unwrap_or(&scene.0.graph)
    }

    pub fn visible(&self, index: usize) -> bool {
        self.graph.is_none() || self.visible.get(index).copied().unwrap_or(false)
    }

    fn refresh(&mut self, doc: &Document, changed: bool) -> Result<bool> {
        if !self.enabled || (!changed && self.revision == Some(doc.revision)) {
            return Ok(false);
        }
        let collapsed = doc.nodes.iter().any(|node| node.collapsed);
        let was_projected = self.graph.is_some();
        if collapsed {
            let view = doc.section_view(|_, node| {
                let content = self
                    .measured
                    .get(&node.id)
                    .copied()
                    .unwrap_or(Vec2::new(80.0, node.font_size));
                let inset = 2.0 * padding(node);
                Ok([(content.x + inset).max(80.0), (content.y + inset).max(40.0)])
            })?;
            let mut nodes = doc.nodes.clone();
            for (node, rect) in nodes.iter_mut().zip(view.rects) {
                node.rect = rect;
            }
            self.visible = view
                .representatives
                .iter()
                .enumerate()
                .map(|(index, representative)| index == *representative)
                .collect();
            self.graph = Some(Graph {
                nodes,
                edges: view.edges.into_iter().flatten().collect(),
                attachments: default(),
                counts: default(),
            });
        } else {
            self.graph = None;
            self.visible.clear();
            self.measured.clear();
        }
        self.revision = Some(doc.revision);
        Ok(collapsed || was_projected)
    }
}

pub fn prepare(
    scene: Res<Scene>,
    mut display: ResMut<Display>,
    mut interaction: ResMut<Interaction>,
    mut editor: ResMut<editor::Editor>,
) {
    match display.refresh(&scene.0, false) {
        Ok(changed) => interaction.dirty |= changed,
        Err(error) => editor.report_error(error.to_string()),
    }
}

pub fn hit(
    display: Option<&Display>,
    scene: &Scene,
    point: Vec2,
    selecting: bool,
) -> Option<usize> {
    let graph = display.map_or(&scene.0.graph, |display| display.graph(scene));
    edges::hit_filtered(graph, point, selecting, |index| {
        display.is_none_or(|display| display.visible(index))
    })
}

pub fn connection_rate(
    display: Option<&Display>,
    scene: &Scene,
    index: usize,
    point: Vec2,
) -> [f32; 2] {
    let graph = display.map_or(&scene.0.graph, |display| display.graph(scene));
    edges::connection_rate(&graph.nodes[index], point)
}

pub fn stack_groups(scene: Res<Scene>, mut roots: Query<(Ref<NodeVisual>, &mut Transform)>) {
    if !scene.is_changed() && !roots.iter().any(|(node, _)| node.is_added()) {
        return;
    }
    let mut groups = scene
        .0
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, node)| node.kind == "Section")
        .collect::<Vec<_>>();
    // Equal sprite depths allow batching/culling to reorder translucent fills.
    // Refreshed parent bounds enclose descendants, so draw larger groups first.
    groups.sort_by(|(_, a), (_, b)| {
        (f64::from(b.rect[2]) * f64::from(b.rect[3]))
            .total_cmp(&(f64::from(a.rect[2]) * f64::from(a.rect[3])))
            .then_with(|| a.id.cmp(&b.id))
    });
    let step = 890.0 / groups.len().max(1) as f32;
    let depths: HashMap<_, _> = groups
        .iter()
        .enumerate()
        .map(|(rank, (index, _))| (*index, -900.0 + rank as f32 * step))
        .collect();
    for (node, mut transform) in &mut roots {
        if let Some(&depth) = depths.get(&node.0)
            && transform.translation.z != depth
        {
            transform.translation.z = depth;
        }
    }
}

pub fn sync_corners(
    scene: Res<Scene>,
    display: Res<Display>,
    assets: Res<NodeAssets>,
    camera: Query<&Projection, With<Camera2d>>,
    export: Option<Res<export_image::ExportImage>>,
    mut parts: Query<(&NodePart, &mut Sprite, Option<&NodePreviewSize>)>,
) {
    let scale = theme::group_scale(camera.single().ok(), export.is_some());
    for (part, mut sprite, preview) in &mut parts {
        let node = &display.graph(&scene).nodes[part.index];
        if !matches!(
            node.kind.as_str(),
            "Section" | "TextNode" | "UrlNode" | "LatexNode"
        ) || node.attachment.is_some()
        {
            continue;
        }
        let size = preview.map_or(Vec2::new(node.rect[2], node.rect[3]), |preview| preview.0);
        if let Some(scale) = scale.filter(|_| !node.collapsed) {
            theme::resize_rounded(
                &mut sprite,
                &assets.corners,
                size,
                theme::corners::group_radius(size, scale),
                if part.border.is_some() {
                    theme::RoundedStyle::Border
                } else {
                    theme::RoundedStyle::Fill
                },
            );
        } else {
            theme::resize(&mut sprite, &assets.corners, size, part.border.is_some());
        }
    }
}

pub fn node<'a>(
    display: Option<&'a Display>,
    doc: &'a Document,
    index: usize,
) -> Option<&'a GraphNode> {
    if display.is_some_and(|display| !display.visible(index)) {
        return None;
    }
    display
        .and_then(|display| display.graph.as_ref())
        .unwrap_or(&doc.graph)
        .nodes
        .get(index)
}

#[allow(clippy::too_many_arguments)]
pub fn sync(
    scene: Res<Scene>,
    assets: Res<NodeAssets>,
    mut display: ResMut<Display>,
    mut interaction: ResMut<Interaction>,
    mut editor: ResMut<editor::Editor>,
    measured: Query<(&NodeLabel, &bevy::text::TextLayoutInfo), Changed<bevy::text::TextLayoutInfo>>,
    mut roots: Query<(&NodeVisual, &mut Visibility)>,
    mut parts: Query<(&NodePart, &mut Sprite)>,
    mut labels: Query<(&NodeLabel, &mut Transform), Without<NodeVisual>>,
) {
    if !display.enabled {
        return;
    }
    let mut changed = false;
    for (label, info) in &measured {
        if let Some(node) = scene.0.nodes.get(label.0)
            && node.kind == "Section"
            && node.collapsed
            && info.size.is_finite()
            && info.size.y > 0.0
            && display.measured.get(&node.id) != Some(&info.size)
        {
            display.measured.insert(node.id.clone(), info.size);
            changed = true;
        }
    }
    let collapsed = scene.0.nodes.iter().any(|node| node.collapsed);
    if !collapsed && display.graph.is_none() && !interaction.dirty {
        return;
    }
    let needs_update = match display.refresh(&scene.0, changed || interaction.dirty) {
        Ok(changed) => changed,
        Err(error) => {
            editor.report_error(error.to_string());
            return;
        }
    };
    let graph = display.graph(&scene);
    for (node, mut visibility) in &mut roots {
        visibility.set_if_neq(if display.visible(node.0) {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
    }
    for (part, mut sprite) in &mut parts {
        let node = &graph.nodes[part.index];
        if node.kind != "Section" {
            continue;
        }
        theme::resize(
            &mut sprite,
            &assets.corners,
            Vec2::new(node.rect[2], node.rect[3]),
            part.border.is_some(),
        );
        if part.border.is_none() && node.color[3] == 0.0 {
            sprite.color = theme::palette(editor.light)
                .surface
                .with_alpha(if node.collapsed { 1.0 } else { 0.45 });
        }
    }
    for (label, mut transform) in &mut labels {
        let node = &graph.nodes[label.0];
        if node.kind == "Section" {
            transform.translation.y = node.rect[3] * 0.5 - padding(node);
        }
    }
    interaction.selected.retain(|index| display.visible(*index));
    interaction
        .selected_edges
        .retain(|id| graph.edges.iter().any(|edge| &edge.id == id));
    interaction.dirty |= needs_update;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn camera_zoom_updates_group_and_text_corners_without_editing_document() {
        fn check_slice_inputs(
            scene: Res<Scene>,
            camera: Single<&Projection, With<Camera2d>>,
            sprites: Query<(&NodePart, &Sprite)>,
        ) {
            let Projection::Orthographic(projection) = &**camera else {
                unreachable!()
            };
            for (part, sprite) in &sprites {
                let node = &scene.0.nodes[part.index];
                if node.kind != "Section" {
                    continue;
                }
                let bevy::sprite::SpriteImageMode::Sliced(slicer) = &sprite.image_mode else {
                    panic!("slice computation received an unprepared group");
                };
                let scale = if node.collapsed {
                    1.0
                } else {
                    projection.scale
                };
                assert_eq!(
                    slicer.max_corner_scale,
                    scale / 4.0,
                    "slice computation received the previous zoom's corner/line width"
                );
            }
        }
        let mut doc = stress::synthetic(2, false, false, false).unwrap();
        let group = doc.create_group(&HashSet::from([0, 1])).unwrap();
        doc.nodes[group].rect = [0.0, 0.0, 2400.0, 1200.0];
        doc.nodes[0].rect = [0.0, 0.0, 1800.0, 1000.0];
        let revision = doc.revision;
        let bounds = doc.nodes[group].rect;
        let mut images = Assets::default();
        let mut app = App::new();
        configure_visual_order(&mut app);
        app.insert_resource(Scene(doc))
            .insert_resource(Display::new(true))
            .insert_resource(NodeAssets {
                font: default(),
                corners: theme::NodeCorners::new(&mut images),
                textures: default(),
            })
            .add_systems(
                PostUpdate,
                (
                    check_slice_inputs.in_set(bevy::sprite_render::SpriteSystems::ComputeSlices),
                    sync_corners.in_set(CanvasVisuals),
                ),
            );
        let group_sprite = app
            .world_mut()
            .spawn((
                NodePart {
                    index: group,
                    border: None,
                },
                Sprite::default(),
            ))
            .id();
        let text_sprite = app
            .world_mut()
            .spawn((
                NodePart {
                    index: 0,
                    border: None,
                },
                Sprite::default(),
            ))
            .id();
        let camera = app
            .world_mut()
            .spawn((
                Camera2d,
                Projection::Orthographic(OrthographicProjection::default_2d()),
            ))
            .id();
        for scale in [0.25, 10.0, 1.0, 4.0, 0.25] {
            let mut projection = app.world_mut().get_mut::<Projection>(camera).unwrap();
            let Projection::Orthographic(p) = &mut *projection else {
                unreachable!()
            };
            p.scale = scale;
            app.update();
            let sprite = app.world().get::<Sprite>(group_sprite).unwrap();
            assert_eq!(
                sprite.custom_size,
                Some(Vec2::new(2400.0, 1200.0) + Vec2::splat(2.0 * scale))
            );
            let actual = app.world().get::<Sprite>(text_sprite).unwrap();
            assert_eq!(
                actual.custom_size,
                Some(Vec2::new(1800.0, 1000.0) + Vec2::splat(2.0 * scale))
            );
            assert_eq!(actual.image_mode, sprite.image_mode);
            assert_eq!(app.world().resource::<Scene>().0.revision, revision);
            assert_eq!(app.world().resource::<Scene>().0.nodes[group].rect, bounds);
        }
        app.world_mut().resource_mut::<Scene>().0.nodes[group].collapsed = true;
        app.update();
        assert_eq!(
            app.world().get::<Sprite>(group_sprite).unwrap().custom_size,
            Some(Vec2::new(2402.0, 1202.0))
        );
    }

    #[test]
    fn preparation_updates_projection_before_the_next_pointer_input() {
        let mut doc = stress::synthetic(2, false, false, false).unwrap();
        let group = doc.create_group(&HashSet::from([0, 1])).unwrap();
        doc.set_sections_collapsed(&HashSet::from([group]), true)
            .unwrap();
        let mut app = App::new();
        app.insert_resource(Scene(doc))
            .insert_resource(Display::new(true))
            .init_resource::<Interaction>()
            .init_resource::<editor::Editor>()
            .add_systems(Update, prepare);
        app.update();
        let display = app.world().resource::<Display>();
        assert!(!display.visible(0));
        let scene = app.world().resource::<Scene>();
        assert_eq!(
            hit(
                Some(display),
                scene,
                center(&display.graph(scene).nodes[group]),
                true
            ),
            Some(group)
        );
        app.world_mut().resource_mut::<Scene>().0.undo();
        app.update();
        assert!(app.world().resource::<Display>().graph.is_none());
        assert!(app.world().resource::<Display>().visible(0));
        app.world_mut().resource_mut::<Scene>().0.redo();
        app.update();
        assert!(!app.world().resource::<Display>().visible(0));
    }

    #[test]
    fn display_overrides_raw_group_layout_and_restores_children_on_expand() {
        let mut doc = Document::blank().unwrap();
        for x in [0.0, 300.0, 650.0] {
            doc.create_text_node([x, 0.0]).unwrap();
        }
        let group = doc.create_group(&HashSet::from([0, 1])).unwrap();
        let original = doc.nodes[group].rect;
        doc.set_sections_collapsed(&HashSet::from([group]), true)
            .unwrap();
        let mut images = Assets::<Image>::default();
        let assets = NodeAssets {
            font: default(),
            corners: theme::NodeCorners::new(&mut images),
            textures: default(),
        };
        let mut app = App::new();
        app.insert_resource(Scene(doc))
            .insert_resource(assets)
            .insert_resource(Display::new(true))
            .init_resource::<Interaction>()
            .init_resource::<editor::Editor>()
            .add_systems(Update, (prepare, sync).chain());
        let inside = app
            .world_mut()
            .spawn((NodeVisual(0), Visibility::Inherited))
            .id();
        let outside = app
            .world_mut()
            .spawn((NodeVisual(2), Visibility::Inherited))
            .id();
        let sprite = app
            .world_mut()
            .spawn((
                NodePart {
                    index: group,
                    border: None,
                },
                Sprite::default(),
            ))
            .id();
        let label = app
            .world_mut()
            .spawn((
                NodeLabel(group),
                Transform::default(),
                bevy::text::TextLayoutInfo {
                    size: Vec2::new(120.0, 30.0),
                    ..default()
                },
            ))
            .id();
        app.update();
        assert_eq!(
            *app.world().get::<Visibility>(inside).unwrap(),
            Visibility::Hidden
        );
        assert_eq!(
            *app.world().get::<Visibility>(outside).unwrap(),
            Visibility::Inherited
        );
        assert_eq!(
            app.world().get::<Sprite>(sprite).unwrap().custom_size,
            Some(Vec2::new(138.0, 48.0))
        );
        assert_eq!(
            app.world().get::<Transform>(label).unwrap().translation.y,
            15.0
        );
        assert_eq!(
            app.world().resource::<Scene>().0.nodes[group].rect,
            original
        );

        let scene = app.world().resource::<Scene>();
        let display = app.world().resource::<Display>();
        let card = &display.graph(scene).nodes[group];
        let position = center(card);
        assert_eq!(hit(Some(display), scene, position, true), Some(group));
        assert_eq!(hit(Some(display), scene, position, false), Some(group));
        assert_eq!(
            hit(Some(display), scene, center(&scene.0.nodes[0]), true),
            None
        );
        assert_eq!(
            hit(Some(display), scene, center(&scene.0.nodes[2]), true),
            Some(2)
        );
        assert!(marquee_hit(
            card,
            position - Vec2::ONE,
            position + Vec2::ONE
        ));
        assert!(!marquee_hit(
            &scene.0.nodes[2],
            position - Vec2::ONE,
            position + Vec2::ONE
        ));
        assert_eq!(
            connection_rate(
                Some(display),
                scene,
                group,
                position + Vec2::new(card.rect[2] * 0.5, 0.0)
            ),
            [0.99, 0.5]
        );

        // Group fitting may reapply document bounds without advancing the document revision.
        app.world_mut()
            .get_mut::<Sprite>(sprite)
            .unwrap()
            .custom_size = Some(Vec2::new(original[2], original[3]));
        app.world_mut()
            .get_mut::<Transform>(label)
            .unwrap()
            .translation
            .y = original[3] * 0.5 - 8.0;
        app.world_mut().resource_mut::<Interaction>().dirty = false;
        app.update();
        assert_eq!(
            app.world().get::<Sprite>(sprite).unwrap().custom_size,
            Some(Vec2::new(138.0, 48.0))
        );
        assert_eq!(
            app.world().get::<Transform>(label).unwrap().translation.y,
            15.0
        );
        assert!(!app.world().resource::<Interaction>().dirty);

        app.world_mut()
            .resource_mut::<Scene>()
            .0
            .set_sections_collapsed(&HashSet::from([group]), false)
            .unwrap();
        app.update();
        assert_eq!(
            *app.world().get::<Visibility>(inside).unwrap(),
            Visibility::Inherited
        );
        assert!(app.world().resource::<Display>().graph.is_none());
        assert_eq!(
            app.world().get::<Sprite>(sprite).unwrap().custom_size,
            Some(Vec2::new(original[2] + 2.0, original[3] + 2.0))
        );
    }
}
