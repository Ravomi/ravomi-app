use super::*;
use bevy::{
    asset::AssetEvent,
    camera::{CameraUpdateSystems, RenderTarget},
    input::{mouse::MouseWheel, touch::TouchPhase},
    render::{camera::camera_system, texture::ManualTextureViews},
    transform::{TransformPlugin, TransformSystems},
    window::{WindowCreated, WindowResized, WindowScaleFactorChanged},
    winit::RawWinitWindowEvent,
};

fn app(dpi: f32, input_before_camera: bool) -> (App, Entity, Vec2) {
    let mut app = App::new();
    app.add_plugins(TransformPlugin)
        .init_resource::<Time<Real>>()
        .init_resource::<navigation::WheelZoom>()
        .insert_resource(Scene(stress::synthetic(2, false, false, false).unwrap()))
        .init_resource::<editor::Editor>()
        .init_resource::<physics::PhysicsEditor>()
        .init_resource::<effects::Effects>()
        .init_resource::<edges::EdgePaths>()
        .init_resource::<Interaction>()
        .init_resource::<ButtonInput<MouseButton>>()
        .init_resource::<ButtonInput<KeyCode>>()
        .add_message::<MouseWheel>()
        .add_message::<RawWinitWindowEvent>()
        .init_resource::<Assets<Image>>()
        .init_resource::<ManualTextureViews>()
        .add_message::<WindowResized>()
        .add_message::<WindowCreated>()
        .add_message::<WindowScaleFactorChanged>()
        .add_message::<AssetEvent<Image>>()
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
        .add_systems(
            PostUpdate,
            camera_system
                .in_set(CameraUpdateSystems)
                .before(TransformSystems::Propagate),
        );
    if input_before_camera {
        app.add_systems(
            PostUpdate,
            (navigation::navigate, input)
                .chain()
                .before(CameraUpdateSystems),
        );
    } else {
        // This was a legal execution order before the canvas declared the dependency.
        app.add_systems(
            PostUpdate,
            (navigation::navigate, input)
                .chain()
                .after(CameraUpdateSystems)
                .before(TransformSystems::Propagate),
        );
    }
    let cursor = Vec2::new(173.0, 267.0);
    let mut window = Window::default();
    window.resolution.set_scale_factor(dpi);
    window.resolution.set(1280.0, 800.0);
    window.focused = true;
    window.set_cursor_position(Some(cursor));
    app.world_mut().spawn((window, PrimaryWindow));
    let mut projection = OrthographicProjection::default_2d();
    projection.scale = 4.0;
    let camera = app
        .world_mut()
        .spawn((
            Camera2d,
            RenderTarget::default(),
            Projection::Orthographic(projection),
            canvas_camera_transform(Vec2::new(20.0, 30.0)),
        ))
        .id();
    app.world_mut()
        .resource_mut::<Time<Real>>()
        .advance_by(std::time::Duration::from_secs_f32(1.0 / 60.0));
    app.update();
    (app, camera, cursor)
}

fn rendered_world_at_cursor(app: &App, entity: Entity, cursor: Vec2) -> Vec2 {
    let camera = app.world().get::<Camera>(entity).unwrap();
    let transform = app.world().get::<GlobalTransform>(entity).unwrap();
    camera.viewport_to_world_2d(transform, cursor).unwrap()
}

