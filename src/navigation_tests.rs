use super::*;
use bevy::{
    asset::AssetEvent,
    camera::{CameraUpdateSystems, RenderTarget},
    input::{mouse::MouseWheel, touch::TouchPhase},
    render::{camera::camera_system, texture::ManualTextureViews},
    transform::{TransformPlugin, TransformSystems},
    window::{WindowCreated, WindowResized, WindowScaleFactorChanged},
    winit::{RawWinitWindowEvent, WINIT_WINDOWS},
};
use winit::{
    dpi::PhysicalPosition,
    event::{DeviceId, TouchPhase as NativePhase, WindowEvent},
    window::WindowId,
};

struct Harness {
    app: App,
    window: Entity,
    camera: Entity,
    cursor: Vec2,
    native: WindowId,
}
impl Harness {
    fn new(dpi: f32) -> Self {
        let mut app = App::new();
        app.add_plugins(TransformPlugin)
            .init_resource::<Time<Real>>()
            .init_resource::<navigation::WheelZoom>()
            .init_resource::<editor::Editor>()
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<Assets<Image>>()
            .init_resource::<ManualTextureViews>()
            .add_message::<WindowResized>()
            .add_message::<WindowCreated>()
            .add_message::<WindowScaleFactorChanged>()
            .add_message::<AssetEvent<Image>>()
            .add_message::<MouseWheel>()
            .add_message::<RawWinitWindowEvent>()
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
            .add_systems(PostUpdate, navigation::navigate.before(CameraUpdateSystems))
            .add_systems(
                PostUpdate,
                camera_system
                    .in_set(CameraUpdateSystems)
                    .before(TransformSystems::Propagate),
            );
        let cursor = Vec2::new(197.0, 283.0);
        let mut window = Window::default();
        window.resolution.set_scale_factor(dpi);
        window.resolution.set(1280.0, 800.0);
        window.focused = true;
        window.set_cursor_position(Some(cursor));
        let window = app.world_mut().spawn((window, PrimaryWindow)).id();
        let native = WindowId::from(1_u64);
        WINIT_WINDOWS.with_borrow_mut(|windows| {
            windows.winit_to_entity.insert(native, window);
            windows.entity_to_winit.insert(window, native);
        });
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
        Self {
            app,
            window,
            camera,
            cursor,
            native,
        }
    }
    fn wheel(&mut self, target: Entity, unit: MouseScrollUnit, delta: Vec2) {
        self.app.world_mut().write_message(MouseWheel {
            window: target,
            unit,
            x: delta.x,
            y: delta.y,
            phase: TouchPhase::Moved,
        });
    }
    fn pinch(&mut self, delta: f32) {
        self.raw_pinch(self.native, NativePhase::Started, 0.0);
        self.raw_pinch(self.native, NativePhase::Moved, delta as f64);
        self.raw_pinch(self.native, NativePhase::Ended, 0.0);
    }
    fn raw_pinch(&mut self, window_id: WindowId, phase: NativePhase, delta: f64) {
        self.app.world_mut().write_message(RawWinitWindowEvent {
            window_id,
            event: WindowEvent::PinchGesture {
                device_id: DeviceId::dummy(),
                delta,
                phase,
            },
        });
    }
    fn raw_pan(&mut self, window_id: WindowId, phase: NativePhase, delta: Vec2) {
        self.app.world_mut().write_message(RawWinitWindowEvent {
            window_id,
            event: WindowEvent::PanGesture {
                device_id: DeviceId::dummy(),
                delta: PhysicalPosition::new(delta.x, delta.y),
                phase,
            },
        });
    }
    fn anchor(&self) -> Vec2 {
        self.app
            .world()
            .get::<Camera>(self.camera)
            .unwrap()
            .viewport_to_world_2d(
                self.app
                    .world()
                    .get::<GlobalTransform>(self.camera)
                    .unwrap(),
                self.cursor,
            )
            .unwrap()
    }
    fn viewport(&self, point: Vec2) -> Vec2 {
        self.app
            .world()
            .get::<Camera>(self.camera)
            .unwrap()
            .world_to_viewport(
                self.app
                    .world()
                    .get::<GlobalTransform>(self.camera)
                    .unwrap(),
                point.extend(0.0),
            )
            .unwrap()
    }
    fn scale(&self) -> f32 {
        let Projection::Orthographic(projection) =
            self.app.world().get::<Projection>(self.camera).unwrap()
        else {
            panic!("orthographic camera required")
        };
        projection.scale
    }
    fn settle(&mut self) {
        for _ in 0..60 {
            self.app.update();
        }
        assert!(
            !self
                .app
                .world()
                .resource::<navigation::WheelZoom>()
                .active()
        );
    }
    fn assert_anchor(&self, expected: Vec2) {
        assert!(
            self.anchor().distance(expected) < 0.005,
            "rendered cursor anchor drifted: {:?} vs {expected:?}",
            self.anchor()
        );
    }
}

