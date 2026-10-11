use super::*;
use bevy::{asset::AssetPlugin, text::TextPlugin};

#[test]
fn native_math_uses_msdf_and_reuses_geometry_while_zooming() {
    check_math_cache(r"\frac{d\mathbf{p}}{dt}=m\mathbf{v}+\sqrt{x^2+1}", None);
}

#[test]
fn invalid_native_math_keeps_its_source_and_reports_failure() {
    let mut app = app();
    let source = r"\frac{1}{";
    app.world_mut()
        .spawn((MsdfText::new(source), super::super::math_text::MathText));
    for _ in 0..4 {
        app.update();
    }
    assert_eq!(
        app.world().resource::<MsdfCache>().status().failed_labels,
        1
    );
    let world = app.world_mut();
    assert_eq!(world.query::<&Text2d>().single(world).unwrap().0, source);
}

#[test]
#[ignore = "requires installed Noto Sans CJK font"]
fn native_math_renders_the_reported_chinese_force_equation() {
    let bytes =
        std::fs::read("/usr/share/fonts/google-noto-sans-cjk-fonts/NotoSansCJK-Regular.ttc")
            .unwrap();
    check_math_cache(
        r"\begin{equation}\mathbf{F} = \frac{d\mathbf{p}}{dt}, \quad \text{其中 } \mathbf{p} = m\mathbf{v}\end{equation}",
        Some(bytes),
    );
}

fn check_math_cache(source: &str, bytes: Option<Vec<u8>>) {
    let mut app = app();
    let font = bytes.map(|bytes| {
        app.world_mut()
            .resource_mut::<Assets<Font>>()
            .add(Font::from_bytes(bytes))
    });
    let entity = app
        .world_mut()
        .spawn((
            MsdfText::new(source),
            super::super::math_text::MathText,
            TextFont {
                font: font.map(Into::into).unwrap_or_default(),
                font_size: FontSize::Px(32.0),
                ..default()
            },
        ))
        .id();
    settle(&mut app);
    let before = app.world().resource::<MsdfCache>().status();
    assert!(before.generated_glyphs > 8);
    let state = app.world().get::<TextState>(entity).unwrap();
    assert!(state.math.is_some());
    assert!(state.parts.len() >= 2, "glyph atlas and vector rules");
    assert_eq!(
        app.world_mut().query::<&Text2d>().iter(app.world()).count(),
        0
    );
    for i in 1..=120 {
        app.world_mut().get_mut::<Transform>(entity).unwrap().scale = Vec3::splat(i as f32 / 10.0);
        app.update();
    }
    let after = app.world().resource::<MsdfCache>().status();
    assert_eq!(before.generated_glyphs, after.generated_glyphs);
    assert_eq!(before.mesh_rebuilds, after.mesh_rebuilds);
    assert_eq!(before.atlas_pages, after.atlas_pages);
    app.world_mut().get_mut::<TextColor>(entity).unwrap().0 = Color::BLACK;
    settle(&mut app);
    assert_eq!(
        before.generated_glyphs,
        app.world()
            .resource::<MsdfCache>()
            .status()
            .generated_glyphs
    );
    assert_atlas_coverage(&app);
}

#[test]
fn glyph_workers_continue_a_batch_without_waiting_for_more_frames() {
    let mut app = app();
    app.world_mut()
        .spawn(MsdfText::new("ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789"));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while app.world().resource::<MsdfCache>().pending() == 0 {
        app.update();
        assert!(std::time::Instant::now() < deadline);
    }
    let pending = app.world().resource::<MsdfCache>().pending();
    assert_eq!(pending, 36);
    app.update();
    // Simulate the main thread being busy with a frame. Worker throughput must
    // not be limited to one glyph per worker until the next frame polls it.
    while !app
        .world()
        .resource::<MsdfCache>()
        .tasks
        .iter()
        .all(|(_, task)| task.is_finished())
    {
        assert!(std::time::Instant::now() < deadline);
        std::thread::yield_now();
    }
    assert_eq!(app.world().resource::<MsdfCache>().pending(), pending);
    app.update();
    let cache = app.world().resource::<MsdfCache>();
    assert!(
        cache.generated >= 32,
        "only {} glyphs completed between frame polls",
        cache.generated
    );
    assert_eq!(cache.pending() + cache.generated, pending);
    settle(&mut app);
    assert_eq!(app.world().resource::<MsdfCache>().generated, pending);
    assert_atlas_coverage(&app);
}

