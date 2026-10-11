# winit 0.30.13: Wayland pinch backport

Ravomi integration note (2026-10-11): this file records dependency provenance and input compatibility, not the product's visual language. Application behavior follows [DESIGN](../../DESIGN.md) and [AI UI rules](../../AI_UI_RULES.md); product scope is defined in [PRD](../../PRD.md). The patch supports canvas navigation and does not implement Agent execution. Upstream attribution, version identifiers and historical validation below remain unchanged.

This directory is the complete published `winit 0.30.13` source, with a narrow
Wayland pointer-gestures patch. License: Apache-2.0, retained in `LICENSE`.

- Source: https://crates.io/crates/winit/0.30.13
- Upstream commit: `e9809ef54b18499bb4f2cac945719ecc2a61061b`
- Crate archive SHA-256:
  `a6755fa58a9f8350bd1e472d4c3fcc25f824ec358933bba33306d0b63df5978d`
- Pinch implementation adapted from winit `v0.31.0-beta.3`, commit
  `7d20408d33210c93bf036335b88ee083ca563a90`:
  https://github.com/rust-windowing/winit/blob/7d20408d33210c93bf036335b88ee083ca563a90/winit-wayland/src/seat/pointer/pointer_gesture.rs
- That upstream source file SHA-256:
  `a2dbb74c3c1f966b83d587aaa9622342879e24bf903136e08276eecade7427f9`

## Scope and compatibility

The application's Cargo patch retains the 0.30.13 API used by Bevy 0.19.1.
No dependencies were upgraded or added inside winit. The existing Smithay
client toolkit 0.19 and Wayland protocol crates already expose pointer gestures.
The backport uses the existing connection, event queue, pointer and seat.
It binds optional `zwp_pointer_gestures_v1` version 1 (pinch is available since
version 1); unsupported compositors continue without native pinch.
Other backends are unchanged. Bevy's `gestures` feature must be enabled.
The source's MSRV remains Rust 1.70; the application pins Rust 1.95 minimum.

Only two-finger pinch and its accompanying center translation are forwarded.
Hold, swipe and rotation are excluded; ordinary two-finger scrolling still uses
existing pointer axis events. Subsurface/client-decoration gestures are ignored.

Each pinch Update emits PinchGesture followed by PanGesture when the logical
center moves. Protocol dx/dy are converted from surface coordinates to physical
pixels using the receiving window's scale factor, as required by Winit PanGesture.
No separate gesture recognizer or input thread is introduced. The application
consumes Bevy's RawWinitWindowEvent stream to retain order, window and phase: a
gesture starts at the cursor and tracks its moving center until Ended/Cancelled.
This is necessary because separate Bevy PinchGesture/PanGesture messages lose
their ordering and phase. See the [protocol definition](https://gitlab.freedesktop.org/wayland/wayland-protocols/-/blob/main/unstable/pointer-gestures/pointer-gestures-unstable-v1.xml).

`PinchGesture.delta` follows upstream winit semantics:
`current_absolute_scale - previous_absolute_scale`, starting at scale 1.
It is neither a scale ratio nor a logarithm. Started, Ended and Cancelled use
delta 0. A new gesture resets the tracked surface and scale. Unknown surfaces,
non-two-finger gestures and non-finite/non-positive scale updates are ignored.
Pointer capability removal and seat removal cancel any active gesture and
destroy its protocol object; reconnecting creates a fresh object.

## Validation

### Wheel source classification (2026-10-09)

`pointer/mod.rs` also preserves the compositor's `axis_source` classification.
An explicitly identified Wheel/WheelTilt frame without a discrete step must not
become a PixelDelta: the canvas interprets pixel scrolling as touchpad pan, which
would interleave panning and zooming during wheel motion. Such partial wheel
frames are ignored until a discrete step arrives; complete steps become LineDelta.
Finger/Continuous remain PixelDelta, including stop frames. Unknown sources retain
upstream behavior. Four unit tests cover partial/complete interleaving, both axes,
direction, DPI, finger stop and legacy source behavior.

The [Wayland pointer specification](https://wayland.freedesktop.org/docs/html/apa.html#protocol-spec-wl_pointer)
defines absolute axis values in surface coordinates, not a fixed number of pixels
per detent. SCTK 0.19 binds pointer version 7 and does not expose `axis_value120`.
Therefore no guessed `absolute / 15` conversion is used. This fixes the confirmed
classification bug while retaining detent-based zoom; fractional high-resolution
wheel zoom remains unsupported, and a compositor emitting only Wheel absolute
values without discrete steps will not scroll. This test evidence does not itself
prove that the user's hardware produced the misclassified sequence.

### Pinch tests

Four unit tests in `pointer_gesture.rs` cover scale differences, end/reset,
ignored begin/finger counts, invalid numbers, lifecycle forwarding and accompanying
pan event order/DPI conversion. Together with the four wheel tests, all eight pass
with `cargo test --release --locked --manifest-path vendor/winit/Cargo.toml
--target-dir /home/waya/.cache/Graplu/winit-validation --lib pointer` on 2026-10-09.
Application compile/test and real touchpad validation are tracked in the
application's VALIDATION.md. A real test must use a native
Wayland window and a compositor exposing pointer gestures; XWayland cannot
exercise this implementation.

To inspect local changes against the published source, compare this directory
with the registry's `winit-0.30.13`: modified upstream files are
`src/event.rs`, `src/platform_impl/linux/wayland/state.rs`,
`src/platform_impl/linux/wayland/seat/mod.rs`, and
`src/platform_impl/linux/wayland/seat/pointer/mod.rs`; the new protocol handler
is `src/platform_impl/linux/wayland/seat/pointer/pointer_gesture.rs`.