#[test]
fn pixel_pan_uses_logical_distance_at_each_dpi() {
    for dpi in [1.0, 1.5, 1.75, 2.0] {
        let mut h = Harness::new(dpi);
        let before = h.anchor();
        let logical_delta = Vec2::new(12.0, -7.0);
        h.wheel(h.window, MouseScrollUnit::Pixel, logical_delta * dpi);
        h.app.update();
        assert_eq!(h.scale(), 4.0);
        assert!(h.viewport(before).distance(h.cursor + logical_delta) < 0.005);
        assert!((h.anchor() - before).distance(Vec2::new(-48.0, -28.0)) < 0.005);
        let after = h.anchor();
        h.app.update();
        h.assert_anchor(after);
    }
}

#[test]
fn keyboard_focus_does_not_swallow_canvas_pan_or_pinch_start() {
    let mut h = Harness::new(1.75);
    h.app.world_mut().resource_mut::<editor::Editor>().capture = true;
    let before = h.anchor();
    let delta = Vec2::new(12.0, -7.0);
    h.wheel(h.window, MouseScrollUnit::Pixel, delta * 1.75);
    h.app.update();
    assert!(h.viewport(before).distance(h.cursor + delta) < 0.005);
    let anchor = h.anchor();
    h.raw_pinch(h.native, NativePhase::Started, 0.1);
    h.app.update();
    assert!((h.scale() - 4.0 * (-0.1_f32).exp()).abs() < 1e-5);
    h.assert_anchor(anchor);
}

#[test]
fn pinch_and_line_wheel_zoom_preserve_the_rendered_cursor_anchor() {
    for dpi in [1.0, 1.75, 2.0] {
        let mut h = Harness::new(dpi);
        let anchor = h.anchor();
        h.pinch(0.15);
        h.app.update();
        assert!(h.scale() < 4.0);
        h.assert_anchor(anchor);
        h.pinch(-0.15);
        h.app.update();
        assert!((h.scale() - 4.0).abs() < 1e-5);
        h.assert_anchor(anchor);
        h.wheel(h.window, MouseScrollUnit::Line, Vec2::new(2.0, 1.0));
        h.app.update();
        assert!(h.scale() > 4.0 * (-0.12_f32).exp() && h.scale() < 4.0);
        h.assert_anchor(anchor);
        h.settle();
        assert!((h.scale() - 4.0 * (-0.12_f32).exp()).abs() < 1e-5);
        h.assert_anchor(anchor);
        h.wheel(h.window, MouseScrollUnit::Line, Vec2::new(-3.0, -1.0));
        h.app.update();
        h.settle();
        assert!((h.scale() - 4.0).abs() < 1e-5);
        h.assert_anchor(anchor);
    }
}

#[test]
fn control_pixel_scroll_zooms_instead_of_panning_at_each_dpi() {
    for dpi in [1.0, 1.75, 2.0] {
        let mut h = Harness::new(dpi);
        let anchor = h.anchor();
        h.app
            .world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::ControlLeft);
        h.wheel(
            h.window,
            MouseScrollUnit::Pixel,
            Vec2::new(24.0, 60.0) * dpi,
        );
        h.app.update();
        assert!((h.scale() - 4.0 * (-0.12_f32).exp()).abs() < 1e-5);
        h.assert_anchor(anchor);
    }
}