#[test]
fn reloaded_identical_fonts_share_glyph_keys_without_merging_distinct_fonts_or_faces() {
    let mut cache = MsdfCache::default();
    let first = cache.register_font(100, b"font-a");
    for source in 101..150 {
        assert_eq!(cache.register_font(source, b"font-a"), first);
    }
    let second = cache.register_font(200, b"font-b");
    assert_ne!(first, second);
    assert_eq!(cache.fonts.len(), 2);
    let key = GlyphKey {
        font: first,
        face: 0,
        glyph: 42,
    };
    cache.glyphs.insert(key, GlyphStatus::Ready(None));
    let alias = cache.register_font(151, b"font-a");
    assert!(cache.glyphs.contains_key(&GlyphKey { font: alias, ..key }));
    assert!(!cache.glyphs.contains_key(&GlyphKey { face: 1, ..key }));
    assert!(!cache.glyphs.contains_key(&GlyphKey {
        font: second,
        ..key
    }));
}

#[test]
#[ignore = "requires GRAPLU_FONT_CACHE_DOC and installed Noto Sans CJK font"]
fn tutorial_font_reload_does_not_regenerate_or_grow_the_glyph_atlas() {
    let path = std::env::var("GRAPLU_FONT_CACHE_DOC").expect("set a tutorial PRG path");
    let doc = super::super::Document::open(std::path::Path::new(&path)).unwrap();
    let bytes =
        std::fs::read("/usr/share/fonts/google-noto-sans-cjk-fonts/NotoSansCJK-Regular.ttc")
            .unwrap();
    let text = doc
        .nodes
        .iter()
        .map(|node| node.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let mut app = app();
    let mut baseline = None;
    for _ in 0..8 {
        let font = app
            .world_mut()
            .resource_mut::<Assets<Font>>()
            .add(Font::from_bytes(bytes.clone()));
        let entity = app
            .world_mut()
            .spawn((
                MsdfText::new(&text),
                TextFont {
                    font: font.clone().into(),
                    font_size: FontSize::Px(24.0),
                    ..default()
                },
            ))
            .id();
        settle(&mut app);
        let status = app.world().resource::<MsdfCache>().status();
        let size = (status.generated_glyphs, status.atlas_pages);
        if let Some(before) = baseline {
            assert_eq!(size, before, "identical font reload regenerated glyphs");
        } else {
            assert!(status.generated_glyphs > 100);
            baseline = Some(size);
        }
        app.world_mut().despawn(entity);
        app.world_mut()
            .resource_mut::<Assets<Font>>()
            .remove(font.id());
    }
}

pub(crate) fn app() -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default(), TextPlugin))
        .init_resource::<Assets<Image>>()
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<MsdfMaterial>>()
        .init_resource::<MsdfCache>()
        .init_resource::<super::super::editor::Editor>()
        .add_systems(
            PostUpdate,
            update.after(bevy::text::load_font_assets_into_font_collection),
        );
    app
}

pub(crate) fn settle(app: &mut App) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        app.update();
        let status = app.world().resource::<MsdfCache>().status();
        assert_eq!(status.failed_labels, 0, "{status:?}");
        if status.ready() {
            break;
        }
        assert!(std::time::Instant::now() < deadline, "{status:?}");
        std::thread::yield_now();
    }
}

fn assert_atlas_coverage(app: &App) {
    let cache = app.world().resource::<MsdfCache>();
    let images = app.world().resource::<Assets<Image>>();
    for status in cache.glyphs.values() {
        let GlyphStatus::Ready(Some(glyph)) = status else {
            continue;
        };
        let bytes = images
            .get(&cache.pages[glyph.page].image)
            .unwrap()
            .data
            .as_ref()
            .unwrap();
        let median = |x: u32, y: u32| {
            let index = ((y * ATLAS_SIZE + x) * 4) as usize;
            let mut rgb = [bytes[index], bytes[index + 1], bytes[index + 2]];
            rgb.sort_unstable();
            rgb[1]
        };
        for x in [glyph.rect.min.x, glyph.rect.max.x - 1] {
            for y in [glyph.rect.min.y, glyph.rect.max.y - 1] {
                assert!(
                    median(x, y) < 128,
                    "glyph exterior must have negative distance"
                );
            }
        }
        assert!(
            (glyph.rect.min.y..glyph.rect.max.y)
                .any(|y| (glyph.rect.min.x..glyph.rect.max.x).any(|x| median(x, y) > 128)),
            "outlined glyph must contain ink"
        );
    }
}