#[test]
fn graph_clipboard_pastes_at_scaled_cursor_and_leaves_text_input_alone() {
    let (mut app, camera, cursor) = app(1.5, true);
    app.world_mut().resource_mut::<Interaction>().selected = HashSet::from([0, 1]);
    let command = |app: &mut App, key| {
        let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keys.reset_all();
        keys.press(KeyCode::ControlLeft);
        keys.press(key);
        app.update();
    };
    command(&mut app, KeyCode::KeyC);
    assert!(
        !app.world()
            .resource::<editor::Editor>()
            .clipboard
            .is_empty()
    );
    app.world_mut()
        .insert_resource(Scene(Document::blank().unwrap()));
    app.world_mut().insert_resource(Interaction::default());
    let point = rendered_world_at_cursor(&app, camera, cursor);
    command(&mut app, KeyCode::KeyV);
    // System clipboard decoding is asynchronous. Inject its no-image result so this
    // test remains independent of whatever the user's desktop clipboard contains.
    app.world_mut()
        .resource_scope(|world, mut editor: Mut<editor::Editor>| {
            let pending = editor.importing.take().expect("clipboard import started");
            assert!(pending.paste_graph);
            assert!((pending.center[0] - point.x).abs() < 0.01);
            assert!((pending.center[1] + point.y).abs() < 0.01);
            world.resource_scope(|world, mut scene: Mut<Scene>| {
                editor.paste_graph(
                    &mut scene.0,
                    &mut world.resource_mut::<Interaction>(),
                    pending.center,
                );
            });
        });
    let scene = app.world().resource::<Scene>();
    assert_eq!(scene.0.nodes.len(), 2);
    let bounds = scene.0.bounds();
    assert!((bounds[0] + bounds[2] * 0.5 - point.x).abs() < 0.01);
    assert!((bounds[1] + bounds[3] * 0.5 + point.y).abs() < 0.01);
    assert_eq!(app.world().resource::<Interaction>().selected.len(), 2);
    let node = scene.0.nodes[0].clone();
    app.world_mut()
        .resource_mut::<editor::Editor>()
        .begin(&node);
    for key in [KeyCode::KeyX, KeyCode::KeyV] {
        command(&mut app, key);
        assert_eq!(app.world().resource::<Scene>().0.nodes.len(), 2);
    }
}

#[test]
fn edge_selection_uses_rendered_paths_and_edit_reverse_delete_are_routed() {
    let (mut app, camera, cursor) = app(2.0, true);
    app.world_mut().resource_mut::<Scene>().0.edges.clear();
    let point = rendered_world_at_cursor(&app, camera, cursor);
    app.world_mut()
        .resource_mut::<Scene>()
        .0
        .edges
        .push(project_graph_bevy_canvas::GraphEdge {
            id: "edge".into(),
            members: vec![0, 1],
            rates: vec![[0.2, 0.5], [0.8, 0.5]],
            ..default()
        });
    app.world_mut().resource_mut::<edges::EdgePaths>().0.insert(
        "edge".into(),
        edges::EdgePath {
            anchor: point,
            segments: vec![[Vec2::new(-30.0, 0.0), Vec2::new(30.0, 0.0)]],
        },
    );
    app.world_mut()
        .resource_mut::<ButtonInput<MouseButton>>()
        .press(MouseButton::Left);
    app.update();
    assert_eq!(
        app.world().resource::<Interaction>().selected_edges,
        HashSet::from(["edge".into()])
    );
    assert!(app.world().resource::<Interaction>().selected.is_empty());
    app.world_mut()
        .resource_mut::<ButtonInput<MouseButton>>()
        .reset_all();
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::F2);
    app.update();
    assert!(app.world().resource::<editor::Editor>().active.is_some());
    app.world_mut().resource_mut::<editor::Editor>().active = None;
    app.world_mut().resource_mut::<editor::Editor>().capture = false;
    let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
    keys.reset_all();
    keys.press(KeyCode::ControlLeft);
    keys.press(KeyCode::KeyR);
    app.update();
    assert_eq!(app.world().resource::<Scene>().0.edges[0].members, [1, 0]);
    let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
    keys.reset_all();
    keys.press(KeyCode::Delete);
    app.update();
    assert!(app.world().resource::<Scene>().0.edges.is_empty());
    app.world_mut().resource_mut::<Scene>().0.undo();
    assert_eq!(app.world().resource::<Scene>().0.edges[0].members, [1, 0]);
}

