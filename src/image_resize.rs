use super::*;
use bevy_egui::{EguiContexts, egui};

#[derive(Resource, Default)]
pub struct Resize {
    drag: Option<Drag>,
}

struct Drag {
    id: String,
    rect: [f32; 4],
    corner: usize,
    pointer_start: egui::Pos2,
}

fn scaled_rect(rect: [f32; 4], factor: f32, anchor: [f32; 2]) -> [f32; 4] {
    let maximum = 100_000.0 / rect[2].max(rect[3]);
    let minimum = (16.0 / rect[2].min(rect[3])).min(maximum);
    let factor = factor.clamp(minimum, maximum);
    let size = [rect[2] * factor, rect[3] * factor];
    [
        rect[0] + (rect[2] - size[0]) * anchor[0],
        rect[1] + (rect[3] - size[1]) * anchor[1],
        size[0],
        size[1],
    ]
}

#[allow(clippy::too_many_arguments)]
pub fn ui(
    mut contexts: EguiContexts,
    mut resize: ResMut<Resize>,
    mut editor: ResMut<editor::Editor>,
    mut scene: ResMut<Scene>,
    mut interaction: ResMut<Interaction>,
    mut physics: ResMut<physics::PhysicsEditor>,
    window: Single<&Window, With<PrimaryWindow>>,
    camera: Single<(&Transform, &Projection), With<Camera2d>>,
) -> bevy::prelude::Result {
    let ctx = contexts.ctx_mut()?;
    if resize.drag.is_some() && (!window.focused || ctx.input(|i| i.key_pressed(egui::Key::Escape)))
    {
        scene.0.cancel();
        resize.drag = None;
        interaction.dirty = true;
        editor.capture = true;
        editor.navigation_capture = true;
        return Ok(());
    }
    if !editor.canvas_resize_allowed() {
        if resize.drag.take().is_some() {
            scene.0.finish();
        }
        return Ok(());
    }
    let Some(index) = interaction
        .selected
        .iter()
        .copied()
        .next()
        .filter(|_| interaction.selected.len() == 1)
    else {
        if resize.drag.take().is_some() {
            scene.0.finish();
        }
        return Ok(());
    };
    let Some(node) = scene.0.nodes.get(index) else {
        resize.drag = None;
        return Ok(());
    };
    if resize.drag.as_ref().is_some_and(|drag| drag.id != node.id) {
        resize.drag = None;
    }
    if !matches!(node.kind.as_str(), "ImageNode" | "SvgNode")
        || scene.0.protected_nodes().contains(&index)
    {
        if resize.drag.take().is_some() {
            scene.0.finish();
        }
        return Ok(());
    }
    let (transform, Projection::Orthographic(projection)) = *camera else {
        return Ok(());
    };
    let id = node.id.clone();
    let rect = node.rect;
    let ui_scale = ctx.zoom_factor();
    let screen = |x: f32, y: f32| {
        egui::pos2(
            ((x - transform.translation.x) / projection.scale + window.width() * 0.5) / ui_scale,
            ((y + transform.translation.y) / projection.scale + window.height() * 0.5) / ui_scale,
        )
    };
    let screen_rect = egui::Rect::from_min_max(
        screen(rect[0], rect[1]),
        screen(rect[0] + rect[2], rect[1] + rect[3]),
    );
    let wheel = ctx.input(|i| {
        if i.modifiers.command
            && i.pointer
                .hover_pos()
                .is_some_and(|p| screen_rect.contains(p))
        {
            i.events
                .iter()
                .filter_map(|event| {
                    if let egui::Event::MouseWheel { unit, delta, .. } = event {
                        Some(
                            delta.y
                                * match unit {
                                    egui::MouseWheelUnit::Point => 1.0,
                                    egui::MouseWheelUnit::Line => 60.0,
                                    egui::MouseWheelUnit::Page => 240.0,
                                },
                        )
                    } else {
                        None
                    }
                })
                .sum::<f32>()
        } else {
            0.0
        }
    });
    if wheel.is_finite() && wheel != 0.0 && resize.drag.is_none() && !editor.capture {
        physics.finish(&mut scene.0);
        scene.0.begin("缩放图片");
        scene.0.nodes[index].rect = scaled_rect(rect, (wheel * 0.002).exp(), [0.5, 0.5]);
        scene.0.finish();
        interaction.dirty = true;
        editor.capture = true;
        editor.navigation_capture = true;
    }
    let corners = [
        screen_rect.left_top(),
        screen_rect.right_top(),
        screen_rect.right_bottom(),
        screen_rect.left_bottom(),
    ];
    let accent = theme::egui_color(theme::palette(editor.light).accent);
    for (corner, position) in corners.into_iter().enumerate() {
        let response = egui::Area::new(egui::Id::new(("image-resize", &id, corner)))
            .fixed_pos(position - egui::vec2(6.0, 6.0))
            .order(egui::Order::Middle)
            .show(ctx, |ui| {
                let (rect, response) =
                    ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::drag());
                ui.painter().rect_filled(rect.shrink(2.0), 2.0, accent);
                response.on_hover_cursor(if corner % 2 == 0 {
                    egui::CursorIcon::ResizeNwSe
                } else {
                    egui::CursorIcon::ResizeNeSw
                })
            })
            .inner;
        if response.drag_started() {
            physics.finish(&mut scene.0);
            scene.0.selection = project_graph_bevy_canvas::document::Selection::capture(
                &scene.0,
                &interaction.selected,
                &interaction.selected_edges,
            );
            scene.0.begin("缩放图片");
            resize.drag = Some(Drag {
                id: id.clone(),
                rect,
                corner,
                pointer_start: ctx.input(|i| i.pointer.press_origin()).unwrap_or(position),
            });
            interaction.moving.clear();
        }
        if let Some(drag) = &resize.drag
            && drag.id == id
            && drag.corner == corner
        {
            let delta = (ctx
                .input(|i| i.pointer.interact_pos())
                .unwrap_or(drag.pointer_start)
                - drag.pointer_start)
                * ui_scale
                * projection.scale;
            let signs = match corner {
                0 => [-1.0, -1.0],
                1 => [1.0, -1.0],
                2 => [1.0, 1.0],
                _ => [-1.0, 1.0],
            };
            // Project the pointer onto the aspect-ratio diagonal, keeping the opposite corner fixed.
            let width = drag.rect[2];
            let height = drag.rect[3];
            let factor = 1.0
                + (signs[0] * delta.x * width + signs[1] * delta.y * height)
                    / (width * width + height * height);
            let anchor = [(1.0 - signs[0]) * 0.5, (1.0 - signs[1]) * 0.5];
            scene.0.nodes[index].rect = scaled_rect(drag.rect, factor, anchor);
            interaction.dirty = true;
            editor.capture = true;
            editor.navigation_capture = true;
            if response.drag_stopped() || !ctx.input(|i| i.pointer.primary_down()) {
                scene.0.finish();
                resize.drag = None;
            }
        }
    }
    Ok(())
}

