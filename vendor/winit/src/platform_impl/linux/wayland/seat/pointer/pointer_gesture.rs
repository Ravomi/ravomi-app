//! Pinch support backported from winit 0.31; see UPSTREAM-PATCH.md.

use std::ops::Deref;
use std::sync::Mutex;

use sctk::compositor::SurfaceData;
use sctk::globals::GlobalData;
use sctk::reexports::client::globals::{BindError, GlobalList};
use sctk::reexports::client::{delegate_dispatch, Connection, Dispatch, Proxy, QueueHandle};
use sctk::reexports::protocols::wp::pointer_gestures::zv1::client::zwp_pointer_gesture_pinch_v1::{
    Event as PinchEvent, ZwpPointerGesturePinchV1,
};
use sctk::reexports::protocols::wp::pointer_gestures::zv1::client::zwp_pointer_gestures_v1::ZwpPointerGesturesV1;

use crate::event::{TouchPhase, WindowEvent};
use crate::platform_impl::wayland::event_loop::sink::EventSink;
use crate::platform_impl::wayland::state::WinitState;
use crate::platform_impl::wayland::{self, WindowId};

pub struct PointerGesturesState {
    manager: ZwpPointerGesturesV1,
}

impl PointerGesturesState {
    pub fn new(
        globals: &GlobalList,
        queue_handle: &QueueHandle<WinitState>,
    ) -> Result<Self, BindError> {
        // Pinch exists in v1. Hold/release additions in newer versions are not needed.
        let manager = globals.bind(queue_handle, 1..=1, GlobalData)?;
        Ok(Self { manager })
    }
}

impl Deref for PointerGesturesState {
    type Target = ZwpPointerGesturesV1;

    fn deref(&self) -> &Self::Target {
        &self.manager
    }
}

#[derive(Debug, Default)]
pub struct PointerGestureData {
    inner: Mutex<PinchState>,
}

#[derive(Debug)]
struct PinchState {
    window: Option<WindowId>,
    previous_scale: f64,
}

impl Default for PinchState {
    fn default() -> Self {
        Self {
            window: None,
            previous_scale: 1.0,
        }
    }
}

impl PinchState {
    fn begin(&mut self, window: Option<WindowId>, fingers: u32) -> Option<WindowId> {
        *self = Self::default();
        if fingers == 2 {
            self.window = window;
        }
        self.window
    }

    fn update(&mut self, scale: f64) -> Option<(WindowId, f64)> {
        let window = self.window?;
        if !scale.is_finite() || scale <= 0.0 {
            return None;
        }
        // Match upstream winit: Wayland's absolute scale becomes an additive delta.
        let delta = scale - self.previous_scale;
        self.previous_scale = scale;
        Some((window, delta))
    }

    fn end(&mut self) -> Option<WindowId> {
        let window = self.window;
        *self = Self::default();
        window
    }
}

fn emit(sink: &mut EventSink, window: WindowId, delta: f64, phase: TouchPhase) {
    let device_id =
        crate::event::DeviceId(crate::platform_impl::DeviceId::Wayland(wayland::DeviceId));
    sink.push_window_event(
        WindowEvent::PinchGesture {
            device_id,
            delta,
            phase,
        },
        window,
    );
}

fn emit_pan(sink: &mut EventSink, window: WindowId, dx: f64, dy: f64, scale_factor: f64) {
    // Protocol offsets are surface-local logical pixels; Winit PanGesture uses physical pixels.
    let delta = crate::dpi::LogicalPosition::new(dx, dy).to_physical::<f32>(scale_factor);
    if !delta.x.is_finite() || !delta.y.is_finite() || (delta.x == 0.0 && delta.y == 0.0) {
        return;
    }
    sink.push_window_event(
        WindowEvent::PanGesture {
            device_id: crate::event::DeviceId(crate::platform_impl::DeviceId::Wayland(
                wayland::DeviceId,
            )),
            delta,
            phase: TouchPhase::Moved,
        },
        window,
    );
}

pub fn destroy_gesture(gesture: ZwpPointerGesturePinchV1, sink: &mut EventSink) {
    if let Some(data) = gesture.data::<PointerGestureData>() {
        if let Some(window) = data.inner.lock().unwrap().end() {
            emit(sink, window, 0.0, TouchPhase::Cancelled);
        }
    }
    gesture.destroy();
}

impl Dispatch<ZwpPointerGesturesV1, GlobalData, WinitState> for PointerGesturesState {
    fn event(
        _state: &mut WinitState,
        _proxy: &ZwpPointerGesturesV1,
        _event: <ZwpPointerGesturesV1 as Proxy>::Event,
        _data: &GlobalData,
        _conn: &Connection,
        _queue_handle: &QueueHandle<WinitState>,
    ) {
    }
}

