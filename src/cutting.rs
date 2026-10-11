//! Project Graph master ControllerCutting: a fixed-origin straight cut from
//! empty canvas, warning preview, and one document transaction on release.
use super::*;
use lyon_tessellation::{geom::LineSegment, math::point};

#[derive(Default)]
pub struct Gesture {
    pub line: Option<[Vec2; 2]>,
    nodes: HashSet<usize>,
    edges: HashSet<usize>,
    revision: u64,
}
impl Gesture {
    pub fn cancel(&mut self) {
        *self = default();
    }
}

fn segment(a: Vec2, b: Vec2) -> LineSegment<f32> {
    LineSegment {
        from: point(a.x, a.y),
        to: point(b.x, b.y),
    }
}
fn intersects(a: Vec2, b: Vec2, c: Vec2, d: Vec2) -> bool {
    segment(a, b).intersection(&segment(c, d)).is_some()
}
fn rect_edges(node: &GraphNode) -> [[Vec2; 2]; 4] {
    let [x, y, w, h] = node.rect;
    let corners = [
        Vec2::new(x, -y),
        Vec2::new(x + w, -y),
        Vec2::new(x + w, -y - h),
        Vec2::new(x, -y - h),
    ];
    std::array::from_fn(|i| [corners[i], corners[(i + 1) % 4]])
}
fn hits_node(node: &GraphNode, from: Vec2, to: Vec2) -> bool {
    if node.kind != "Section" || node.collapsed {
        contains(node, from)
            || contains(node, to)
            || rect_edges(node)
                .iter()
                .any(|[a, b]| intersects(from, to, *a, *b))
    } else {
        let mut title = node.clone();
        title.kind = "TextNode".into();
        title.rect[3] = title.rect[3].min(50.0);
        (!node.text.is_empty() && hits_node(&title, from, to))
            || rect_edges(node)
                .iter()
                .any(|[a, b]| intersects(from, to, *a, *b))
    }
}
pub(super) fn occupied(
    doc: &Document,
    display: Option<&sections::Display>,
    paths: &edges::EdgePaths,
    cursor: Vec2,
    scale: f32,
) -> bool {
    (0..doc.nodes.len())
        .filter_map(|i| sections::node(display, doc, i))
        .any(|n| {
            if n.kind != "Section" || n.collapsed || n.locked {
                contains(n, cursor)
            } else {
                contains(n, cursor)
                    && ((!n.text.is_empty() && -cursor.y - n.rect[1] <= 50.0)
                        || rect_edges(n).iter().any(|[a, b]| {
                            let p = segment(*a, *b).closest_point(point(cursor.x, cursor.y));
                            Vec2::new(p.x, p.y).distance(cursor) <= 2.0 * scale
                        }))
            }
        })
        || paths.0.values().any(|path| {
            path.segments.iter().any(|[a, b]| {
                let p = segment(*a + path.anchor, *b + path.anchor)
                    .closest_point(point(cursor.x, cursor.y));
                Vec2::new(p.x, p.y).distance(cursor) <= 4.0 * scale
            })
        })
}

fn candidates(
    doc: &Document,
    display: Option<&sections::Display>,
    paths: &edges::EdgePaths,
    from: Vec2,
    to: Vec2,
) -> (HashSet<usize>, HashSet<usize>) {
    let nodes = doc
        .nodes
        .iter()
        .enumerate()
        .filter(|(i, _)| sections::node(display, doc, *i).is_some_and(|n| hits_node(n, from, to)))
        .map(|(i, _)| i)
        .collect();
    let edges = doc
        .edges
        .iter()
        .enumerate()
        .filter(|(_, e)| {
            paths.0.get(&e.id).is_some_and(|path| {
                path.segments
                    .iter()
                    .any(|[a, b]| intersects(from, to, *a + path.anchor, *b + path.anchor))
            })
        })
        .map(|(i, _)| i)
        .collect();
    (nodes, edges)
}