pub fn sync(scene: Res<Scene>, mut parts: Query<(&NodePart, &mut Sprite)>) {
    for (part, mut sprite) in &mut parts {
        if let Some(node) = scene.0.nodes.get(part.index)
            && matches!(node.kind.as_str(), "ImageNode" | "SvgNode")
        {
            let size = Some(Vec2::new(node.rect[2], node.rect[3]));
            if sprite.custom_size != size {
                sprite.custom_size = size;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;

    #[test]
    fn corner_drag_accumulates_pointer_motion_and_keeps_size_when_stopped_and_released() {
        let mut doc = Document::blank().unwrap();
        let index = doc
            .import_image([0.0, 0.0], [200.0, 100.0], "a.svg", b"<svg/>".to_vec())
            .unwrap();
        let original = doc.nodes[index].rect;
        let mut world = World::new();
        world.insert_resource(Scene(doc));
        world.insert_resource(editor::Editor::with_startup(false, false));
        world.insert_resource(Interaction {
            selected: HashSet::from([index]),
            ..default()
        });
        world.init_resource::<Resize>();
        world.init_resource::<physics::PhysicsEditor>();
        world.init_resource::<bevy_egui::EguiUserTextures>();
        world.spawn((
            Window {
                resolution: (800, 600).into(),
                focused: true,
                ..default()
            },
            PrimaryWindow,
        ));
        world.spawn((
            Camera2d,
            Projection::Orthographic(OrthographicProjection::default_2d()),
            Transform::default(),
        ));
        let context = world.spawn(bevy_egui::PrimaryEguiContext).id();
        let ctx = world
            .get_mut::<bevy_egui::EguiContext>(context)
            .unwrap()
            .get_mut()
            .clone();
        let mut tick = 0;
        let mut frame = |world: &mut World, events| {
            tick += 1;
            world.resource_mut::<editor::Editor>().capture = false;
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(800.0, 600.0),
                    )),
                    time: Some(tick as f64 / 60.0),
                    events,
                    ..default()
                },
                |_| {
                    let result: bevy::prelude::Result = world.run_system_once(ui).unwrap();
                    result.unwrap();
                },
            );
            output.textures_delta.clear();
        };
        frame(&mut world, vec![]);
        frame(&mut world, vec![]);
        let start = egui::pos2(500.0, 350.0);
        frame(
            &mut world,
            vec![
                egui::Event::PointerMoved(start),
                egui::Event::PointerButton {
                    pos: start,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: default(),
                },
            ],
        );
        for (point, width) in [
            (egui::pos2(520.0, 360.0), 220.0),
            (egui::pos2(540.0, 370.0), 240.0),
        ] {
            frame(&mut world, vec![egui::Event::PointerMoved(point)]);
            assert!((world.resource::<Scene>().0.nodes[index].rect[2] - width).abs() < 0.01);
        }
        let resized = world.resource::<Scene>().0.nodes[index].rect;
        for _ in 0..3 {
            frame(&mut world, vec![]);
            assert_eq!(world.resource::<Scene>().0.nodes[index].rect, resized);
        }
        frame(
            &mut world,
            vec![egui::Event::PointerButton {
                pos: egui::pos2(540.0, 370.0),
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: default(),
            }],
        );
        assert!(world.resource::<Resize>().drag.is_none());
        assert_eq!(world.resource::<Scene>().0.nodes[index].rect, resized);
        let mut scene = world.resource_mut::<Scene>();
        scene.0.undo();
        assert_eq!(scene.0.nodes[index].rect, original);
        scene.0.redo();
        assert_eq!(scene.0.nodes[index].rect, resized);
    }

    #[test]
    fn resizing_preserves_aspect_anchor_and_undo() {
        let original = [100.0, 80.0, 200.0, 100.0];
        let thin = scaled_rect([0.0, 0.0, 8192.0, 1.0], 2.0, [0.5; 2]);
        assert!(thin.iter().all(|value| value.is_finite()));
        assert_eq!(
            scaled_rect(original, 2.0, [0.0, 0.0]),
            [100.0, 80.0, 400.0, 200.0]
        );
        assert_eq!(
            scaled_rect(original, 2.0, [1.0, 1.0]),
            [-100.0, -20.0, 400.0, 200.0]
        );
        assert_eq!(
            scaled_rect(original, 2.0, [0.5, 0.5]),
            [0.0, 30.0, 400.0, 200.0]
        );
        let mut doc = Document::blank().unwrap();
        let index = doc
            .import_image([200.0, 130.0], [200.0, 100.0], "a.svg", b"<svg/>".to_vec())
            .unwrap();
        doc.begin("缩放图片");
        doc.nodes[index].rect = scaled_rect(original, 2.0, [0.5, 0.5]);
        doc.finish();
        doc.undo();
        assert_eq!(doc.nodes[index].rect, original);
        doc.redo();
        assert_eq!(doc.nodes[index].rect, [0.0, 30.0, 400.0, 200.0]);
        doc.begin("缩放图片");
        doc.nodes[index].rect = scaled_rect(original, 0.0, [0.5, 0.5]);
        assert_eq!(doc.nodes[index].rect[3], 16.0);
        doc.cancel();
        assert_eq!(doc.nodes[index].rect[2..], [400.0, 200.0]);
    }
}
