use super::*;
use lyon_tessellation::path::{Path, iterator::PathIterator, math::point};

#[derive(Default)]
pub(super) struct Gesture {
    active: bool,
    cancelled: bool,
    preview: Option<Preview>,
}
struct Preview {
    start: Vec2,
    end: Vec2,
    size: Vec2,
    target: Option<(Vec2, Vec2)>,
    valid: bool,
}
impl Gesture {
    pub fn stop(&mut self, doc: &mut Document) {
        if self.active {
            doc.finish();
        }
        *self = default();
    }
    pub fn cancelled() -> Self {
        Self {
            cancelled: true,
            ..default()
        }
    }
    pub fn hint(&self) -> Option<&'static str> {
        self.preview.as_ref().map(|p| {
            if !p.valid {
                "此处不能放置：目标须为可编辑分组或空白，且不能包含自身"
            } else if p.target.is_some() {
                "Alt 跳跃：松开左键移入分组；Esc 取消"
            } else {
                "Alt 跳跃：松开左键移到最外层；Esc 取消"
            }
        })
    }
}

// Alt moves a selection between container levels. Reuse Document's history and
// validation, Bevy's input routing and Lyon's cubic flattening for the arc.
#[allow(clippy::too_many_arguments)]
pub(super) fn input(
    keys: &ButtonInput<KeyCode>,
    mouse: &ButtonInput<MouseButton>,
    cursor: Vec2,
    doc: &mut Document,
    display: Option<&sections::Display>,
    interaction: &mut Interaction,
    physics: &mut physics::PhysicsEditor,
    editor: &mut editor::Editor,
) -> bool {
    let alt = keys.any_pressed([KeyCode::AltLeft, KeyCode::AltRight]);
    if !alt {
        let was_active = interaction.jump.active;
        interaction.jump.stop(doc);
        return was_active;
    }
    if keys.pressed(KeyCode::Space) || interaction.linking.is_some() {
        interaction.jump.stop(doc);
        return false;
    }
    if interaction.jump.cancelled {
        interaction.jump.preview = None;
        return interaction.jump.cancelled;
    }
    let start = mouse.just_pressed(MouseButton::Left)
        || (mouse.pressed(MouseButton::Left)
            && keys.any_just_pressed([KeyCode::AltLeft, KeyCode::AltRight]));
    if start {
        if physics.dragging || (!interaction.moving.is_empty() && doc.in_transaction()) {
            physics.pause_drag();
        } else {
            physics.finish(doc);
        }
        if interaction.selected.is_empty()
            && let Some(index) = edges::hit_filtered(
                display
                    .and_then(|display| display.graph.as_ref())
                    .unwrap_or(&doc.graph),
                cursor,
                false,
                |i| sections::node(display, doc, i).is_some(),
            )
        {
            interaction.selected.insert(index);
        }
        interaction.jump.active = !interaction.selected.is_empty();
        interaction.moving.clear();
        interaction.drag_start = None;
        interaction.selecting = false;
        interaction.last_click = None;
    }
    let destination = destination(doc, display, &interaction.selected, cursor);
    let plan = doc.jump_plan(&interaction.selected, destination, [cursor.x, -cursor.y]);
    let bounds = selection_bounds(doc, display, &interaction.selected);
    let landing = plan
        .as_ref()
        .ok()
        .zip(bounds)
        .map(|(plan, (center, _))| {
            let [x, y, w, h] = plan.source_bounds;
            [
                cursor.x + x + w * 0.5 - center.x,
                -cursor.y + y + h * 0.5 + center.y,
            ]
        })
        .unwrap_or([cursor.x, -cursor.y]);
    interaction.jump.preview = bounds.map(|(center, size)| Preview {
        start: center,
        end: cursor,
        size,
        target: destination
            .and_then(|i| sections::node(display, doc, i))
            .map(|node| (super::center(node), Vec2::new(node.rect[2], node.rect[3]))),
        valid: plan.is_ok(),
    });
    if mouse.just_released(MouseButton::Left) && interaction.jump.active {
        interaction.jump.active = false;
        match doc.jump_selection(&interaction.selected, destination, landing) {
            Ok(()) => interaction.dirty = true,
            Err(error) => {
                doc.finish();
                editor.report_error(error.to_string());
            }
        }
        interaction.jump.preview = None;
        interaction.last_cursor = Some(cursor);
        return true;
    }
    interaction.last_cursor = Some(cursor);
    interaction.jump.active || start
}