#[allow(clippy::too_many_arguments)]
pub fn input(
    mouse: &ButtonInput<MouseButton>,
    keys: &ButtonInput<KeyCode>,
    cursor: Vec2,
    scale: f32,
    doc: &mut Document,
    display: Option<&sections::Display>,
    interaction: &mut Interaction,
    physics: &mut physics::PhysicsEditor,
    editor: &mut editor::Editor,
    paths: &edges::EdgePaths,
    effects: &mut effects::Effects,
) -> bool {
    if keys.pressed(KeyCode::Space) || mouse.pressed(MouseButton::Middle) {
        interaction.cutting.cancel();
        return false;
    }
    if mouse.just_pressed(MouseButton::Right)
        && !mouse.pressed(MouseButton::Left)
        && !occupied(doc, display, paths, cursor, scale)
    {
        physics.finish(doc);
        interaction.jump.stop(doc);
        interaction.linking = None;
        interaction.moving.clear();
        interaction.drag_start = None;
        interaction.selecting = false;
        interaction.cutting.line = Some([cursor, cursor]);
        interaction.cutting.revision = u64::MAX;
    }
    let Some([from, previous]) = interaction.cutting.line else {
        return false;
    };
    if cursor != previous
        || interaction.cutting.revision != doc.revision
        || mouse.just_released(MouseButton::Right)
    {
        interaction.cutting.line = Some([from, cursor]);
        interaction.cutting.revision = doc.revision;
        let (nodes, edges) = candidates(doc, display, paths, from, cursor);
        match doc.deletion_plan(&nodes, &edges) {
            Ok(plan) => {
                for &i in &nodes {
                    if !plan.nodes.contains(&i) {
                        continue;
                    }
                    let Some(n) = sections::node(display, doc, i) else {
                        continue;
                    };
                    for point in effects::segment_rect_contacts(
                        center(n),
                        Vec2::new(n.rect[2], n.rect[3]),
                        from,
                        cursor,
                    ) {
                        effects.cut_contact(point);
                    }
                }
                interaction.cutting.nodes = plan.nodes;
                interaction.cutting.edges = plan.edges;
            }
            Err(error) => {
                editor.report_error(error.to_string());
                interaction.cutting.cancel();
                return true;
            }
        }
    }
    if mouse.just_released(MouseButton::Right) {
        let nodes = interaction.cutting.nodes.clone();
        let edges = interaction.cutting.edges.clone();
        let pieces: Vec<_> = nodes
            .iter()
            .filter_map(|&i| sections::node(display, doc, i).cloned())
            .collect();
        doc.selection = project_graph_bevy_canvas::document::Selection::capture(
            doc,
            &interaction.selected,
            &interaction.selected_edges,
        );
        match doc.delete_objects(&nodes, &edges) {
            Ok(()) => {
                let palette = theme::palette(editor.light);
                for node in pieces {
                    if hits_node(&node, from, cursor) {
                        effects.split(
                            center(&node),
                            Vec2::new(node.rect[2], node.rect[3]),
                            from,
                            cursor,
                            if node.color[3] > 0.0 {
                                color(node.color)
                            } else {
                                Color::NONE
                            },
                            palette.border,
                        );
                    }
                }
                effects.cut_line(from, cursor);
                interaction.selected.clear();
                interaction.last_click = None;
                interaction.dirty = true;
                physics.finish(doc);
            }
            Err(error) => editor.report_error(error.to_string()),
        }
        interaction.cutting.cancel();
    } else if !mouse.pressed(MouseButton::Right) {
        interaction.cutting.cancel();
    }
    true
}

pub fn preview(
    gesture: &Gesture,
    doc: &Document,
    display: Option<&sections::Display>,
    paths: &edges::EdgePaths,
    effects: &mut effects::Effects,
) {
    let Some([from, to]) = gesture.line else {
        effects.cutting_preview(None);
        return;
    };
    let mut preview = effects::CutPreview {
        line: [from, to],
        boxes: vec![],
        edges: vec![],
    };
    for &i in &gesture.nodes {
        if let Some(n) = sections::node(display, doc, i) {
            preview
                .boxes
                .push((center(n), Vec2::new(n.rect[2], n.rect[3])));
        }
    }
    for &i in &gesture.edges {
        if let Some(edge) = doc.edges.get(i)
            && let Some(path) = paths.0.get(&edge.id)
        {
            preview.edges.push((
                path.segments
                    .iter()
                    .map(|[a, b]| [*a + path.anchor, *b + path.anchor])
                    .collect(),
                edge.kind == "LineEdge",
            ));
        }
    }
    effects.cutting_preview(Some(preview));
}