#[test]
#[ignore = "requires pinned tutorial and installed Noto Sans CJK font"]
fn tutorial_auto_size_preserves_hard_lines_and_contains_measured_text() {
    use super::super::{
        Document, Interaction, NodeAssets, NodeLabel, Scene, editor, node_text_layout, padding,
        physics, text_width, theme,
    };
    let doc = Document::open(std::path::Path::new(
        "fixtures/tutorial-shortcut-keys-3.1-original.prg",
    ))
    .unwrap();
    let mut app = app();
    let font = app
        .world_mut()
        .resource_mut::<Assets<Font>>()
        .add(Font::from_bytes(
            std::fs::read("/usr/share/fonts/google-noto-sans-cjk-fonts/NotoSansCJK-Regular.ttc")
                .unwrap(),
        ));
    for (index, node) in doc
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| n.kind == "TextNode")
    {
        app.world_mut().spawn((
            NodeLabel(index),
            MsdfText::new(&node.text),
            TextFont {
                font: font.clone().into(),
                font_size: FontSize::Px(node.font_size),
                ..default()
            },
            node_text_layout(node),
            TextBounds {
                width: text_width(node),
                height: None,
            },
            LineHeight::RelativeToFont(1.5),
            Anchor::TOP_CENTER,
        ));
    }
    let corners = theme::NodeCorners::new(&mut app.world_mut().resource_mut::<Assets<Image>>());
    app.insert_resource(NodeAssets {
        font,
        corners,
        textures: default(),
    });
    app.insert_resource(Scene(doc))
        .init_resource::<Interaction>()
        .init_resource::<physics::PhysicsEditor>()
        .add_systems(
            PostUpdate,
            (
                editor::sync_labels.before(update),
                editor::fit_text_nodes.after(update),
            ),
        );
    settle(&mut app);
    let mut checked = 0;
    let world = app.world_mut();
    let mut labels = world.query::<(&NodeLabel, &TextLayoutInfo, &ComputedTextBlock)>();
    for (label, info, block) in labels.iter(world) {
        let node = &world.resource::<Scene>().0.nodes[label.0];
        let inset = 2.0 * padding(node);
        assert!(
            info.size.y + inset <= node.rect[3] + 0.02,
            "{} overflows vertically",
            node.text
        );
        if node.auto_size {
            assert_eq!(
                block.buffer().lines().count(),
                node.text.split('\n').count(),
                "unexpected wrap: {}",
                node.text
            );
            assert!(
                info.size.x + inset <= node.rect[2] + 0.02,
                "{} overflows horizontally",
                node.text
            );
            checked += 1;
        }
    }
    assert!(checked > 250);
    assert!(!world.resource::<Scene>().0.dirty());
    let before = world.resource::<MsdfCache>().status();
    for _ in 0..3 {
        app.update();
    }
    assert_eq!(
        app.world().resource::<MsdfCache>().status().mesh_rebuilds,
        before.mesh_rebuilds
    );
}

#[test]
fn zoom_reuses_glyphs_and_meshes_while_edits_relayout() {
    let mut app = app();
    let label = app
        .world_mut()
        .spawn((
            MsdfText::new("AB AB"),
            TextFont::from_font_size(32.0),
            TextBounds::new_horizontal(200.0),
        ))
        .id();
    settle(&mut app);
    let initial = app.world().resource::<MsdfCache>().status();
    assert_atlas_coverage(&app);
    assert_eq!(initial.generated_glyphs, 3);
    assert_eq!(initial.atlas_pages, 1);
    let size = app.world().get::<TextLayoutInfo>(label).unwrap().size;
    let parts = app.world().get::<TextState>(label).unwrap().parts.clone();
    assert_eq!(parts.len(), 1);
    app.world_mut().get_mut::<Transform>(label).unwrap().scale = Vec3::splat(100.0);
    app.update();
    assert_eq!(
        app.world().resource::<MsdfCache>().status().mesh_rebuilds,
        initial.mesh_rebuilds
    );
    assert_eq!(app.world().get::<TextState>(label).unwrap().parts, parts);
    assert_eq!(app.world().get::<TextLayoutInfo>(label).unwrap().size, size);
    app.world_mut().get_mut::<TextBounds>(label).unwrap().width = Some(40.0);
    settle(&mut app);
    assert!(app.world().get::<TextLayoutInfo>(label).unwrap().size.y > size.y);
    assert_eq!(
        app.world().resource::<MsdfCache>().generated,
        initial.generated_glyphs
    );
    app.world_mut().get_mut::<MsdfText>(label).unwrap().0 = "C".into();
    settle(&mut app);
    assert_eq!(
        app.world().resource::<MsdfCache>().generated,
        initial.generated_glyphs + 1
    );
    app.world_mut()
        .get_mut::<MsdfText>(label)
        .unwrap()
        .0
        .clear();
    settle(&mut app);
    assert!(
        app.world()
            .get::<TextState>(label)
            .unwrap()
            .parts
            .is_empty()
    );
}

