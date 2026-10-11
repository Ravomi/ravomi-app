use super::*;
use bevy::{
    ecs::system::NonSendMarker,
    input::mouse::MouseWheel,
    winit::{RawWinitWindowEvent, WINIT_WINDOWS},
};
use winit::event::{TouchPhase, WindowEvent};

// Apply sensitivity after converting physical input to logical screen distance.
const TRACKPAD_PAN_SPEED: f32 = 2.0;
const TRACKPAD_ZOOM_SPEED: f32 = 1.5;

#[derive(Default)]
pub struct GestureAnchor(Option<Vec2>);

#[derive(Resource, Default)]
pub struct WheelZoom {
    target: Option<f32>,
    cursor: Vec2,
    expected: Option<(f32, Vec3, Vec2)>,
}

impl WheelZoom {
    pub fn active(&self) -> bool {
        self.target.is_some()
    }

    fn cancel(&mut self) {
        self.target = None;
        self.expected = None;
    }
}

// Winit pixel-wheel deltas are physical pixels; Window cursor positions are logical.
// Keep device interpretation here, and leave projection caching to CameraUpdateSystems.
#[allow(clippy::too_many_arguments)]
pub fn navigate(
    window: Single<(Entity, &Window), With<PrimaryWindow>>,
    mut wheels: MessageReader<MouseWheel>,
    mut gestures: MessageReader<RawWinitWindowEvent>,
    keys: Res<ButtonInput<KeyCode>>,
    editor: Res<editor::Editor>,
    config: Res<Config>,
    time: Res<Time<Real>>,
    mouse: Option<Res<ButtonInput<MouseButton>>>,
    mut wheel: ResMut<WheelZoom>,
    mut camera: Single<(&mut Transform, &mut Projection), With<Camera2d>>,
    mut anchor: Local<GestureAnchor>,
    _main_thread: NonSendMarker,
) {
    let (entity, window) = *window;
    let mut wheel_events = wheels.read();
    let mut gesture_events = gestures.read();
    let Some(cursor) = window
        .cursor_position()
        .filter(|_| window.focused && !editor.navigation_capture && !config.benchmark)
    else {
        wheel_events.for_each(drop);
        gesture_events.for_each(drop);
        anchor.0 = None;
        wheel.cancel();
        return;
    };
    let (transform, projection) = &mut *camera;
    let Projection::Orthographic(projection) = &mut **projection else {
        wheel_events.for_each(drop);
        gesture_events.for_each(drop);
        anchor.0 = None;
        wheel.cancel();
        return;
    };
    let size = Vec2::new(window.width(), window.height());
    if wheel
        .expected
        .is_some_and(|expected| expected != (projection.scale, transform.translation, size))
    {
        // Fit-to-view and other camera owners supersede an unfinished zoom.
        wheel.cancel();
    }
    let pointer_busy = mouse.is_some_and(|mouse| mouse.get_pressed().next().is_some());
    if pointer_busy || keys.just_pressed(KeyCode::KeyF) {
        wheel.cancel();
    }
    let was_active = wheel.active();
    let command = keys.any_pressed([
        KeyCode::ControlLeft,
        KeyCode::ControlRight,
        KeyCode::SuperLeft,
        KeyCode::SuperRight,
    ]);
    for event in &mut wheel_events {
        if event.window != entity || !event.x.is_finite() || !event.y.is_finite() {
            continue;
        }
        match event.unit {
            MouseScrollUnit::Line if !pointer_busy && event.y != 0.0 => {
                let current = projection.scale.ln();
                let target = wheel.target.unwrap_or(current);
                let amount = event.y * 0.12;
                // Reversing the wheel should respond immediately, without paying off
                // a queue of unrendered motion in the old direction.
                let base = if (target - current) * amount > 0.0 {
                    current
                } else {
                    target
                };
                wheel.target = Some((base - amount).clamp(0.01_f32.ln(), 1000.0_f32.ln()));
                wheel.cursor = cursor;
            }
            MouseScrollUnit::Line => {}
            MouseScrollUnit::Pixel => {
                wheel.cancel();
                let delta = Vec2::new(event.x, event.y) / window.resolution.scale_factor();
                if command {
                    zoom_at(
                        cursor,
                        window,
                        transform,
                        projection,
                        delta.y * 0.002 * TRACKPAD_ZOOM_SPEED,
                    );
                } else {
                    pan_by(transform, projection.scale, delta * TRACKPAD_PAN_SPEED);
                }
            }
        }
    }
    // The high-level messages split pan from pinch and discard phase/window identity.
    // Consume Winit's ordered stream so every update uses the moving gesture center.
    for event in &mut gesture_events {
        if WINIT_WINDOWS.with_borrow(|windows| windows.get_window_entity(event.window_id))
            != Some(entity)
        {
            continue;
        }
        match event.event {
            WindowEvent::PinchGesture {
                phase: TouchPhase::Started,
                delta,
                ..
            } => {
                wheel.cancel();
                anchor.0 = Some(cursor);
                if delta.is_finite() {
                    zoom_at(
                        cursor,
                        window,
                        transform,
                        projection,
                        delta as f32 * TRACKPAD_ZOOM_SPEED,
                    );
                }
            }
            WindowEvent::PinchGesture {
                delta,
                phase: TouchPhase::Moved,
                ..
            } => {
                if let Some(center) = anchor.0
                    && delta.is_finite()
                {
                    zoom_at(
                        center,
                        window,
                        transform,
                        projection,
                        delta as f32 * TRACKPAD_ZOOM_SPEED,
                    );
                }
            }
            WindowEvent::PanGesture {
                delta,
                phase: TouchPhase::Moved,
                ..
            } => {
                if let Some(center) = anchor.0 {
                    let delta = Vec2::new(delta.x, delta.y) / window.resolution.scale_factor()
                        * TRACKPAD_PAN_SPEED;
                    if delta.is_finite() && (center + delta).is_finite() {
                        pan_by(transform, projection.scale, delta);
                        anchor.0 = Some(center + delta);
                    }
                }
            }
            WindowEvent::PinchGesture {
                phase: TouchPhase::Ended | TouchPhase::Cancelled,
                ..
            } => anchor.0 = None,
            _ => {}
        }
    }
    if let Some(target) = wheel.target {
        let current = projection.scale.ln();
        // A reactive window can wake with a large delta. Do not spend its idle
        // time on an animation that only started with this input event.
        let dt = time
            .delta_secs()
            .min(if was_active { 0.05 } else { 1.0 / 60.0 });
        let remaining = target - current;
        let step = if remaining.abs() < 0.00001 {
            wheel.cancel();
            remaining
        } else {
            remaining * -(-dt / 0.045).exp_m1()
        };
        zoom_at(wheel.cursor, window, transform, projection, -step);
        if wheel.active() {
            wheel.expected = Some((projection.scale, transform.translation, size));
        }
    }
}

fn pan_by(transform: &mut Transform, scale: f32, delta: Vec2) {
    let translation = transform.translation + Vec2::new(-delta.x, delta.y).extend(0.0) * scale;
    if translation.is_finite() && transform.translation != translation {
        transform.translation = translation;
    }
}

fn zoom_at(
    cursor: Vec2,
    window: &Window,
    transform: &mut Transform,
    projection: &mut OrthographicProjection,
    amount: f32,
) {
    if amount == 0.0 {
        return;
    }
    let before = world_cursor(cursor, window, transform, projection.scale);
    projection.scale = (projection.scale * (-amount).exp()).clamp(0.01, 1000.0);
    let after = world_cursor(cursor, window, transform, projection.scale);
    transform.translation += (before - after).extend(0.0);
}