#[cfg(test)]
mod tests {
    use super::*;
    use lyon_tessellation::path::{Path, iterator::PathIterator};

    struct Harness {
        doc: Document,
        display: Option<sections::Display>,
        interaction: Interaction,
        physics: physics::PhysicsEditor,
        editor: editor::Editor,
        paths: edges::EdgePaths,
        effects: effects::Effects,
        mouse: ButtonInput<MouseButton>,
        keys: ButtonInput<KeyCode>,
    }
    impl Harness {
        fn new(count: usize) -> Self {
            Self {
                doc: stress::synthetic(count, false, false, false).unwrap(),
                display: None,
                interaction: default(),
                physics: default(),
                editor: default(),
                paths: default(),
                effects: default(),
                mouse: default(),
                keys: default(),
            }
        }
        fn frame(&mut self, cursor: Vec2) -> bool {
            input(
                &self.mouse,
                &self.keys,
                cursor,
                1.0,
                &mut self.doc,
                self.display.as_ref(),
                &mut self.interaction,
                &mut self.physics,
                &mut self.editor,
                &self.paths,
                &mut self.effects,
            )
        }
    }

    #[test]
    fn collapsed_cut_ignores_hidden_members_and_deletes_the_whole_group_once() {
        let mut h = Harness::new(3);
        let group = h.doc.create_group(&HashSet::from([0, 1])).unwrap();
        h.doc
            .set_sections_collapsed(&HashSet::from([group]), true)
            .unwrap();
        let view = h.doc.section_view(|_, _| Ok([80.0, 40.0])).unwrap();
        let mut nodes = h.doc.nodes.clone();
        for (node, rect) in nodes.iter_mut().zip(&view.rects) {
            node.rect = *rect;
        }
        let card = nodes[group].clone();
        let mut display = sections::Display::new(true);
        display.visible = (0..nodes.len()).map(|i| view.visible(i)).collect();
        display.graph = Some(Graph {
            nodes,
            edges: view.edges.into_iter().flatten().collect(),
            attachments: default(),
            counts: default(),
        });
        h.display = Some(display);
        let hidden = center(&h.doc.nodes[0]);
        assert!(!occupied(&h.doc, h.display.as_ref(), &h.paths, hidden, 1.0));
        assert!(
            candidates(
                &h.doc,
                h.display.as_ref(),
                &h.paths,
                hidden - Vec2::X,
                hidden + Vec2::X
            )
            .0
            .is_empty()
        );
        let position = center(&card);
        assert!(occupied(
            &h.doc,
            h.display.as_ref(),
            &h.paths,
            position,
            1.0
        ));
        let from = position - Vec2::X * 60.0;
        let to = position + Vec2::X * 60.0;
        assert_eq!(
            candidates(&h.doc, h.display.as_ref(), &h.paths, from, to).0,
            HashSet::from([group])
        );
        let before = h.doc.nodes.clone();
        let edges = h.doc.edges.clone();
        h.mouse.press(MouseButton::Right);
        assert!(h.frame(from));
        h.mouse.clear();
        assert!(h.frame(to));
        assert_eq!(h.interaction.cutting.nodes, HashSet::from([0, 1, group]));
        h.mouse.release(MouseButton::Right);
        assert!(h.frame(to));
        assert_eq!(h.doc.nodes, vec![before[2].clone()]);
        h.doc.undo();
        assert_eq!(h.doc.nodes, before);
        assert_eq!(h.doc.edges, edges);
        h.doc.redo();
        assert_eq!(h.doc.nodes.len(), 1);
    }