#[test]
#[ignore = "requires installed Noto Sans CJK font"]
fn chinese_outlines_generate_and_share_atlas() {
    let mut app = app();
    let font = std::fs::read("/usr/share/fonts/google-noto-sans-cjk-fonts/NotoSansCJK-Regular.ttc")
        .unwrap();
    let handle = app
        .world_mut()
        .resource_mut::<Assets<Font>>()
        .add(Font::from_bytes(font));
    for _ in 0..2 {
        app.world_mut().spawn((
            MsdfText::new("中文画布缩放，鬱龘 ABC"),
            TextFont {
                font: handle.clone().into(),
                font_size: FontSize::Px(32.0),
                ..default()
            },
        ));
    }
    settle(&mut app);
    assert_atlas_coverage(&app);
    let cache = app.world().resource::<MsdfCache>();
    assert_eq!(cache.status().mesh_rebuilds, 2);
    assert_eq!(cache.generated, 13);
    assert_eq!(cache.pages.len(), 1);
}

#[test]
fn failed_glyph_replaces_old_mesh_with_current_text() {
    let mut app = app();
    let label = app.world_mut().spawn(MsdfText::new("AB")).id();
    settle(&mut app);
    let old = app.world().get::<TextState>(label).unwrap().parts[0];
    let key = app.world().get::<TextState>(label).unwrap().glyphs[0].key;
    app.world_mut()
        .resource_mut::<MsdfCache>()
        .glyphs
        .insert(key, GlyphStatus::Failed("test failure".into()));
    app.world_mut().get_mut::<MsdfText>(label).unwrap().0 = "A".into();
    app.update();
    assert_eq!(
        app.world().resource::<MsdfCache>().status().failed_labels,
        1
    );
    assert!(app.world().get_entity(old).is_err());
    let part = app.world().get::<TextState>(label).unwrap().parts[0];
    assert_eq!(app.world().get::<Text2d>(part).unwrap().0, "A");
    app.world_mut().get_mut::<MsdfText>(label).unwrap().0 = "B".into();
    settle(&mut app);
    assert!(app.world().get_entity(part).is_err());
    let part = app.world().get::<TextState>(label).unwrap().parts[0];
    assert!(app.world().get::<Mesh2d>(part).is_some());
}

#[test]
fn color_glyph_fallback_is_ready_without_error_and_recovers_to_msdf() {
    let mut app = app();
    let label = app.world_mut().spawn(MsdfText::new("AB")).id();
    settle(&mut app);
    let old = app.world().get::<TextState>(label).unwrap().parts[0];
    let key = app.world().get::<TextState>(label).unwrap().glyphs[0].key;
    app.world_mut()
        .resource_mut::<MsdfCache>()
        .glyphs
        .insert(key, GlyphStatus::Color);
    app.world_mut().get_mut::<MsdfText>(label).unwrap().0 = "A".into();
    settle(&mut app);
    let status = app.world().resource::<MsdfCache>().status();
    assert_eq!(status.color_labels, 1);
    assert_eq!(status.failed_labels, 0);
    assert!(
        !app.world()
            .resource::<super::super::editor::Editor>()
            .tabs_blocked()
    );
    assert!(app.world().get_entity(old).is_err());
    let part = app.world().get::<TextState>(label).unwrap().parts[0];
    assert_eq!(app.world().get::<Text2d>(part).unwrap().0, "A");
    app.world_mut().get_mut::<MsdfText>(label).unwrap().0 = "B".into();
    settle(&mut app);
    assert!(app.world().get_entity(part).is_err());
    assert_eq!(app.world().resource::<MsdfCache>().status().color_labels, 0);
    let part = app.world().get::<TextState>(label).unwrap().parts[0];
    assert!(app.world().get::<Mesh2d>(part).is_some());
}

