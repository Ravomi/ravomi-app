use super::*;

#[test]
fn cutting_a_node_emits_only_line_and_contact_feedback() {
    use super::super::{Interaction, cutting, edges, editor, physics};
    use project_graph_bevy_canvas::document::Document;

    let mut doc = Document::blank().unwrap();
    doc.create_text_node([0.0, 0.0]).unwrap();
    let before = doc.nodes.clone();
    let mut interaction = Interaction::default();
    let mut physics = physics::PhysicsEditor::default();
    let mut editor = editor::Editor::default();
    let paths = edges::EdgePaths::default();
    let mut effects = Effects::default();
    let mut mouse = ButtonInput::default();
    for (cursor, released) in [
        (Vec2::new(-100.0, 0.0), false),
        (Vec2::new(100.0, 0.0), true),
    ] {
        if released {
            mouse.clear();
            mouse.release(MouseButton::Right);
        } else {
            mouse.press(MouseButton::Right);
        }
        assert!(cutting::input(
            &mouse,
            &ButtonInput::default(),
            cursor,
            1.0,
            &mut doc,
            None,
            &mut interaction,
            &mut physics,
            &mut editor,
            &paths,
            &mut effects,
        ));
    }
    assert!(doc.nodes.is_empty());
    assert!(effects.active());
    assert!(
        effects
            .items
            .iter()
            .all(|effect| matches!(effect.shape, Shape::Cut { .. } | Shape::Contact { .. }))
    );
    effects.advance(0.1);
    for scale in [0.25, 1.0, 4.0] {
        let mesh = geometry(&effects, scale).unwrap();
        let (positions, _) = vertices(&mesh);
        assert!(positions.iter().all(|point| point[1].abs() <= 10.0 * scale));
    }
    effects.advance(0.16);
    assert!(!effects.active());
    doc.undo();
    assert_eq!(doc.nodes, before);
}

#[test]
fn editing_feedback_stays_on_the_node_edge_at_every_zoom() {
    for scale in [0.25, 1.0, 4.0] {
        let mut effects = Effects::default();
        effects.begin_edit(Vec2::ZERO, Vec2::new(60.0, 76.0), true);
        for _ in 0..3 {
            let mesh = geometry(&effects, scale).unwrap();
            let (positions, colors) = vertices(&mesh);
            assert!(
                positions.iter().all(|p| {
                    p[0].abs() <= 30.0 + 1.6 * scale && p[1].abs() <= 38.0 + 1.6 * scale
                })
            );
            assert!(colors.iter().all(|color| color[3] <= 0.35));
            effects.advance(0.05);
        }
        effects.advance(0.05);
        assert!(!effects.active());
    }
}

#[test]
fn connection_flow_follows_the_curve_and_finishes_without_residual_animation() {
    for hz in [30.0, 60.0, 144.0] {
        let mut effects = Effects::default();
        effects.connected(vec![
            Vec2::ZERO,
            Vec2::new(100.0, 60.0),
            Vec2::new(200.0, 0.0),
        ]);
        effects.advance(0.1);
        let mesh = geometry(&effects, 1.0).unwrap();
        let (positions, colors) = vertices(&mesh);
        assert!(!positions.is_empty());
        assert!(
            positions
                .iter()
                .all(|p| p[0] >= -3.0 && p[0] <= 203.0 && p[1] >= -3.0 && p[1] <= 63.0)
        );
        assert!(colors.iter().all(|color| color[3] > 0.0 && color[3] < 1.0));
        for _ in 0..(hz * 0.4) as usize + 1 {
            effects.advance(1.0 / hz);
        }
        assert!(!effects.active());
    }
}

#[test]
fn effect_material_blends_vertex_alpha_instead_of_making_halos_opaque() {
    assert_eq!(
        effect_material().alpha_mode,
        bevy::sprite_render::AlphaMode2d::Blend
    );
}