#[test]
fn captured_unfocused_and_cursorless_gestures_are_drained_without_replay() {
    for block in 0..3 {
        let mut h = Harness::new(1.75);
        let anchor = h.anchor();
        match block {
            0 => {
                h.app
                    .world_mut()
                    .resource_mut::<editor::Editor>()
                    .navigation_capture = true
            }
            1 => {
                h.app
                    .world_mut()
                    .get_mut::<Window>(h.window)
                    .unwrap()
                    .focused = false
            }
            _ => h
                .app
                .world_mut()
                .get_mut::<Window>(h.window)
                .unwrap()
                .set_cursor_position(None),
        }
        h.wheel(h.window, MouseScrollUnit::Pixel, Vec2::new(42.0, 90.0));
        h.wheel(h.window, MouseScrollUnit::Line, Vec2::new(0.0, 2.0));
        h.pinch(0.2);
        h.app.update();
        assert_eq!(h.scale(), 4.0);
        h.assert_anchor(anchor);
        h.app
            .world_mut()
            .resource_mut::<editor::Editor>()
            .navigation_capture = false;
        {
            let mut window = h.app.world_mut().get_mut::<Window>(h.window).unwrap();
            window.focused = true;
            window.set_cursor_position(Some(h.cursor));
        }
        h.app.update();
        assert_eq!(h.scale(), 4.0);
        h.assert_anchor(anchor);
        h.pinch(0.1);
        h.app.update();
        assert!(h.scale() < 4.0);
        h.assert_anchor(anchor);
    }
}

#[test]
fn foreign_window_wheels_and_nonfinite_gestures_do_not_change_the_camera() {
    let mut h = Harness::new(1.75);
    let anchor = h.anchor();
    let foreign = h.app.world_mut().spawn(Window::default()).id();
    let foreign_native = WindowId::from(2_u64);
    WINIT_WINDOWS.with_borrow_mut(|windows| {
        windows.winit_to_entity.insert(foreign_native, foreign);
        windows.entity_to_winit.insert(foreign, foreign_native);
    });
    h.wheel(foreign, MouseScrollUnit::Pixel, Vec2::new(80.0, 90.0));
    h.wheel(foreign, MouseScrollUnit::Line, Vec2::new(0.0, 10.0));
    h.wheel(h.window, MouseScrollUnit::Line, Vec2::new(0.0, f32::NAN));
    h.wheel(h.window, MouseScrollUnit::Pixel, Vec2::new(f32::NAN, 50.0));
    h.wheel(
        h.window,
        MouseScrollUnit::Pixel,
        Vec2::new(0.0, f32::INFINITY),
    );
    h.pinch(f32::NAN);
    h.pinch(f32::INFINITY);
    h.raw_pinch(foreign_native, NativePhase::Started, 0.0);
    h.raw_pan(foreign_native, NativePhase::Moved, Vec2::new(200.0, 120.0));
    h.raw_pinch(foreign_native, NativePhase::Moved, 0.4);
    h.raw_pinch(foreign_native, NativePhase::Ended, 0.0);
    h.raw_pinch(h.native, NativePhase::Started, 0.0);
    h.raw_pan(h.native, NativePhase::Moved, Vec2::new(f32::NAN, 20.0));
    h.raw_pan(h.native, NativePhase::Moved, Vec2::new(20.0, f32::INFINITY));
    h.raw_pinch(h.native, NativePhase::Ended, 0.0);
    h.app.update();
    assert_eq!(h.scale(), 4.0);
    h.assert_anchor(anchor);
}

#[test]
fn combined_pan_and_pinch_preserve_a_moving_anchor_without_lifting_fingers() {
    for dpi in [1.0, 1.5, 1.75, 2.0] {
        let mut h = Harness::new(dpi);
        let anchor = h.anchor();
        let mut expected = h.cursor;
        h.raw_pinch(h.native, NativePhase::Started, 0.0);
        h.raw_pan(h.native, NativePhase::Started, Vec2::ZERO);
        h.app.update();

        let first = Vec2::new(10.0, -6.0);
        h.raw_pan(h.native, NativePhase::Moved, first * dpi);
        h.raw_pinch(h.native, NativePhase::Moved, 0.12);
        h.app.update();
        expected += first;
        assert!(h.viewport(anchor).distance(expected) < 0.005);
        assert!((h.scale() - 4.0 * (-0.12_f32).exp()).abs() < 1e-5);

        // Reverse event order on the next frame; both gestures still use the same center.
        let second = Vec2::new(-3.0, 8.0);
        h.raw_pinch(h.native, NativePhase::Moved, -0.04);
        h.raw_pan(h.native, NativePhase::Moved, second * dpi);
        h.app.update();
        expected += second;
        assert!(h.viewport(anchor).distance(expected) < 0.005);
        assert!((h.scale() - 4.0 * (-0.08_f32).exp()).abs() < 1e-5);

        // A translation-only phase does not require ending or restarting the pinch.
        let third = Vec2::new(5.0, 2.0);
        h.raw_pan(h.native, NativePhase::Moved, third * dpi);
        h.app.update();
        expected += third;
        assert!(h.viewport(anchor).distance(expected) < 0.005);
        let scale = h.scale();
        h.raw_pinch(h.native, NativePhase::Moved, 0.06);
        h.app.update();
        assert!(h.scale() < scale);
        assert!(h.viewport(anchor).distance(expected) < 0.005);

        h.raw_pinch(h.native, NativePhase::Ended, 0.0);
        h.raw_pan(h.native, NativePhase::Ended, Vec2::ZERO);
        h.app.update();
        let final_scale = h.scale();
        h.raw_pan(h.native, NativePhase::Moved, Vec2::splat(40.0));
        h.raw_pinch(h.native, NativePhase::Moved, 0.5);
        h.app.update();
        assert_eq!(h.scale(), final_scale);
        assert!(h.viewport(anchor).distance(expected) < 0.005);
    }
}