#[test]
#[ignore = "requires installed Noto Sans CJK and Noto Color Emoji fonts"]
fn tutorial_runner_emoji_uses_color_fallback_without_blocking_readiness() {
    let mut app = app();
    let font = app
        .world_mut()
        .resource_mut::<Assets<Font>>()
        .add(Font::from_bytes(
            std::fs::read("/usr/share/fonts/google-noto-sans-cjk-fonts/NotoSansCJK-Regular.ttc")
                .unwrap(),
        ));
    let label = app
        .world_mut()
        .spawn((
            MsdfText::new("运动 🏃"),
            TextFont {
                font: font.into(),
                ..default()
            },
        ))
        .id();
    settle(&mut app);
    assert_eq!(app.world().resource::<MsdfCache>().status().color_labels, 1);
    assert!(
        !app.world()
            .resource::<super::super::editor::Editor>()
            .tabs_blocked()
    );
    let state = app.world().get::<TextState>(label).unwrap();
    assert!(state.color_fallback);
    assert_eq!(
        app.world().get::<Text2d>(state.parts[0]).unwrap().0,
        "运动 🏃"
    );
}

#[test]
#[ignore = "requires installed Noto Sans CJK and system symbol fallback fonts"]
fn system_fallback_shapes_option_symbol_and_reuses_msdf_cache() {
    let mut app = app();
    let font = std::fs::read("/usr/share/fonts/google-noto-sans-cjk-fonts/NotoSansCJK-Regular.ttc")
        .unwrap();
    let handle = app
        .world_mut()
        .resource_mut::<Assets<Font>>()
        .add(Font::from_bytes(font));
    let label = app
        .world_mut()
        .spawn((
            MsdfText::new("中文⌥⌘"),
            TextFont {
                font: handle.into(),
                font_size: FontSize::Px(32.0),
                ..default()
            },
        ))
        .id();
    settle(&mut app);
    let state = app.world().get::<TextState>(label).unwrap();
    assert_eq!(state.glyphs.len(), 4);
    assert!(
        state.glyphs.iter().all(|glyph| glyph.key.glyph != 0),
        "fallback must resolve real glyphs, not .notdef boxes"
    );
    assert_ne!(
        state.glyphs[0].key.font, state.glyphs[2].key.font,
        "Option is absent from Noto Sans CJK and must use a fallback face"
    );
    assert_atlas_coverage(&app);
    let before = app.world().resource::<MsdfCache>().status();
    app.world_mut().get_mut::<Transform>(label).unwrap().scale = Vec3::splat(16.0);
    app.update();
    let after = app.world().resource::<MsdfCache>().status();
    assert_eq!(before.generated_glyphs, after.generated_glyphs);
    assert_eq!(before.mesh_rebuilds, after.mesh_rebuilds);
}

#[test]
fn unavailable_character_is_reported_and_recovers_after_edit() {
    let mut app = app();
    app.world_mut().resource_mut::<FontCx>().context.collection =
        parley::fontique::Collection::new(parley::fontique::CollectionOptions {
            system_fonts: false,
            ..default()
        });
    let label = app.world_mut().spawn(MsdfText::new("⌥")).id();
    app.update();
    let state = app.world().get::<TextState>(label).unwrap();
    assert!(state.failed);
    assert_eq!(
        app.world().resource::<MsdfCache>().status().failed_labels,
        1
    );
    assert_eq!(app.world().get::<Text2d>(state.parts[0]).unwrap().0, "⌥");
    assert!(
        app.world()
            .resource::<MsdfCache>()
            .glyphs
            .keys()
            .all(|key| key.glyph != 0)
    );
    app.world_mut().get_mut::<MsdfText>(label).unwrap().0 = "A".into();
    settle(&mut app);
    assert_eq!(
        app.world().resource::<MsdfCache>().status().failed_labels,
        0
    );
}