#[test]
fn right_click_opens_context_but_drag_keeps_cutting_and_ctrl_f_keeps_camera() {
    let (mut app, camera, cursor) = app(1.0, true);
    app.world_mut()
        .resource_mut::<ButtonInput<MouseButton>>()
        .press(MouseButton::Right);
    app.update();
    let mut mouse = app.world_mut().resource_mut::<ButtonInput<MouseButton>>();
    mouse.clear();
    mouse.release(MouseButton::Right);
    app.update();
    assert!(
        app.world()
            .resource::<editor::Editor>()
            .tools
            .context
            .is_some()
    );
    app.world_mut().resource_mut::<editor::Editor>().tools = default();
    app.world_mut().resource_mut::<editor::Editor>().capture = false;
    let mut mouse = app.world_mut().resource_mut::<ButtonInput<MouseButton>>();
    mouse.reset_all();
    mouse.press(MouseButton::Right);
    app.update();
    let mut window = app
        .world_mut()
        .query_filtered::<&mut Window, With<PrimaryWindow>>()
        .single_mut(app.world_mut())
        .unwrap();
    window.set_cursor_position(Some(cursor + Vec2::new(40.0, 0.0)));
    let mut mouse = app.world_mut().resource_mut::<ButtonInput<MouseButton>>();
    mouse.clear();
    mouse.release(MouseButton::Right);
    app.update();
    assert!(
        app.world()
            .resource::<editor::Editor>()
            .tools
            .context
            .is_none()
    );
    let before = *app.world().get::<Transform>(camera).unwrap();
    let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
    keys.press(KeyCode::ControlLeft);
    keys.press(KeyCode::KeyF);
    app.update();
    assert!(app.world().resource::<editor::Editor>().tools.visible());
    assert_eq!(*app.world().get::<Transform>(camera).unwrap(), before);
}

fn scroll(app: &mut App, unit: MouseScrollUnit, delta: f32) {
    let window = app
        .world_mut()
        .query_filtered::<Entity, With<PrimaryWindow>>()
        .single(app.world())
        .unwrap();
    if unit == MouseScrollUnit::Pixel {
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::ControlLeft);
    } else {
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .release(KeyCode::ControlLeft);
    }
    app.world_mut().write_message(MouseWheel {
        window,
        unit,
        x: 0.0,
        y: delta,
        phase: TouchPhase::Moved,
    });
    app.update();
}

#[test]
fn wheel_zoom_keeps_rendered_cursor_anchor_across_frames_and_dpi() {
    for dpi in [1.0, 1.5, 1.75, 2.0] {
        let (mut app, camera, cursor) = app(dpi, true);
        let anchor = rendered_world_at_cursor(&app, camera, cursor);
        for (unit, delta) in [
            (MouseScrollUnit::Line, 1.0),
            (MouseScrollUnit::Line, 1.0),
            (MouseScrollUnit::Line, 0.0),
            (MouseScrollUnit::Line, -1.0),
            (MouseScrollUnit::Line, -2.0),
            (MouseScrollUnit::Pixel, 12.0),
            (MouseScrollUnit::Pixel, 6.0),
            (MouseScrollUnit::Pixel, -8.0),
            (MouseScrollUnit::Pixel, 0.0),
        ] {
            scroll(&mut app, unit, delta);
            let error = rendered_world_at_cursor(&app, camera, cursor).distance(anchor);
            assert!(
                error < 0.005,
                "rendered anchor drifted {error} world units at DPI {dpi}"
            );
        }
    }
}

#[test]
fn camera_before_input_reproduces_the_one_frame_anchor_jump() {
    let (mut app, camera, cursor) = app(1.75, false);
    let anchor = rendered_world_at_cursor(&app, camera, cursor);
    // A direct pixel gesture isolates matrix ordering from wheel easing.
    scroll(&mut app, MouseScrollUnit::Pixel, 60.0 * 1.75);
    let first = rendered_world_at_cursor(&app, camera, cursor);
    assert!(
        first.distance(anchor) > 100.0,
        "the old matrix/new transform mismatch must be observable"
    );
    scroll(&mut app, MouseScrollUnit::Pixel, 0.0);
    let next = rendered_world_at_cursor(&app, camera, cursor);
    assert!(
        next.distance(anchor) < 0.005,
        "the following idle frame catches the projection up"
    );
}