fn vertices(mesh: &Mesh) -> (&[[f32; 3]], &[[f32; 4]]) {
    use bevy::mesh::VertexAttributeValues;
    let Some(VertexAttributeValues::Float32x3(positions)) =
        mesh.attribute(Mesh::ATTRIBUTE_POSITION)
    else {
        panic!()
    };
    let Some(VertexAttributeValues::Float32x4(colors)) = mesh.attribute(Mesh::ATTRIBUTE_COLOR)
    else {
        panic!()
    };
    (positions, colors)
}

#[test]
fn preview_uses_a_thin_catppuccin_line_at_all_zooms() {
    for scale in [0.05, 0.25, 1.0, 4.0, 16.0] {
        let mut effects = Effects::default();
        effects.cutting_preview(Some(CutPreview {
            line: [Vec2::ZERO, Vec2::new(400.0, 0.0)],
            edges: vec![],
        }));
        assert!(
            !effects.active(),
            "stationary preview does not need animation frames"
        );
        let mesh = geometry(&effects, scale).unwrap();
        let (positions, colors) = vertices(&mesh);
        let mut max_y: f32 = 0.0;
        let expected = Color::srgba_u8(243, 139, 168, 179)
            .to_linear()
            .to_f32_array();
        for (p, c) in positions.iter().zip(colors) {
            max_y = max_y.max(p[1].abs());
            assert_eq!(*c, expected);
        }
        assert!((max_y / scale - 0.75).abs() < 0.001);
        effects.cutting_preview(None);
        assert_eq!(geometry(&effects, scale).unwrap().count_vertices(), 0);
    }
}

#[test]
fn release_blade_stays_thin_and_contracts_without_self_crossing() {
    for scale in [0.05, 0.25, 1.0, 4.0, 16.0] {
        for progress in [0.0, 0.5, 0.9, 0.99] {
            let p = cut_blade(Vec2::ZERO, Vec2::new(400.0, 0.0), 40.0, progress, scale);
            assert!(p[0].x <= p[1].x && p[1].x <= p[2].x);
            assert!((p[1].y - p[3].y).abs() / scale <= 8.0 * (1.0 - progress) + 0.0001);
            assert_eq!(p[2], Vec2::new(400.0, 0.0));
        }
    }
}

#[test]
fn acute_blade_round_joins_cannot_extend_beyond_the_halo_radius() {
    use lyon_tessellation::geom::LineSegment;
    for scale in [0.05, 0.25, 1.0, 4.0, 16.0] {
        for progress in [0.0, 0.99, 0.9999] {
            let points = cut_blade(Vec2::ZERO, Vec2::new(400.0, 0.0), 40.0, progress, scale);
            let mut buffers = VertexBuffers::new();
            glow(
                &mut buffers,
                &polygon(&points),
                Color::srgb(1.0, 0.0, 0.0),
                0.0,
                scale,
            )
            .unwrap();
            for v in buffers.vertices {
                let p = point(v.position[0], v.position[1]);
                let distance = (0..4)
                    .map(|i| {
                        let a = points[i];
                        let b = points[(i + 1) % 4];
                        LineSegment {
                            from: point(a.x, a.y),
                            to: point(b.x, b.y),
                        }
                        .closest_point(p)
                        .distance_to(p)
                    })
                    .fold(f32::INFINITY, f32::min);
                assert!(
                    distance / scale <= 15.02,
                    "round join produced a spike: {distance} at scale {scale}"
                );
            }
        }
    }
}