#[test]
#[ignore = "requires GPU, Noto Sans CJK and symbol fonts; writes results/msdf-smoke.png"]
fn gpu_renders_chinese_at_multiple_scales() {
    use bevy::{
        camera::RenderTarget,
        render::view::screenshot::{Screenshot, ScreenshotCaptured},
        window::ExitCondition,
        winit::WinitPlugin,
    };
    #[derive(Resource, Default)]
    struct Capture(Option<Image>);
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
        update
            .after(bevy::text::load_font_assets_into_font_collection)
            .before(bevy::transform::TransformSystems::Propagate),
    );
    install(&mut app);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(90);
    while app.plugins_state() != bevy::app::PluginsState::Ready {
        assert!(std::time::Instant::now() < deadline);
        bevy::tasks::tick_global_task_pools_on_main_thread();
        std::thread::yield_now();
    }
    app.finish();
    app.cleanup();
    let target = app
        .world_mut()
        .resource_mut::<Assets<Image>>()
        .add(Image::new_target_texture(
            1280,
            600,
            TextureFormat::Rgba8UnormSrgb,
            None,
        ));
    app.world_mut()
        .spawn((Camera2d, RenderTarget::Image(target.clone().into())));
    let font = std::fs::read("/usr/share/fonts/google-noto-sans-cjk-fonts/NotoSansCJK-Regular.ttc")
        .unwrap();
    let handle = app
        .world_mut()
        .resource_mut::<Assets<Font>>()
        .add(Font::from_bytes(font));
    for (scale, y) in [(1.0, 260.0), (4.0, 210.0), (16.0, 80.0)] {
        app.world_mut().spawn((
            MsdfText::new("中路径⌥"),
            TextFont {
                font: handle.clone().into(),
                font_size: FontSize::Px(16.0),
                ..default()
            },
            Anchor::TOP_LEFT,
            Transform::from_xyz(-608.0, y, 0.0).with_scale(Vec3::splat(scale)),
        ));
    }
    app.world_mut().spawn((
        MsdfText::new("🏃"),
        TextFont {
            font: handle.into(),
            font_size: FontSize::Px(96.0),
            ..default()
        },
        Anchor::TOP_LEFT,
        Transform::from_xyz(480.0, -160.0, 0.0),
    ));
    settle(&mut app);
    let before = app.world().resource::<MsdfCache>().status();
    assert_eq!(before.generated_glyphs, 4);
    assert_eq!(before.color_labels, 1);
    let cache = app.world().resource::<MsdfCache>();
    assert!(
        cache.fonts.len() >= 2,
        "GPU capture must cover system font fallback"
    );
    assert!(cache.glyphs.keys().all(|key| key.glyph != 0));
    for _ in 0..30 {
        app.update();
    }
    app.world_mut().spawn(Screenshot::image(target)).observe(
        |capture: On<ScreenshotCaptured>, mut saved: ResMut<Capture>| {
            saved.0 = Some(capture.image.clone());
        },
    );
    while app.world().resource::<Capture>().0.is_none() {
        app.update();
        assert!(
            std::time::Instant::now() < deadline,
            "GPU capture timed out"
        );
        std::thread::yield_now();
    }
    let captured = app.world_mut().resource_mut::<Capture>().0.take().unwrap();
    let image = captured.try_into_dynamic().unwrap().to_rgb8();
    std::fs::create_dir_all("results").unwrap();
    image.save("results/msdf-smoke.png").unwrap();
    let colored_ink = image
        .enumerate_pixels()
        .filter(|(x, y, p)| {
            let [r, g, b] = p.0;
            *x >= 1100 && *y >= 450 && r.max(g).max(b).saturating_sub(r.min(g).min(b)) > 40
        })
        .count();
    assert!(
        colored_ink > 100,
        "emoji must retain actual color, not grayscale or an empty fallback: {colored_ink}"
    );
    for (top, bottom, minimum) in [(40, 80, 100), (90, 210, 1000), (220, 590, 10000)] {
        let ink = image
            .enumerate_pixels()
            .filter(|(_, y, p)| *y >= top && *y < bottom && p.0[0] > 128)
            .count();
        assert!(ink > minimum, "MSDF row {top} is missing: {ink} pixels");
        assert!(
            ink < (1024 * (bottom - top) / 3) as usize,
            "MSDF row {top} has inverted coverage: {ink} pixels"
        );
    }
    assert_eq!(
        image.get_pixel(40, 245).0,
        [0, 0, 0],
        "glyph padding must be transparent"
    );
    assert_eq!(
        image.get_pixel(100, 320).0,
        [0, 0, 0],
        "中 inner counter must be transparent"
    );
    assert_eq!(
        image.get_pixel(210, 320).0,
        [0, 0, 0],
        "中 right counter must be transparent"
    );
    assert_eq!(
        image.get_pixel(160, 400).0,
        [255, 255, 255],
        "中 center stroke must be opaque"
    );
    assert_eq!(
        app.world()
            .resource::<MsdfCache>()
            .status()
            .generated_glyphs,
        before.generated_glyphs
    );
}