#[test]
fn enclosing_marquee_selects_existing_group_without_hijacking_inner_selection() {
    let doc = stress::synthetic(1, false, false, false).unwrap();
    let mut group = doc.nodes[0].clone();
    group.kind = "Section".into();
    group.rect = [0.0, 0.0, 300.0, 200.0];
    assert!(marquee_hit(
        &group,
        Vec2::new(-1.0, 1.0),
        Vec2::new(301.0, -201.0)
    ));
    assert!(marquee_hit(
        &group,
        Vec2::new(301.0, -201.0),
        Vec2::new(-1.0, 1.0)
    ));
    assert!(!marquee_hit(
        &group,
        Vec2::new(30.0, -30.0),
        Vec2::new(250.0, -150.0)
    ));
    assert!(!marquee_hit(
        &group,
        Vec2::new(-1.0, 1.0),
        Vec2::new(150.0, -150.0)
    ));
    group.kind = "TextNode".into();
    assert!(marquee_hit(
        &group,
        Vec2::new(-1.0, 1.0),
        Vec2::new(150.0, -150.0)
    ));
}

#[test]
#[ignore = "requires pinned tutorial fixture"]
fn select_all_then_group_preserves_tutorial_groups_and_connections() {
    let (mut app, _, _) = app(1.75, true);
    let doc = Document::open(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/tutorial-shortcut-keys.mio"),
    )
    .unwrap();
    let before = doc.nodes.clone();
    let edges = doc.edges.clone();
    assert!(
        before
            .iter()
            .any(|n| n.kind == "Section" && !n.children.is_empty())
    );
    app.insert_resource(Scene(doc));
    {
        let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keys.press(KeyCode::ControlLeft);
        keys.press(KeyCode::KeyA);
    }
    app.update();
    assert_eq!(
        app.world().resource::<Interaction>().selected.len(),
        before.len(),
        "whole-stage selection must include old Section frames"
    );
    {
        let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keys.release(KeyCode::KeyA);
        keys.clear();
        keys.press(KeyCode::KeyG);
    }
    app.update();
    let selection = app.world().resource::<Interaction>().selected.clone();
    let mut scene = app.world_mut().resource_mut::<Scene>();
    assert_eq!(scene.0.nodes.len(), before.len() + 1);
    assert_eq!(scene.0.edges, edges);
    for (node, old) in scene.0.nodes.iter().zip(&before) {
        assert_eq!(
            node.children, old.children,
            "old nested groups must not be emptied: {}",
            node.id
        );
    }
    let moving = scene.0.expand_selection(&selection);
    assert_eq!(moving.len(), scene.0.nodes.len());
    let grouped = scene.0.nodes.clone();
    scene.0.begin("移动整图分组");
    move_nodes(&mut scene.0, &moving, Vec2::new(321.0, -654.0));
    scene.0.refresh_group_bounds().unwrap();
    scene.0.finish();
    for (node, old) in scene.0.nodes.iter().zip(&grouped) {
        assert!((node.rect[0] - old.rect[0] - 321.0).abs() < 0.01);
        assert!((node.rect[1] - old.rect[1] - 654.0).abs() < 0.01);
        assert_eq!(node.children, old.children);
    }
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("grouped.mio");
    scene.0.save(&file).unwrap();
    let saved = Document::open(&file).unwrap();
    for node in &scene.0.nodes {
        assert_eq!(
            saved
                .nodes
                .iter()
                .find(|n| n.id == node.id)
                .unwrap()
                .children,
            node.children
        );
    }
    scene.0.undo();
    assert_eq!(scene.0.nodes, grouped);
    scene.0.undo();
    assert_eq!(scene.0.nodes, before);
}

