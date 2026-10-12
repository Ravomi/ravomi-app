use super::*;

#[test]
fn double_click_completes_only_on_second_release_and_consumes_the_pair() {
    let mut gesture = Gesture::default();
    let cursor = Vec2::new(100.0, 80.0);
    let view = Vec3::new(20.0, 30.0, 1.0);
    let time = Instant::now();
    gesture.press(cursor, view);
    assert!(!gesture.release(cursor, view, time, true));
    gesture.press(cursor, view);
    assert!(gesture.release(cursor, view, time + Duration::from_millis(100), true));
    gesture.press(cursor, view);
    assert!(!gesture.release(cursor, view, time + Duration::from_millis(200), true));
}

#[test]
fn dragging_returning_to_start_or_navigation_does_not_create() {
    for camera_moved in [false, true] {
        let mut gesture = Gesture::default();
        let cursor = Vec2::ZERO;
        let view = Vec3::Z;
        let time = Instant::now();
        gesture.press(cursor, view);
        assert!(!gesture.release(cursor, view, time, true));
        gesture.press(cursor, view);
        if camera_moved {
            gesture.observe(cursor, Vec3::new(0.0, 0.0, 2.0));
        } else {
            gesture.observe(Vec2::new(10.0, 0.0), view);
        }
        gesture.observe(cursor, view);
        assert!(!gesture.release(cursor, view, time + Duration::from_millis(100), true));
    }
}

#[test]
fn relaxed_double_click_accepts_touchpad_drift_and_slower_second_tap() {
    let mut gesture = Gesture::default();
    let view = Vec3::Z;
    let time = Instant::now();
    gesture.press(Vec2::ZERO, view);
    assert!(!gesture.release(Vec2::new(6.0, 0.0), view, time, true));
    gesture.press(Vec2::new(6.0, 0.0), view);
    assert!(gesture.release(
        Vec2::new(12.0, 0.0),
        view,
        time + Duration::from_millis(450),
        true
    ));
}

#[test]
fn timeout_distance_occupied_release_and_cancellation_reject_double_click() {
    for (delay, cursor, empty, cancel) in [
        (501, Vec2::ZERO, true, false),
        (100, Vec2::new(21.0, 0.0), true, false),
        (100, Vec2::ZERO, false, false),
        (100, Vec2::ZERO, true, true),
    ] {
        let mut gesture = Gesture::default();
        let view = Vec3::Z;
        let time = Instant::now();
        gesture.press(Vec2::ZERO, view);
        assert!(!gesture.release(Vec2::ZERO, view, time, true));
        gesture.press(cursor, view);
        if cancel {
            gesture.cancel();
        }
        assert!(!gesture.release(cursor, view, time + Duration::from_millis(delay), empty));
    }
}

fn input_app() -> App {
    let mut app = App::new();
    app.insert_resource(Scene(stress::synthetic(2, false, false, false).unwrap()))
        .init_resource::<editor::Editor>()
        .init_resource::<physics::PhysicsEditor>()
        .init_resource::<effects::Effects>()
        .init_resource::<edges::EdgePaths>()
        .init_resource::<Interaction>()
        .init_resource::<ButtonInput<MouseButton>>()
        .init_resource::<ButtonInput<KeyCode>>()
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
    let mut window = Window {
        focused: true,
        ..default()
    };
    window.resolution.set(1280.0, 800.0);
    window.set_cursor_position(Some(Vec2::new(1200.0, 700.0)));
    app.world_mut().spawn((window, PrimaryWindow));
    app.world_mut().spawn((
        Camera2d,
        Transform::default(),
        Projection::Orthographic(OrthographicProjection::default_2d()),
    ));
    app
}

fn click(app: &mut App) {
    {
        let mut mouse = app.world_mut().resource_mut::<ButtonInput<MouseButton>>();
        mouse.clear();
        mouse.press(MouseButton::Left);
    }
    app.update();
    {
        let mut mouse = app.world_mut().resource_mut::<ButtonInput<MouseButton>>();
        mouse.clear();
        mouse.release(MouseButton::Left);
    }
    app.update();
}