#[test]
fn capture_cancels_an_active_gesture_and_rejects_its_remaining_moved_events() {
    let mut h = Harness::new(1.75);
    h.raw_pinch(h.native, NativePhase::Started, 0.0);
    h.raw_pan(h.native, NativePhase::Moved, Vec2::new(20.0, 10.0));
    h.raw_pinch(h.native, NativePhase::Moved, 0.1);
    h.app.update();
    let anchor = h.anchor();
    let scale = h.scale();
    h.app
        .world_mut()
        .resource_mut::<editor::Editor>()
        .navigation_capture = true;
    h.raw_pan(h.native, NativePhase::Moved, Vec2::new(30.0, 40.0));
    h.raw_pinch(h.native, NativePhase::Moved, 0.2);
    h.app.update();
    assert_eq!(h.scale(), scale);
    h.assert_anchor(anchor);
    h.app
        .world_mut()
        .resource_mut::<editor::Editor>()
        .navigation_capture = false;
    h.raw_pan(h.native, NativePhase::Moved, Vec2::new(50.0, 60.0));
    h.raw_pinch(h.native, NativePhase::Moved, 0.3);
    h.raw_pinch(h.native, NativePhase::Ended, 0.0);
    h.app.update();
    assert_eq!(h.scale(), scale);
    h.assert_anchor(anchor);
    h.pinch(0.1);
    h.app.update();
    assert!(h.scale() < scale);
    h.assert_anchor(anchor);
}

#[test]
fn cancelled_gesture_and_foreign_end_events_do_not_corrupt_local_gesture_state() {
    let mut h = Harness::new(1.75);
    let anchor = h.anchor();
    let foreign = h.app.world_mut().spawn(Window::default()).id();
    let foreign_native = WindowId::from(2_u64);
    WINIT_WINDOWS.with_borrow_mut(|windows| {
        windows.winit_to_entity.insert(foreign_native, foreign);
    });
    h.raw_pinch(h.native, NativePhase::Started, 0.0);
    h.raw_pinch(foreign_native, NativePhase::Ended, 0.0);
    h.raw_pinch(h.native, NativePhase::Moved, 0.1);
    h.app.update();
    assert!(h.scale() < 4.0);
    h.assert_anchor(anchor);
    let scale = h.scale();
    h.raw_pinch(h.native, NativePhase::Cancelled, 0.0);
    h.raw_pan(h.native, NativePhase::Moved, Vec2::splat(70.0));
    h.raw_pinch(h.native, NativePhase::Moved, 0.4);
    h.app.update();
    assert_eq!(h.scale(), scale);
    h.assert_anchor(anchor);
}

#[test]
fn wheel_accumulates_steps_and_reverses_without_old_direction_tail() {
    let mut h = Harness::new(1.75);
    let anchor = h.anchor();
    for _ in 0..3 {
        h.wheel(h.window, MouseScrollUnit::Line, Vec2::Y);
        h.app.update();
        h.assert_anchor(anchor);
    }
    h.settle();
    assert!((h.scale() - 4.0 * (-0.36_f32).exp()).abs() < 1e-5);
    h.wheel(h.window, MouseScrollUnit::Line, Vec2::Y * 5.0);
    h.app.update();
    let turning = h.scale();
    h.wheel(h.window, MouseScrollUnit::Line, -Vec2::Y);
    h.app.update();
    assert!(h.scale() > turning);
    h.settle();
    assert!((h.scale() - turning * 0.12_f32.exp()).abs() < 1e-5);
    h.assert_anchor(anchor);
}