#[test]
fn deletion_and_undo_keep_visuals_attached_to_stable_node_ids() {
    let mut app = App::new();
    app.insert_resource(Scene(stress::synthetic(2, false, false, false).unwrap()))
        .init_resource::<Interaction>()
        .init_resource::<editor::Editor>()
        .init_resource::<Assets<Image>>()
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
        });
    app.insert_resource(NodeAssets {
        font: default(),
        corners: theme::NodeCorners::new(&mut Assets::default()),
        textures: default(),
    });
    app.add_systems(PostUpdate, sync_nodes);
    app.update();
    let survivor = app.world().resource::<Scene>().0.nodes[1].id.clone();
    app.world_mut().resource_mut::<Interaction>().selected = HashSet::from([1]);
    app.world_mut().resource_mut::<Scene>().0.selection =
        project_graph_bevy_canvas::document::Selection {
            nodes: HashSet::from([survivor.clone()]),
            edges: HashSet::new(),
        };
    app.world_mut()
        .resource_mut::<Scene>()
        .0
        .delete_objects(&HashSet::from([0]), &HashSet::new())
        .unwrap();
    app.update();
    let mut roots = app.world_mut().query::<(&NodeVisual, &NodeIdentity)>();
    let live: Vec<_> = roots
        .iter(app.world())
        .map(|(i, id)| (i.0, id.0.clone()))
        .collect();
    assert_eq!(live, vec![(0, survivor.clone())]);
    assert_eq!(
        app.world().resource::<Interaction>().selected,
        HashSet::from([0])
    );
    app.world_mut().resource_mut::<Scene>().0.undo();
    app.update();
    assert_eq!(roots.iter(app.world()).count(), 2);
    assert_eq!(
        app.world().resource::<Interaction>().selected,
        HashSet::from([1])
    );
    for (index, id) in roots.iter(app.world()) {
        assert_eq!(id.0, app.world().resource::<Scene>().0.nodes[index.0].id);
    }
}

#[test]
fn movement_particles_require_node_motion_and_an_active_drag() {
    let mut app = App::new();
    let doc = stress::synthetic(1, false, false, false).unwrap();
    let origin = center(&doc.nodes[0]);
    app.insert_resource(Scene(doc))
        .init_resource::<Interaction>()
        .init_resource::<physics::PhysicsEditor>()
        .init_resource::<effects::Effects>()
        .add_systems(Update, movement_effects);
    let root = app
        .world_mut()
        .spawn((
            NodeVisual(0),
            TrailPosition(origin),
            Transform::from_translation(origin.extend(1.0)),
        ))
        .id();
    app.update();
    assert!(!app.world().resource::<effects::Effects>().active());
    // Undo/layout changes are not motion feedback.
    app.world_mut()
        .get_mut::<Transform>(root)
        .unwrap()
        .translation
        .x += 10.0;
    app.update();
    assert!(!app.world().resource::<effects::Effects>().active());
    app.world_mut()
        .resource_mut::<Interaction>()
        .moving
        .insert(0);
    app.update();
    assert!(!app.world().resource::<effects::Effects>().active());
    app.world_mut()
        .get_mut::<Transform>(root)
        .unwrap()
        .translation
        .x += 10.0;
    app.update();
    assert!(app.world().resource::<effects::Effects>().active());
}

#[test]
fn pasted_image_uploads_texture_and_keeps_it_through_undo_redo() {
    let mut source = Document::blank().unwrap();
    let mut encoded = std::io::Cursor::new(Vec::new());
    image::RgbaImage::from_pixel(2, 2, image::Rgba([255, 0, 0, 255]))
        .write_to(&mut encoded, image::ImageFormat::Png)
        .unwrap();
    source
        .import_image([0.0; 2], [20.0; 2], "red.png", encoded.into_inner())
        .unwrap();
    let mut clipboard = project_graph_bevy_canvas::document::GraphClipboard::default();
    clipboard.copy(&source, &HashSet::from([0])).unwrap();
    let (mut app, _, _) = app(1.0, true);
    app.insert_resource(NodeAssets {
        font: default(),
        corners: theme::NodeCorners::new(&mut Assets::default()),
        textures: default(),
    })
    .add_systems(PostUpdate, sync_nodes.after(input));
    let index = *clipboard
        .paste(&mut app.world_mut().resource_mut::<Scene>().0, [500.0, 0.0])
        .unwrap()
        .iter()
        .next()
        .unwrap();
    app.update();
    let name = app.world().resource::<Scene>().0.nodes[index]
        .attachment
        .clone()
        .unwrap();
    let texture = app.world().resource::<NodeAssets>().textures[&name].clone();
    let image = app
        .world()
        .resource::<Assets<Image>>()
        .get(&texture)
        .unwrap();
    assert_eq!(image.size(), UVec2::splat(2));
    assert_eq!(
        image.data.as_deref(),
        Some([255, 0, 0, 255].repeat(4).as_slice())
    );
    app.world_mut().resource_mut::<Scene>().0.undo();
    app.update();
    app.world_mut().resource_mut::<Scene>().0.redo();
    app.update();
    assert_eq!(
        app.world().resource::<NodeAssets>().textures[&name],
        texture
    );
    assert!(
        app.world_mut()
            .query::<&Sprite>()
            .iter(app.world())
            .any(|sprite| sprite.image == texture)
    );
}