#[test]
fn empty_node_double_click_accepts_slower_tap_with_small_drift() {
    let mut app = input_app();
    {
        let mut scene = app.world_mut().resource_mut::<Scene>();
        scene.0.nodes[0].text.clear();
        scene.0.nodes[0].rect = [-30.0, -38.0, 60.0, 76.0];
    }
    let world = app.world_mut();
    world
        .query::<&mut Window>()
        .single_mut(world)
        .unwrap()
        .set_cursor_position(Some(Vec2::new(640.0, 400.0)));
    click(&mut app);
    app.world_mut().resource_mut::<Interaction>().last_click = Some((
        0,
        Instant::now() - Duration::from_millis(450),
        Vec2::new(634.0, 400.0),
    ));
    click(&mut app);
    let scene = app.world().resource::<Scene>();
    assert!(
        app.world()
            .resource::<editor::Editor>()
            .is_editing(&scene.0.nodes[0])
    );
    assert_eq!(scene.0.nodes.len(), 2);
}

#[test]
fn empty_canvas_double_click_creates_selects_and_undoes_one_node_without_dragging() {
    let mut app = input_app();
    click(&mut app);
    assert_eq!(app.world().resource::<Scene>().0.nodes.len(), 2);
    click(&mut app);
    let scene = &app.world().resource::<Scene>().0;
    assert_eq!(scene.nodes.len(), 3);
    assert_eq!(scene.nodes[2].text, "...");
    assert_eq!(center(&scene.nodes[2]), Vec2::new(560.0, -300.0));
    let interaction = app.world().resource::<Interaction>();
    assert_eq!(interaction.selected, HashSet::from([2]));
    assert!(interaction.moving.is_empty());
    assert!(interaction.drag_start.is_none());
    assert!(!interaction.selecting);
    assert!(!app.world().resource::<effects::Effects>().active());
    assert!(
        app.world()
            .resource::<editor::Editor>()
            .is_editing(&scene.nodes[2])
    );
    assert!(app.world().resource::<editor::Editor>().capture);
    app.world_mut().resource_mut::<Scene>().0.undo();
    assert_eq!(app.world().resource::<Scene>().0.nodes.len(), 2);
}

#[test]
fn ui_capture_and_navigation_modifiers_do_not_create_nodes() {
    for key in [
        None,
        Some(KeyCode::Space),
        Some(KeyCode::AltLeft),
        Some(KeyCode::KeyE),
    ] {
        let mut app = input_app();
        if let Some(key) = key {
            app.world_mut()
                .resource_mut::<ButtonInput<KeyCode>>()
                .press(key);
        } else {
            app.world_mut().resource_mut::<editor::Editor>().capture = true;
        }
        click(&mut app);
        click(&mut app);
        assert_eq!(app.world().resource::<Scene>().0.nodes.len(), 2);
    }
}

#[test]
fn an_untitled_section_interior_can_create_but_a_title_or_locked_group_cannot() {
    for (title, locked, creates) in [
        ("", false, true),
        ("Group", false, false),
        ("", true, false),
    ] {
        let mut app = input_app();
        let parent_id = "creation-parent".to_owned();
        app.world_mut()
            .resource_mut::<Scene>()
            .0
            .nodes
            .push(GraphNode {
                id: parent_id.clone(),
                kind: "Section".into(),
                // The click is 20 world units below the top, far from all borders.
                rect: [400.0, 280.0, 400.0, 300.0],
                text: title.into(),
                url: None,
                font_size: 32.0,
                auto_size: false,
                color: [0.0; 4],
                attachment: None,
                source_name: None,
                children: vec![],
                locked,
                collapsed: false,
                stroke: vec![],
            });
        click(&mut app);
        click(&mut app);
        let doc = &app.world().resource::<Scene>().0;
        assert_eq!(doc.nodes.len(), if creates { 4 } else { 3 });
        if creates {
            assert_eq!(doc.nodes[2].children, [doc.nodes[3].id.clone()]);
            assert_eq!(
                app.world().resource::<Interaction>().selected,
                HashSet::from([3])
            );
        }
    }
}

#[test]
fn double_clicking_a_visible_edge_does_not_create_a_text_node() {
    let mut app = input_app();
    app.world_mut().resource_mut::<edges::EdgePaths>().0.insert(
        "edge".into(),
        edges::EdgePath {
            anchor: Vec2::new(560.0, -300.0),
            segments: vec![[Vec2::new(-100.0, 0.0), Vec2::new(100.0, 0.0)]],
        },
    );
    click(&mut app);
    click(&mut app);
    assert_eq!(app.world().resource::<Scene>().0.nodes.len(), 2);
}