fn selection_bounds(
    doc: &Document,
    display: Option<&sections::Display>,
    selection: &HashSet<usize>,
) -> Option<(Vec2, Vec2)> {
    let mut lo = Vec2::splat(f32::INFINITY);
    let mut hi = Vec2::splat(f32::NEG_INFINITY);
    for &i in selection {
        let Some(node) = sections::node(display, doc, i) else {
            continue;
        };
        let half = Vec2::new(node.rect[2], node.rect[3]) * 0.5;
        lo = lo.min(center(node) - half);
        hi = hi.max(center(node) + half);
    }
    (lo.is_finite() && hi.is_finite()).then_some(((lo + hi) * 0.5, hi - lo))
}

fn destination(
    doc: &Document,
    display: Option<&sections::Display>,
    selection: &HashSet<usize>,
    cursor: Vec2,
) -> Option<usize> {
    let indices: HashMap<_, _> = doc
        .nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.id.as_str(), i))
        .collect();
    let mut roots = selection.clone();
    let mut todo: Vec<_> = selection
        .iter()
        .filter_map(|&i| doc.nodes.get(i))
        .flat_map(|n| n.children.iter())
        .filter_map(|id| indices.get(id.as_str()).copied())
        .collect();
    let mut visited = HashSet::new();
    while let Some(i) = todo.pop() {
        if visited.insert(i) {
            roots.remove(&i);
            todo.extend(
                doc.nodes[i]
                    .children
                    .iter()
                    .filter_map(|id| indices.get(id.as_str()).copied()),
            );
        }
    }
    // Smaller overlapping containers are the innermost drop targets. Do not
    // ignore selected descendants: validation must reject a cycle, not ungroup.
    let hit = doc
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(i, _)| sections::node(display, doc, i).map(|node| (i, node)))
        .filter(|(i, node)| !roots.contains(i) && contains(node, cursor))
        .min_by(|(_, a), (_, b)| (a.rect[2] * a.rect[3]).total_cmp(&(b.rect[2] * b.rect[3])))
        .map(|(i, _)| i);
    hit.or_else(|| {
        selection
            .iter()
            .copied()
            .filter(|&i| sections::node(display, doc, i).is_some_and(|node| contains(node, cursor)))
            .min()
    })
}