#[test]
fn grouping_shortcut_moves_members_and_undo_removes_the_visual() {
    let (mut app, _, _) = app(1.75, true);
    app.insert_resource(NodeAssets {
        font: default(),
        corners: theme::NodeCorners::new(&mut Assets::default()),
        textures: default(),
    });
    app.add_systems(PostUpdate, sync_nodes.after(input));
    app.world_mut().resource_mut::<Interaction>().selected = HashSet::from([0, 1]);
    {
        let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keys.press(KeyCode::ControlLeft);
        keys.press(KeyCode::KeyG);
    }
    app.update();
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .reset_all();
    let group = app.world().resource::<Scene>().0.nodes.len() - 1;
    assert_eq!(
        app.world().resource::<Interaction>().selected,
        HashSet::from([group])
    );
    let mut visuals = app.world_mut().query::<&NodeVisual>();
    assert_eq!(visuals.iter(app.world()).count(), 3);
    let before = app.world().resource::<Scene>().0.nodes.clone();
    {
        let mut scene = app.world_mut().resource_mut::<Scene>();
        let moving = scene.0.expand_selection(&HashSet::from([group]));
        assert_eq!(moving.len(), 3);
        scene.0.begin("移动分组");
        move_nodes(&mut scene.0, &moving, Vec2::new(21.0, -13.0));
        scene.0.finish();
        for (node, old) in scene.0.nodes.iter().zip(&before) {
            assert_eq!(node.rect[0], old.rect[0] + 21.0);
            assert_eq!(node.rect[1], old.rect[1] + 13.0);
        }
        scene.0.undo();
        scene.0.undo();
    }
    app.update();
    assert_eq!(visuals.iter(app.world()).count(), 2);
    assert_eq!(
        app.world().resource::<Interaction>().selected,
        HashSet::from([0, 1])
    );
    app.world_mut().resource_mut::<Scene>().0.redo();
    app.update();
    assert_eq!(visuals.iter(app.world()).count(), 3);
    assert_eq!(
        app.world().resource::<Interaction>().selected,
        HashSet::from([group])
    );
    // Text input owns Ctrl+G while an editor or dialog captures keys.
    app.world_mut().resource_mut::<editor::Editor>().capture = true;
    app.world_mut().resource_mut::<Interaction>().selected = HashSet::from([0]);
    let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
    keys.press(KeyCode::ControlLeft);
    keys.press(KeyCode::KeyG);
    app.update();
    assert_eq!(app.world().resource::<Scene>().0.nodes.len(), 3);
}

#[test]
fn double_click_enters_inline_edit_without_moving_or_zooming_the_camera() {
    let (mut app, camera, _) = app(1.75, true);
    let node = app.world().resource::<Scene>().0.nodes[0].clone();
    let transform = *app.world().get::<Transform>(camera).unwrap();
    let Projection::Orthographic(projection) = app.world().get::<Projection>(camera).unwrap()
    else {
        panic!()
    };
    let scale = projection.scale;
    let screen = (center(&node) - transform.translation.truncate()) / scale;
    let mut windows = app
        .world_mut()
        .query_filtered::<&mut Window, With<PrimaryWindow>>();
    let mut window = windows.single_mut(app.world_mut()).unwrap();
    let cursor = Vec2::new(
        window.width() * 0.5 + screen.x,
        window.height() * 0.5 - screen.y,
    );
    window.set_cursor_position(Some(cursor));
    app.world_mut().resource_mut::<Interaction>().last_click = Some((0, Instant::now(), cursor));
    app.world_mut()
        .resource_mut::<ButtonInput<MouseButton>>()
        .press(MouseButton::Left);
    app.update();
    assert!(app.world().resource::<editor::Editor>().is_editing(&node));
    assert_eq!(*app.world().get::<Transform>(camera).unwrap(), transform);
    let Projection::Orthographic(projection) = app.world().get::<Projection>(camera).unwrap()
    else {
        panic!()
    };
    assert_eq!(projection.scale, scale);
}