#[test]
#[ignore = "requires a GPU; captures actual production effects material at three zoom levels under results/effects-gpu-*.png"]
fn gpu_blends_cut_halos_at_multiple_zooms() {
    use bevy::{
        camera::RenderTarget,
        render::{
            render_resource::TextureFormat,
            view::screenshot::{Screenshot, ScreenshotCaptured},
        },
        window::ExitCondition,
        winit::WinitPlugin,
    };
    use std::time::{Duration, Instant};

    #[derive(Resource, Default)]
    struct Capture(Option<Image>);

    fn capture(
        app: &mut App,
        target: &Handle<Image>,
        deadline: Instant,
        expect_ink: bool,
    ) -> image::RgbImage {
        loop {
            app.world_mut().resource_mut::<Capture>().0 = None;
            app.world_mut()
                .spawn(Screenshot::image(target.clone()))
                .observe(
                    |capture: On<ScreenshotCaptured>, mut saved: ResMut<Capture>| {
                        saved.0 = Some(capture.image.clone());
                    },
                );
            while app.world().resource::<Capture>().0.is_none() {
                app.update();
                assert!(Instant::now() < deadline, "effects GPU capture timed out");
                std::thread::yield_now();
            }
            let captured = app.world_mut().resource_mut::<Capture>().0.take().unwrap();
            let image = captured.try_into_dynamic().unwrap().to_rgb8();
            if !expect_ink || image.pixels().any(|p| p.0.iter().any(|&v| v > 10)) {
                return image;
            }
            // A first capture can arrive before the GPU pipeline has finished
            // compiling. Wait for visible output, not an arbitrary sleep.
            assert!(
                Instant::now() < deadline,
                "effects GPU pipeline never rendered"
            );
        }
    }

    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: None,
                exit_condition: ExitCondition::DontExit,
                ..default()
            })
            .disable::<WinitPlugin>(),
    )
    .insert_resource(ClearColor(Color::BLACK))
    .init_resource::<super::super::editor::Editor>()
    .init_resource::<Capture>()
    .add_systems(
        PostUpdate,
        draw.before(bevy::transform::TransformSystems::Propagate),
    );
    install(&mut app);
    let deadline = Instant::now() + Duration::from_secs(90);
    while app.plugins_state() != bevy::app::PluginsState::Ready {
        assert!(
            Instant::now() < deadline,
            "GPU plugins did not become ready"
        );
        bevy::tasks::tick_global_task_pools_on_main_thread();
        std::thread::yield_now();
    }
    app.finish();
    app.cleanup();
    let target = app
        .world_mut()
        .resource_mut::<Assets<Image>>()
        .add(Image::new_target_texture(
            512,
            128,
            TextureFormat::Rgba8UnormSrgb,
            None,
        ));
    let camera = app
        .world_mut()
        .spawn((Camera2d, RenderTarget::Image(target.clone().into())))
        .id();
    std::fs::create_dir_all("results").unwrap();

    // Deliberately omit the age update system: freeze each production effect
    // at a known progress while pipeline compilation and readback complete.
    for (index, scale) in [0.25, 1.0, 4.0].into_iter().enumerate() {
        if let Projection::Orthographic(projection) =
            &mut *app.world_mut().get_mut::<Projection>(camera).unwrap()
        {
            projection.scale = scale;
        }
        for released in [false, true] {
            {
                let mut effects = app.world_mut().resource_mut::<Effects>();
                effects.clear();
                let from = Vec2::new(-160.0 * scale, 0.0);
                let to = Vec2::new(160.0 * scale, 0.0);
                if released {
                    effects.cut_line(from, to);
                    effects.advance(0.1);
                } else {
                    effects.cutting_preview(Some(CutPreview {
                        line: [from, to],
                        edges: vec![],
                    }));
                }
            }
            // Run extraction after the projection/mesh mutation before asking
            // for a screenshot, avoiding capture of the previous test case.
            app.update();
            app.update();
            let image = capture(&mut app, &target, deadline, true);
            let phase = if released { "release" } else { "preview" };
            image
                .save(format!("results/effects-gpu-{phase}-{index}.png"))
                .unwrap();
            let visible_rows: Vec<_> = (0..128)
                .filter(|&y| {
                    (96..416).any(|x| {
                        let p = image.get_pixel(x, y).0;
                        p.iter().any(|&c| c > 30)
                    })
                })
                .collect();
            assert!(
                !visible_rows.is_empty(),
                "cut feedback missing at {scale}, {phase}"
            );
            assert!(
                visible_rows.len() <= 10,
                "cut feedback too wide at {scale}, {phase}: {visible_rows:?}"
            );
            for y in [40, 87] {
                assert!(
                    (96..416).all(|x| image.get_pixel(x, y).0 == [0, 0, 0]),
                    "halo escaped its screen-space radius"
                );
            }
        }
    }

    app.world_mut().resource_mut::<Effects>().clear();
    app.update();
    app.update();
    let cleared = capture(&mut app, &target, deadline, false);
    assert!(
        cleared.pixels().all(|p| p.0 == [0, 0, 0]),
        "expired mesh remained visible"
    );
}