    #[test]
    fn only_empty_canvas_starts_a_right_button_cut() {
        let mut h = Harness::new(1);
        h.mouse.press(MouseButton::Right);
        assert!(!h.frame(Vec2::new(30.0, -30.0)));
        assert!(h.interaction.cutting.line.is_none());
        h.paths.0.insert(
            "visible-edge".into(),
            edges::EdgePath {
                anchor: Vec2::new(400.0, 200.0),
                segments: vec![[Vec2::ZERO, Vec2::new(100.0, 0.0)]],
            },
        );
        assert!(!h.frame(Vec2::new(450.0, 202.0)));
        assert!(h.interaction.cutting.line.is_none());
        h.mouse.press(MouseButton::Left);
        assert!(!h.frame(Vec2::splat(-200.0)));
        h.mouse.release(MouseButton::Left);
        assert!(h.frame(Vec2::splat(-200.0)));
        assert_eq!(h.interaction.cutting.line, Some([Vec2::splat(-200.0); 2]));
        assert!(!h.doc.dirty());
    }

    #[test]
    fn section_interior_is_hollow_and_only_nonempty_titles_occupy_title_area() {
        let mut h = Harness::new(1);
        let node = &mut h.doc.nodes[0];
        node.kind = "Section".into();
        node.rect = [0.0, 0.0, 200.0, 200.0];
        node.text.clear();
        let from = Vec2::new(40.0, -20.0);
        let to = Vec2::new(160.0, -20.0);
        assert!(!hits_node(&h.doc.nodes[0], from, to));
        assert!(!occupied(&h.doc, None, &h.paths, from, 1.0));
        assert!(hits_node(
            &h.doc.nodes[0],
            Vec2::new(-20.0, -100.0),
            Vec2::new(20.0, -100.0)
        ));
        assert!(occupied(
            &h.doc,
            None,
            &h.paths,
            Vec2::new(0.0, -100.0),
            1.0
        ));
        h.doc.nodes[0].text = "标题".into();
        assert!(hits_node(&h.doc.nodes[0], from, to));
        assert!(occupied(&h.doc, None, &h.paths, from, 1.0));
        assert!(!hits_node(
            &h.doc.nodes[0],
            Vec2::new(40.0, -100.0),
            Vec2::new(160.0, -100.0)
        ));
        assert!(!occupied(
            &h.doc,
            None,
            &h.paths,
            Vec2::new(100.0, -100.0),
            1.0
        ));
    }

    #[test]
    fn cut_uses_cached_curve_segments_with_anchor_not_endpoint_chord() {
        let mut h = Harness::new(2);
        let mut builder = Path::builder();
        builder.begin(point(0.0, 0.0));
        builder.cubic_bezier_to(point(0.0, 100.0), point(100.0, 100.0), point(100.0, 0.0));
        builder.end(false);
        let segments = builder
            .build()
            .iter()
            .flattened(0.25)
            .filter_map(|event| {
                if let lyon_tessellation::path::Event::Line { from, to } = event {
                    Some([Vec2::new(from.x, from.y), Vec2::new(to.x, to.y)])
                } else {
                    None
                }
            })
            .collect();
        let anchor = Vec2::new(400.0, 300.0);
        h.paths.0.insert(
            h.doc.edges[0].id.clone(),
            edges::EdgePath { anchor, segments },
        );
        let (_, curve_hit) = candidates(
            &h.doc,
            None,
            &h.paths,
            anchor + Vec2::new(50.0, 60.0),
            anchor + Vec2::new(50.0, 90.0),
        );
        assert_eq!(curve_hit, HashSet::from([0]));
        let (_, chord_hit) = candidates(
            &h.doc,
            None,
            &h.paths,
            anchor + Vec2::new(50.0, -10.0),
            anchor + Vec2::new(50.0, 10.0),
        );
        assert!(chord_hit.is_empty());
    }