#[test]
fn pending_wheel_stops_on_capture_focus_loss_pointer_drag_or_external_camera_change() {
    for interruption in 0..4 {
        let mut h = Harness::new(1.0);
        h.wheel(h.window, MouseScrollUnit::Line, Vec2::Y);
        h.app.update();
        assert!(h.app.world().resource::<navigation::WheelZoom>().active());
        match interruption {
            0 => {
                h.app
                    .world_mut()
                    .resource_mut::<editor::Editor>()
                    .navigation_capture = true
            }
            1 => {
                h.app
                    .world_mut()
                    .get_mut::<Window>(h.window)
                    .unwrap()
                    .focused = false
            }
            2 => {
                let mut mouse = ButtonInput::default();
                mouse.press(MouseButton::Left);
                h.app.insert_resource(mouse);
            }
            _ => {
                h.app
                    .world_mut()
                    .get_mut::<Transform>(h.camera)
                    .unwrap()
                    .translation
                    .x += 100.0
            }
        }
        let scale = h.scale();
        h.app.update();
        assert_eq!(h.scale(), scale);
        assert!(!h.app.world().resource::<navigation::WheelZoom>().active());
        h.app
            .world_mut()
            .resource_mut::<editor::Editor>()
            .navigation_capture = false;
        h.app
            .world_mut()
            .get_mut::<Window>(h.window)
            .unwrap()
            .focused = true;
        h.app
            .world_mut()
            .remove_resource::<ButtonInput<MouseButton>>();
        h.settle();
        assert_eq!(h.scale(), scale);
    }
}

#[test]
fn wheel_is_time_based_and_idle_wakeup_does_not_skip_the_animation() {
    let mut samples = Vec::new();
    for fps in [30, 60, 144] {
        let mut h = Harness::new(1.0);
        h.app
            .world_mut()
            .resource_mut::<Time<Real>>()
            .advance_by(std::time::Duration::from_secs(2));
        h.wheel(h.window, MouseScrollUnit::Line, Vec2::Y);
        h.app.update();
        assert!(h.scale() > 4.0 * (-0.12_f32).exp());
        for _ in 0..fps / 6 {
            h.app
                .world_mut()
                .resource_mut::<Time<Real>>()
                .advance_by(std::time::Duration::from_secs_f64(1.0 / fps as f64));
            h.app.update();
        }
        samples.push(h.scale());
    }
    assert!(samples.iter().all(|s| (s - samples[0]).abs() < 1e-5));
}

#[test]
fn direct_trackpad_input_cancels_pending_wheel_and_limits_still_settle() {
    let mut h = Harness::new(2.0);
    h.wheel(h.window, MouseScrollUnit::Line, Vec2::Y);
    h.app.update();
    h.wheel(h.window, MouseScrollUnit::Pixel, Vec2::new(20.0, -10.0));
    h.app.update();
    let scale = h.scale();
    let anchor = h.anchor();
    h.settle();
    assert_eq!(h.scale(), scale);
    h.assert_anchor(anchor);
    for (delta, expected) in [(10000.0, 0.01), (-10000.0, 1000.0)] {
        h.wheel(h.window, MouseScrollUnit::Line, Vec2::Y * delta);
        h.settle();
        assert!((h.scale() - expected).abs() <= expected * 0.00001);
    }
}

#[test]
fn wheel_keeps_redrawing_until_settled_then_restores_idle_policy() {
    let mut h = Harness::new(1.0);
    h.app
        .init_resource::<msdf_text::MsdfCache>()
        .init_resource::<physics::PhysicsEditor>()
        .init_resource::<effects::Effects>()
        .insert_resource(idle_settings())
        .add_systems(PostUpdate, stress::redraw.after(navigation::navigate));
    h.app.update();
    let baseline = h.app.world().resource::<WinitSettings>().focused_mode;
    assert_ne!(baseline, bevy::winit::UpdateMode::Continuous);
    h.wheel(h.window, MouseScrollUnit::Line, Vec2::Y);
    h.app.update();
    assert_eq!(
        h.app.world().resource::<WinitSettings>().focused_mode,
        bevy::winit::UpdateMode::Continuous
    );
    h.settle();
    assert_eq!(
        h.app.world().resource::<WinitSettings>().focused_mode,
        baseline
    );
}