#[test]
fn cutting_preview_stays_on_the_cut_line_at_every_zoom() {
    for scale in [0.1, 0.5, 1.0, 2.0, 20.0] {
        let preview = CutPreview {
            line: [Vec2::new(1000.0, 0.0); 2],
            edges: vec![],
        };
        let mut effects = Effects::default();
        effects.cutting_preview(Some(preview));
        let mesh = geometry(&effects, scale).unwrap();
        let (positions, _) = vertices(&mesh);
        assert!(!positions.is_empty());
        assert!(
            positions
                .iter()
                .all(|p| (p[0] - 1000.0).abs() <= 5.0 * scale && p[1].abs() <= 5.0 * scale)
        );
    }
}

/// Deterministic software artifact of the actual tessellated geometry. The
/// rows use projection scales 0.25 / 1 / 4 (400% / 100% / 25% zoom). This does
/// not replace the GPU material/blending regression or an application capture.
#[test]
#[ignore = "writes deterministic preview/release PNGs under results; no GPU required"]
fn export_cutting_visual_reference() {
    use resvg::tiny_skia;
    std::fs::create_dir_all("results").unwrap();
    for released in [false, true] {
        let mut pixels = tiny_skia::Pixmap::new(900, 600).unwrap();
        pixels.fill(tiny_skia::Color::from_rgba8(30, 30, 46, 255));
        for (row, scale) in [0.25, 1.0, 4.0].into_iter().enumerate() {
            let y = (500.0 - row as f32 * 200.0) * scale;
            let from = Vec2::new(350.0 * scale, y + 90.0 * scale);
            let to = Vec2::new(550.0 * scale, y - 90.0 * scale);
            let mut effects = Effects::default();
            if released {
                effects.cut_line(from, to);
                effects.advance(0.1);
            } else {
                effects.cutting_preview(Some(CutPreview {
                    line: [from, to],
                    edges: vec![],
                }));
            }
            let mesh = geometry(&effects, scale).unwrap();
            let (positions, colors) = vertices(&mesh);
            let Some(Indices::U32(indices)) = mesh.indices() else {
                panic!()
            };
            for triangle in indices.as_chunks::<3>().0 {
                let mut path = tiny_skia::PathBuilder::new();
                for (i, index) in triangle.iter().enumerate() {
                    let p = positions[*index as usize];
                    let (x, y) = (p[0] / scale, 600.0 - p[1] / scale);
                    if i == 0 {
                        path.move_to(x, y);
                    } else {
                        path.line_to(x, y);
                    }
                }
                path.close();
                let Some(path) = path.finish() else {
                    continue;
                };
                let c = colors[triangle[0] as usize];
                let [r, g, b, a] = Color::linear_rgba(c[0], c[1], c[2], c[3])
                    .to_srgba()
                    .to_u8_array();
                let mut paint = tiny_skia::Paint::default();
                paint.set_color_rgba8(r, g, b, a);
                paint.anti_alias = false;
                pixels.fill_path(
                    &path,
                    &paint,
                    tiny_skia::FillRule::Winding,
                    tiny_skia::Transform::identity(),
                    None,
                );
            }
        }
        let name = if released {
            "results/cutting-release-reference.png"
        } else {
            "results/cutting-preview-reference.png"
        };
        image::save_buffer(name, pixels.data(), 900, 600, image::ColorType::Rgba8).unwrap();
    }
}