    #[test]
    fn release_recomputes_latest_endpoint_and_multiple_deletions_undo_together() {
        let mut h = Harness::new(2);
        let before = h.doc.nodes.clone();
        let edges = h.doc.edges.clone();
        h.mouse.press(MouseButton::Right);
        assert!(h.frame(Vec2::new(-100.0, -48.0)));
        h.mouse.clear();
        assert!(h.frame(Vec2::new(-50.0, -48.0)));
        assert!(h.interaction.cutting.nodes.is_empty());
        h.mouse.release(MouseButton::Right);
        assert!(h.frame(Vec2::new(600.0, -48.0)));
        assert!(h.doc.nodes.is_empty());
        assert!(h.doc.edges.is_empty());
        assert!(h.interaction.cutting.line.is_none());
        assert!(h.effects.active());
        h.doc.undo();
        assert_eq!(h.doc.nodes, before);
        assert_eq!(h.doc.edges, edges);
        assert!(
            !h.doc.history.can_undo(),
            "one cut must produce one history entry"
        );
        h.doc.redo();
        assert!(h.doc.nodes.is_empty() && h.doc.edges.is_empty());
    }

    #[test]
    fn release_checks_refreshed_edge_paths_even_when_cursor_is_stationary() {
        let mut h = Harness::new(2);
        let before = h.doc.nodes.clone();
        let edges = h.doc.edges.clone();
        h.mouse.press(MouseButton::Right);
        h.frame(Vec2::new(0.0, 100.0));
        h.mouse.clear();
        h.frame(Vec2::new(100.0, 100.0));
        assert!(h.interaction.cutting.edges.is_empty());
        h.paths.0.insert(
            h.doc.edges[0].id.clone(),
            edges::EdgePath {
                anchor: Vec2::ZERO,
                segments: vec![[Vec2::new(50.0, 90.0), Vec2::new(50.0, 110.0)]],
            },
        );
        h.mouse.release(MouseButton::Right);
        h.frame(Vec2::new(100.0, 100.0));
        assert_eq!(h.doc.nodes, before);
        assert!(h.doc.edges.is_empty());
        h.doc.undo();
        assert_eq!(h.doc.edges, edges);
    }

    #[test]
    fn preview_and_release_filter_locked_ancestors_and_their_edges() {
        let mut h = Harness::new(2);
        let child = h.doc.nodes[0].id.clone();
        h.doc.nodes[1].kind = "Section".into();
        h.doc.nodes[1].children = vec![child];
        h.doc.nodes[1].locked = true;
        h.doc.nodes[1].rect = [-30.0, -80.0, 300.0, 206.0];
        let before = h.doc.nodes.clone();
        let edges = h.doc.edges.clone();
        h.paths.0.insert(
            h.doc.edges[0].id.clone(),
            edges::EdgePath {
                anchor: Vec2::ZERO,
                segments: vec![[Vec2::new(300.0, -100.0), Vec2::new(300.0, 100.0)]],
            },
        );
        h.mouse.press(MouseButton::Right);
        h.frame(Vec2::new(-100.0, -48.0));
        h.mouse.clear();
        h.frame(Vec2::new(400.0, -48.0));
        assert!(h.interaction.cutting.nodes.is_empty());
        assert!(h.interaction.cutting.edges.is_empty());
        h.mouse.release(MouseButton::Right);
        h.frame(Vec2::new(400.0, -48.0));
        assert_eq!(h.doc.nodes, before);
        assert_eq!(h.doc.edges, edges);
        assert!(!h.doc.history.can_undo());
    }

    #[test]
    fn cancellation_and_missing_release_do_not_delete_previewed_nodes() {
        for reason in 0..3 {
            let mut h = Harness::new(1);
            let before = h.doc.nodes.clone();
            h.mouse.press(MouseButton::Right);
            h.frame(Vec2::new(-100.0, -48.0));
            h.mouse.clear();
            h.frame(Vec2::new(300.0, -48.0));
            assert_eq!(h.interaction.cutting.nodes, HashSet::from([0]));
            match reason {
                0 => h.keys.press(KeyCode::Space),
                1 => h.mouse.press(MouseButton::Middle),
                _ => {
                    h.mouse.release(MouseButton::Right);
                    h.mouse.clear();
                }
            }
            h.frame(Vec2::new(300.0, -48.0));
            assert!(h.interaction.cutting.line.is_none());
            assert_eq!(h.doc.nodes, before);
            assert!(!h.doc.history.can_undo());
        }
    }
}