pub(super) fn preview(gesture: &Gesture, gizmos: &mut Gizmos) {
    let Some(p) = &gesture.preview else { return };
    let color = if p.valid {
        Color::srgb_u8(166, 227, 161)
    } else {
        Color::srgb_u8(243, 139, 168)
    };
    theme::outline(gizmos, p.start, p.size, color.with_alpha(0.45));
    theme::outline(gizmos, p.end, p.size, color);
    if let Some((center, size)) = p.target {
        theme::outline(gizmos, center, size + Vec2::splat(8.0), color);
    }
    let distance = p.start.distance(p.end);
    if distance <= 1.0 {
        return;
    }
    let lift = Vec2::Y * distance * 0.5;
    let xy = |v: Vec2| point(v.x, v.y);
    let mut path = Path::builder();
    path.begin(xy(p.start));
    path.cubic_bezier_to(xy(p.start + lift), xy(p.end + lift), xy(p.end));
    path.end(false);
    for event in path.build().iter().flattened(0.5) {
        if let lyon_tessellation::path::Event::Line { from, to } = event {
            gizmos.line_2d(Vec2::new(from.x, from.y), Vec2::new(to.x, to.y), color);
        }
    }
    let length = 10.0 + distance * 0.01;
    gizmos.linestrip_2d(
        [
            p.end + Vec2::new(-length, length * 2.0),
            p.end,
            p.end + Vec2::new(length, length * 2.0),
        ],
        color,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collapsed_jump_uses_visible_targets_and_lands_mixed_selection_at_cursor() {
        let mut doc = stress::synthetic(3, false, false, false).unwrap();
        let group = doc.create_group(&HashSet::from([0, 1])).unwrap();
        doc.set_sections_collapsed(&HashSet::from([group]), true)
            .unwrap();
        let view = doc.section_view(|_, _| Ok([80.0, 40.0])).unwrap();
        let mut nodes = doc.nodes.clone();
        for (node, rect) in nodes.iter_mut().zip(&view.rects) {
            node.rect = *rect;
        }
        let card = nodes[group].clone();
        let mut display = sections::Display::new(true);
        display.visible = (0..nodes.len()).map(|i| view.visible(i)).collect();
        display.graph = Some(Graph {
            nodes,
            edges: vec![],
            attachments: default(),
            counts: default(),
        });
        assert_eq!(
            destination(
                &doc,
                Some(&display),
                &HashSet::from([2]),
                center(&doc.nodes[0])
            ),
            None
        );
        assert_eq!(
            destination(&doc, Some(&display), &HashSet::from([2]), center(&card)),
            Some(group)
        );
        let selected = HashSet::from([group, 2]);
        let (origin, size) = selection_bounds(&doc, Some(&display), &selected).unwrap();
        let cursor = Vec2::new(-3000.0, 2500.0);
        let delta = cursor - origin;
        let before = doc.nodes.clone();
        let edges = doc.edges.clone();
        let mut interaction = Interaction {
            selected,
            ..default()
        };
        let mut physics = physics::PhysicsEditor::default();
        let mut editor = editor::Editor::default();
        let mut keys = ButtonInput::default();
        let mut mouse = ButtonInput::default();
        keys.press(KeyCode::AltLeft);
        mouse.press(MouseButton::Left);
        assert!(input(
            &keys,
            &mouse,
            cursor,
            &mut doc,
            Some(&display),
            &mut interaction,
            &mut physics,
            &mut editor
        ));
        let preview = interaction.jump.preview.as_ref().unwrap();
        assert!(preview.valid);
        assert_eq!(preview.size, size);
        assert_eq!(preview.start, origin);
        assert_eq!(preview.end, cursor);
        keys.clear();
        mouse.clear();
        mouse.release(MouseButton::Left);
        assert!(input(
            &keys,
            &mouse,
            cursor,
            &mut doc,
            Some(&display),
            &mut interaction,
            &mut physics,
            &mut editor
        ));
        for (node, original) in doc.nodes.iter().zip(&before) {
            assert!((center(node) - center(original) - delta).length() < 0.01);
            assert_eq!(node.children, original.children);
        }
        assert_eq!(doc.edges, edges);
        doc.undo();
        assert_eq!(doc.nodes, before);
        doc.redo();
        assert!((center(&doc.nodes[0]) - center(&before[0]) - delta).length() < 0.01);
    }

    #[test]
    fn redundant_child_selection_does_not_hide_an_invalid_descendant_target() {
        let mut doc = stress::synthetic(2, false, false, false).unwrap();
        let group = doc.create_group(&HashSet::from([0, 1])).unwrap();
        let cursor = center(&doc.nodes[0]);
        assert_eq!(
            destination(&doc, None, &HashSet::from([group]), cursor),
            Some(0)
        );
        assert_eq!(
            destination(&doc, None, &HashSet::from([group, 0]), cursor),
            Some(0)
        );
    }

    #[test]
    fn ordinary_drag_then_alt_jump_remains_one_undo_and_space_clears_the_gesture() {
        let mut doc = stress::synthetic(1, false, false, false).unwrap();
        let before = doc.nodes.clone();
        doc.begin("移动节点");
        doc.nodes[0].rect[0] += 100.0;
        let mut interaction = Interaction {
            selected: HashSet::from([0]),
            moving: HashSet::from([0]),
            ..default()
        };
        let mut physics = physics::PhysicsEditor::default();
        let mut editor = editor::Editor::default();
        let mut keys = ButtonInput::default();
        let mut mouse = ButtonInput::default();
        mouse.press(MouseButton::Left);
        mouse.clear();
        keys.press(KeyCode::AltLeft);
        let cursor = Vec2::splat(-2000.0);
        input(
            &keys,
            &mouse,
            cursor,
            &mut doc,
            None,
            &mut interaction,
            &mut physics,
            &mut editor,
        );
        assert!(doc.in_transaction());
        assert!(!doc.history.can_undo());
        keys.clear();
        mouse.release(MouseButton::Left);
        input(
            &keys,
            &mouse,
            cursor,
            &mut doc,
            None,
            &mut interaction,
            &mut physics,
            &mut editor,
        );
        doc.undo();
        assert_eq!(doc.nodes, before);
        assert!(!doc.history.can_undo());
        mouse.clear();
        mouse.press(MouseButton::Left);
        input(
            &keys,
            &mouse,
            cursor,
            &mut doc,
            None,
            &mut interaction,
            &mut physics,
            &mut editor,
        );
        assert!(interaction.jump.active);
        keys.press(KeyCode::Space);
        assert!(!input(
            &keys,
            &mouse,
            cursor,
            &mut doc,
            None,
            &mut interaction,
            &mut physics,
            &mut editor
        ));
        assert!(!interaction.jump.active);
        assert!(interaction.jump.preview.is_none());
    }

    #[test]
    fn alt_click_previews_then_moves_selection_once_and_undoes() {
        let mut doc = stress::synthetic(2, false, false, false).unwrap();
        let before = doc.nodes.clone();
        let mut interaction = Interaction {
            selected: HashSet::from([0, 1]),
            ..default()
        };
        let mut physics = physics::PhysicsEditor::default();
        let mut editor = editor::Editor::default();
        let mut keys = ButtonInput::default();
        let mut mouse = ButtonInput::default();
        keys.press(KeyCode::AltLeft);
        let cursor = Vec2::new(-3000.0, 2500.0);
        assert!(!input(
            &keys,
            &mouse,
            cursor,
            &mut doc,
            None,
            &mut interaction,
            &mut physics,
            &mut editor
        ));
        assert!(interaction.jump.preview.as_ref().unwrap().valid);
        assert_eq!(doc.nodes, before);
        mouse.press(MouseButton::Left);
        assert!(input(
            &keys,
            &mouse,
            cursor,
            &mut doc,
            None,
            &mut interaction,
            &mut physics,
            &mut editor
        ));
        assert_eq!(doc.nodes, before);
        mouse.clear();
        mouse.release(MouseButton::Left);
        assert!(input(
            &keys,
            &mouse,
            cursor,
            &mut doc,
            None,
            &mut interaction,
            &mut physics,
            &mut editor
        ));
        assert!(!interaction.jump.active);
        assert!(interaction.dirty);
        assert!(
            selection_bounds(&doc, None, &interaction.selected)
                .unwrap()
                .0
                .distance(cursor)
                < 0.001
        );
        assert_eq!(
            doc.nodes[1].rect[0] - doc.nodes[0].rect[0],
            before[1].rect[0] - before[0].rect[0]
        );
        doc.undo();
        assert_eq!(doc.nodes, before);
        assert!(!doc.history.can_undo());
    }

    #[test]
    fn releasing_alt_cancels_without_moving_or_starting_a_normal_drag() {
        let mut doc = stress::synthetic(1, false, false, false).unwrap();
        let before = doc.nodes.clone();
        let mut interaction = Interaction {
            selected: HashSet::from([0]),
            ..default()
        };
        let mut physics = physics::PhysicsEditor::default();
        let mut editor = editor::Editor::default();
        let mut keys = ButtonInput::default();
        let mut mouse = ButtonInput::default();
        keys.press(KeyCode::AltRight);
        mouse.press(MouseButton::Left);
        let cursor = Vec2::splat(-2000.0);
        input(
            &keys,
            &mouse,
            cursor,
            &mut doc,
            None,
            &mut interaction,
            &mut physics,
            &mut editor,
        );
        keys.release(KeyCode::AltRight);
        mouse.clear();
        assert!(input(
            &keys,
            &mouse,
            cursor,
            &mut doc,
            None,
            &mut interaction,
            &mut physics,
            &mut editor
        ));
        assert!(interaction.jump.preview.is_none());
        assert!(interaction.moving.is_empty());
        assert_eq!(doc.nodes, before);
        assert!(!doc.dirty());
    }
}