#[test]
fn master_lifetimes_are_frame_rate_independent_and_eventually_idle() {
    for fps in [30, 60, 120, 144] {
        let mut effects = Effects::default();
        effects.begin_edit(Vec2::ZERO, Vec2::splat(100.0), true);
        effects.move_trail(0, Vec2::ZERO, Vec2::splat(100.0), Vec2::X * 2.0);
        for _ in 0..fps / 2 {
            effects.advance(1.0 / fps as f32);
        }
        assert!(!effects.active(), "effects remained at {fps} fps");
    }
    let mut effects = Effects::default();
    effects.begin_edit(Vec2::ZERO, Vec2::splat(100.0), true);
    effects.advance(0.25);
    assert!(!effects.active());
}

#[test]
fn movement_feedback_follows_each_node_without_accumulating_particles() {
    let mut effects = Effects::default();
    effects.move_trail(0, Vec2::ZERO, Vec2::splat(100.0), Vec2::X * 0.9);
    assert!(!effects.active());
    for step in 0..200 {
        effects.move_trail(0, Vec2::X * step as f32, Vec2::splat(100.0), Vec2::X * 2.0);
        effects.advance(1.0 / 144.0);
    }
    assert_eq!(effects.items.len(), 1);
    effects.move_trail(0, Vec2::ZERO, Vec2::splat(100.0), -Vec2::X * 2.0);
    for scale in [0.25, 1.0, 4.0] {
        let mesh = geometry(&effects, scale).unwrap();
        let (positions, colors) = vertices(&mesh);
        assert!(
            positions
                .iter()
                .all(|p| (p[0] - 50.0).abs() <= 2.01 * scale)
        );
        assert!(positions.iter().all(|p| p[1].abs() <= 18.01 * scale));
        assert!(colors.iter().all(|color| color[3] <= 0.16));
    }
    effects.move_trail(1, Vec2::ZERO, Vec2::splat(100.0), Vec2::Y * 2.0);
    assert_eq!(effects.items.len(), 2);
    effects.advance(0.17);
    assert!(!effects.active());
}

#[test]
fn edit_zoom_gating_and_capacity_bound() {
    let mut effects = Effects::default();
    effects.begin_edit(Vec2::ZERO, Vec2::ONE, false);
    effects.commit_edit(Vec2::ZERO, Vec2::ONE, false);
    assert!(!effects.active());
    for node in 0..MAX_EFFECTS * 2 {
        effects.move_trail(node, Vec2::ZERO, Vec2::splat(100.0), Vec2::X);
    }
    assert_eq!(effects.items.len(), MAX_EFFECTS);
    effects.advance(10.0);
    assert!(!effects.active());
}

#[test]
fn all_effect_shapes_tessellate_and_expire() {
    let mut effects = Effects::default();
    effects.begin_edit(Vec2::ZERO, Vec2::splat(100.0), true);
    effects.move_trail(0, Vec2::ZERO, Vec2::splat(100.0), Vec2::X * 10.0);
    effects.cut_line(Vec2::new(-60.0, 0.0), Vec2::new(60.0, 0.0));
    effects.advance(0.1);
    effects.cut_contact(Vec2::ZERO);
    let mesh = geometry(&effects, 1.0).unwrap();
    assert!(mesh.count_vertices() > 0);
    effects.advance(50.0 / FPS);
    assert!(!effects.active());
}

#[test]
fn contact_flame_lives_five_frames_and_contacts_include_interior_endpoints() {
    let hits = segment_rect_contacts(
        Vec2::ZERO,
        Vec2::splat(100.0),
        Vec2::ZERO,
        Vec2::new(60.0, 0.0),
    );
    assert_eq!(hits.len(), 2);
    assert!(hits.contains(&Vec2::ZERO));
    assert!(hits.contains(&Vec2::new(50.0, 0.0)));
    let mut effects = Effects::default();
    effects.cut_contact(Vec2::ZERO);
    effects.advance(4.0 / FPS);
    assert!(effects.active());
    assert!(geometry(&effects, 1.0).unwrap().count_vertices() >= 33);
    effects.advance(1.0 / FPS);
    assert!(!effects.active());
}