impl Dispatch<ZwpPointerGesturePinchV1, PointerGestureData, WinitState> for PointerGesturesState {
    fn event(
        state: &mut WinitState,
        _proxy: &ZwpPointerGesturePinchV1,
        event: <ZwpPointerGesturePinchV1 as Proxy>::Event,
        data: &PointerGestureData,
        _conn: &Connection,
        _queue_handle: &QueueHandle<WinitState>,
    ) {
        let mut gesture = data.inner.lock().unwrap();
        let (window, delta, phase, pan) = match event {
            PinchEvent::Begin {
                surface, fingers, ..
            } => {
                let window = surface
                    .data::<SurfaceData>()
                    .filter(|data| data.parent_surface().is_none())
                    .map(|_| wayland::make_wid(&surface))
                    .filter(|id| state.windows.get_mut().contains_key(id));
                let Some(window) = gesture.begin(window, fingers) else {
                    return;
                };
                (window, 0.0, TouchPhase::Started, None)
            }
            PinchEvent::Update { scale, dx, dy, .. } => {
                let Some((window, delta)) = gesture.update(scale) else {
                    return;
                };
                let Some(window_state) = state.windows.get_mut().get(&window) else {
                    gesture.end();
                    return;
                };
                let dpi = window_state.lock().unwrap().scale_factor();
                (window, delta, TouchPhase::Moved, Some((dx, dy, dpi)))
            }
            PinchEvent::End { cancelled, .. } => {
                let Some(window) = gesture.end() else { return };
                let phase = if cancelled == 0 {
                    TouchPhase::Ended
                } else {
                    TouchPhase::Cancelled
                };
                (window, 0.0, phase, None)
            }
            _ => return,
        };
        emit(&mut state.events_sink, window, delta, phase);
        if let Some((dx, dy, dpi)) = pan {
            emit_pan(&mut state.events_sink, window, dx, dy, dpi);
        }
    }
}

delegate_dispatch!(WinitState: [ZwpPointerGesturesV1: GlobalData] => PointerGesturesState);
delegate_dispatch!(WinitState: [ZwpPointerGesturePinchV1: PointerGestureData] => PointerGesturesState);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pinch_translation_follows_zoom_and_uses_physical_pixels() {
        for dpi in [1.0, 1.75, 2.0] {
            let mut sink = EventSink::new();
            let window = WindowId(7);
            emit(&mut sink, window, 0.2, TouchPhase::Moved);
            emit_pan(&mut sink, window, 8.0, -4.0, dpi);
            let mut events = sink.drain();
            assert!(matches!(
                events.next(),
                Some(crate::event::Event::WindowEvent {
                    event: WindowEvent::PinchGesture { .. },
                    ..
                })
            ));
            match events.next() {
                Some(crate::event::Event::WindowEvent {
                    event: WindowEvent::PanGesture { delta, phase, .. },
                    ..
                }) => {
                    assert_eq!(delta.x, (8.0 * dpi) as f32);
                    assert_eq!(delta.y, (-4.0 * dpi) as f32);
                    assert_eq!(phase, TouchPhase::Moved);
                }
                _ => panic!("expected accompanying pan event"),
            }
            assert!(events.next().is_none());
        }
        let mut sink = EventSink::new();
        emit_pan(&mut sink, WindowId(7), f64::NAN, 2.0, 1.0);
        emit_pan(&mut sink, WindowId(7), 0.0, 0.0, 1.0);
        assert!(sink.is_empty());
    }

    #[test]
    fn absolute_scales_become_upstream_deltas_and_end_resets_focus() {
        let mut state = PinchState::default();
        let window = WindowId(7);
        assert_eq!(state.update(1.5), None);
        assert_eq!(state.begin(Some(window), 2), Some(window));
        assert_eq!(state.update(1.5), Some((window, 0.5)));
        assert_eq!(state.update(1.25), Some((window, -0.25)));
        assert_eq!(state.end(), Some(window));
        assert_eq!(state.update(2.0), None);
        assert_eq!(state.end(), None);
        state.begin(Some(WindowId(8)), 2);
        assert_eq!(state.update(1.25), Some((WindowId(8), 0.25)));
    }

    #[test]
    fn ignored_begin_clears_previous_surface_and_invalid_scale_does_not_poison_state() {
        let mut state = PinchState::default();
        let window = WindowId(7);
        state.begin(Some(window), 2);
        assert_eq!(state.begin(Some(window), 3), None);
        assert_eq!(state.update(1.5), None);
        state.begin(Some(window), 2);
        assert_eq!(state.begin(None, 2), None);
        assert_eq!(state.end(), None);
        state.begin(Some(window), 2);
        for scale in [f64::NAN, f64::INFINITY, 0.0, -1.0] {
            assert_eq!(state.update(scale), None);
        }
        assert_eq!(state.update(1.25), Some((window, 0.25)));
    }

    #[test]
    fn zero_delta_lifecycle_events_and_cancellation_are_preserved() {
        let mut sink = EventSink::new();
        let window = WindowId(7);
        for phase in [
            TouchPhase::Started,
            TouchPhase::Moved,
            TouchPhase::Cancelled,
        ] {
            emit(&mut sink, window, 0.0, phase);
        }
        let phases: Vec<_> = sink
            .drain()
            .map(|event| match event {
                crate::event::Event::WindowEvent {
                    event: WindowEvent::PinchGesture { delta, phase, .. },
                    ..
                } => {
                    assert_eq!(delta, 0.0);
                    phase
                }
                _ => panic!("unexpected event"),
            })
            .collect();
        assert_eq!(
            phases,
            [
                TouchPhase::Started,
                TouchPhase::Moved,
                TouchPhase::Cancelled
            ]
        );
    }
}
