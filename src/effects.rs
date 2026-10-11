//! Project Graph master feedback effects, expressed in world units (Y up).
//! Reference: feedbackService/effectEngine/{NodeMoveShadowEffect,
//! RectangleLittleNoteEffect,LineCuttingEffect,RectangleSplitTwoPartEffect}.
//! Rounded cutting geometry and motion follow Godot's stage_object_slicer.gd.
//! Lyon and Bevy's built-in ColorMaterial provide one shared transient mesh;
//! a GPU particle framework is unnecessary for this bounded feedback workload.
use bevy::{
    asset::RenderAssetUsages,
    camera::visibility::NoFrustumCulling,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};
use lyon_tessellation::{
    BuffersBuilder, FillOptions, FillTessellator, FillVertex, StrokeOptions, StrokeTessellator,
    StrokeVertex, VertexBuffers,
    math::{Box2D, point},
    path::{Path, Winding, builder::BorderRadii},
};
use std::{
    collections::{VecDeque, hash_map::RandomState},
    hash::{BuildHasher, Hasher},
};

const MAX_EFFECTS: usize = 512;
const FPS: f32 = 60.0;

#[derive(Resource, Default)]
pub struct Effects {
    pub light: bool,
    items: VecDeque<Effect>,
    preview: Option<CutPreview>,
    entropy: RandomState,
    sample: u64,
}

pub struct CutPreview {
    pub line: [Vec2; 2],
    pub boxes: Vec<(Vec2, Vec2)>,
    pub edges: Vec<(Vec<[Vec2; 2]>, bool)>,
}

struct Effect {
    frames: f32,
    duration: f32,
    shape: Shape,
}

enum Shape {
    Connection {
        points: Vec<Vec2>,
    },
    Note {
        center: Vec2,
        size: Vec2,
        expansion: [f32; 16],
    },
    Trail {
        points: [Vec2; 2],
        velocities: [Vec2; 2],
    },
    Cut {
        from: Vec2,
        to: Vec2,
        width: f32,
        glow: Color,
    },
    Contact {
        point: Vec2,
    },
    Shard {
        polygon: Vec<Vec2>,
        velocity: Vec2,
        fill: Color,
        border: Color,
    },
    Dust {
        particles: Vec<(Vec2, Vec2, f32)>,
    },
    CreateFlash {
        center: Vec2,
        size: Vec2,
        color: Color,
    },
}

impl Effects {
    pub fn connected(&mut self, points: Vec<Vec2>) {
        if points.len() >= 2 && points.iter().all(|p| p.is_finite()) {
            self.push(24.0, Shape::Connection { points });
        }
    }
    pub fn active(&self) -> bool {
        !self.items.is_empty()
    }

    pub fn clear(&mut self) {
        self.items.clear();
        self.preview = None;
    }

    pub fn cutting_preview(&mut self, preview: Option<CutPreview>) {
        self.preview = preview;
    }

    // Reuse std's randomly seeded hashing for visual variation; no custom PRNG.
    fn random(&mut self, low: f32, high: f32) -> f32 {
        let mut hash = self.entropy.build_hasher();
        hash.write_u64(self.sample);
        self.sample = self.sample.wrapping_add(1);
        low + (high - low) * ((hash.finish() >> 40) as f32 / 16_777_216.0)
    }

    fn push(&mut self, duration: f32, shape: Shape) {
        if self.items.len() == MAX_EFFECTS {
            self.items.pop_front();
        }
        self.items.push_back(Effect {
            frames: 0.0,
            duration,
            shape,
        });
    }

    /// `default_zoom` means master's 0.1 < camera zoom < 4, not exactly 100%.
    pub fn begin_edit(&mut self, center: Vec2, size: Vec2, default_zoom: bool) {
        if !default_zoom || !valid_rect(center, size) {
            return;
        }
        let mut expansion = [0.0; 16];
        for i in 1..16 {
            expansion[i] = expansion[i - 1] + self.random(1.0, 2.0);
        }
        self.push(
            15.0,
            Shape::Note {
                center,
                size,
                expansion,
            },
        );
    }

    /// Only the successful Enter exit shortcut emits this in master; callers
    /// must not use it for blur, cancellation, or each text input event.
    pub fn commit_edit(&mut self, center: Vec2, size: Vec2, default_zoom: bool) {
        self.begin_edit(center, size, default_zoom);
    }

    /// Supply the rectangle *after* movement and the actual world displacement.
    pub fn move_trail(&mut self, center: Vec2, size: Vec2, delta: Vec2) {
        if !valid_rect(center, size) || !delta.is_finite() || delta.length() < 1.0 {
            return;
        }
        let mut points = [center; 2];
        let mut velocities = [Vec2::ZERO; 2];
        for i in 0..2 {
            if delta.y.abs() > delta.x.abs() {
                points[i] += Vec2::new(
                    self.random(-0.5, 0.5) * size.x,
                    -delta.y.signum() * size.y * 0.5,
                );
            } else {
                points[i] += Vec2::new(
                    -delta.x.signum() * size.x * 0.5,
                    self.random(-0.5, 0.5) * size.y,
                );
            }
            let speed = -delta * self.random(0.1, 1.0);
            velocities[i] =
                Vec2::from_angle(self.random(-30.0_f32.to_radians(), 30.0_f32.to_radians()))
                    .rotate(speed);
        }
        self.push(30.0, Shape::Trail { points, velocities });
    }

    pub fn cut_line(&mut self, from: Vec2, to: Vec2) {
        if from.is_finite() && to.is_finite() && from.distance_squared(to) > 0.0001 {
            self.push(
                15.0,
                Shape::Cut {
                    from,
                    to,
                    width: from.distance(to) / 10.0,
                    glow: super::theme::palette(self.light).accent,
                },
            );
        }
    }

    pub fn cut_contact(&mut self, point: Vec2) {
        if point.is_finite() {
            self.push(5.0, Shape::Contact { point });
        }
    }

    /// Clip the visible rounded outline, matching Godot's stage_object_slicer.
    #[allow(clippy::too_many_arguments)]
    pub fn split(
        &mut self,
        center: Vec2,
        size: Vec2,
        from: Vec2,
        to: Vec2,
        fill: Color,
        border: Color,
    ) {
        let mut polygons = split_polygons(center, size, from, to);
        if !(polygons.len() == 2 || polygons.len() == 4)
            || polygons
                .iter()
                .any(|p| !(3..=110).contains(&p.len()) || p.iter().any(|v| !v.is_finite()))
        {
            return;
        }
        let four = polygons.len() == 4;
        let direction = (to - from).normalize_or_zero();
        let normal = direction.perp();
        let particles = (0..30)
            .map(|_| {
                let offset = Vec2::new(self.random(-0.5, 0.5), self.random(-0.5, 0.5)) * size;
                (
                    center + offset,
                    offset.normalize_or(Vec2::X) * self.random(30.0, 600.0) / FPS,
                    self.random(1.0, 2.0),
                )
            })
            .collect();
        self.push(30.0, Shape::Dust { particles });
        polygons.sort_by(|a, b| bounds_center(a).x.total_cmp(&bounds_center(b).x));
        for (index, polygon) in polygons.into_iter().enumerate() {
            let velocity = if four {
                let speed = Vec2::Y * self.random(1.0, 11.0).floor();
                Vec2::from_angle(self.random(-45.0_f32.to_radians(), 45.0_f32.to_radians()))
                    .rotate(speed)
            } else {
                let centroid = polygon.iter().copied().sum::<Vec2>() / polygon.len() as f32;
                let side = direction.perp_dot(centroid - from).signum();
                let side = if side == 0.0 {
                    if index == 0 { -1.0 } else { 1.0 }
                } else {
                    side
                };
                normal * side * self.random(90.0, 240.0) / FPS
                    + Vec2::Y * self.random(60.0, 180.0) / FPS
            };
            self.push(
                50.0,
                Shape::Shard {
                    polygon,
                    velocity,
                    fill,
                    border,
                },
            );
        }
    }

    fn advance(&mut self, seconds: f32) {
        if !seconds.is_finite() || seconds <= 0.0 {
            return;
        }
        for effect in &mut self.items {
            effect.frames += seconds * FPS;
        }
        self.items
            .retain(|effect| effect.frames < effect.duration - 0.0001);
    }
}

fn valid_rect(center: Vec2, size: Vec2) -> bool {
    center.is_finite() && size.is_finite() && size.min_element() > 0.0
}

fn bounds_center(points: &[Vec2]) -> Vec2 {
    let min = points
        .iter()
        .copied()
        .fold(Vec2::splat(f32::INFINITY), Vec2::min);
    let max = points
        .iter()
        .copied()
        .fold(Vec2::splat(f32::NEG_INFINITY), Vec2::max);
    (min + max) * 0.5
}

/// Master's rectangle contacts include interior segment endpoints. Corner
/// contacts are deduplicated so a diagonal through two corners remains a cut.
pub fn segment_rect_contacts(center: Vec2, size: Vec2, from: Vec2, to: Vec2) -> Vec<Vec2> {
    if !valid_rect(center, size)
        || !from.is_finite()
        || !to.is_finite()
        || from.distance_squared(to) < 0.0001
    {
        return vec![];
    }
    let min = center - size * 0.5;
    let max = center + size * 0.5;
    let corners = [min, Vec2::new(max.x, min.y), max, Vec2::new(min.x, max.y)];
    let cut = lyon_tessellation::geom::LineSegment {
        from: point(from.x, from.y),
        to: point(to.x, to.y),
    };
    let mut hits: Vec<Vec2> = [from, to]
        .into_iter()
        .filter(|p| p.cmpge(min).all() && p.cmple(max).all())
        .collect();
    for i in 0..4 {
        let a = corners[i];
        let b = corners[(i + 1) % 4];
        let edge = lyon_tessellation::geom::LineSegment {
            from: point(a.x, a.y),
            to: point(b.x, b.y),
        };
        if let Some(hit) = edge.line_intersection(&cut.to_line()) {
            let hit = Vec2::new(hit.x, hit.y);
            let along = (hit - from).dot(to - from) / from.distance_squared(to);
            if (0.0..=1.0).contains(&along)
                && !hits.iter().any(|p: &Vec2| p.distance_squared(hit) < 0.0001)
            {
                hits.push(hit);
            }
        }
    }
    hits
}

fn split_polygons(center: Vec2, size: Vec2, from: Vec2, to: Vec2) -> Vec<Vec<Vec2>> {
    let hits = segment_rect_contacts(center, size, from, to);
    if hits.len() != 2 {
        return vec![];
    }
    let min = center - size * 0.5;
    let max = center + size * 0.5;
    let corners = [min, Vec2::new(max.x, min.y), max, Vec2::new(min.x, max.y)];
    let mut outline = super::theme::corners::outline(center, size)[..104].to_vec();
    outline.dedup_by(|a, b| a.distance_squared(*b) < 0.0001);
    if let Some(inner) = hits
        .into_iter()
        .find(|p| p.cmpgt(min).all() && p.cmplt(max).all())
    {
        return (0..4)
            .map(|i| {
                let triangle = [corners[i], corners[(i + 1) % 4], inner];
                let mut piece = outline.clone();
                for j in 0..3 {
                    piece = clip_side(&piece, triangle[j], triangle[(j + 1) % 3], 1.0);
                }
                piece
            })
            .filter(|p| p.len() >= 3)
            .collect();
    }
    let mut polygons = Vec::new();
    for sign in [-1.0, 1.0] {
        let output = clip_side(&outline, from, to, sign);
        if output.len() < 3 {
            return vec![];
        }
        polygons.push(output);
    }
    polygons
}

fn clip_side(points: &[Vec2], from: Vec2, to: Vec2, sign: f32) -> Vec<Vec2> {
    let mut output = Vec::new();
    for i in 0..points.len() {
        let a = points[i];
        let b = points[(i + 1) % points.len()];
        let da = (to - from).perp_dot(a - from) * sign;
        let db = (to - from).perp_dot(b - from) * sign;
        if da >= 0.0 {
            output.push(a);
        }
        if (da >= 0.0) != (db >= 0.0) {
            output.push(a.lerp(b, da / (da - db)));
        }
    }
    output.dedup_by(|a, b| a.distance_squared(*b) < 0.0001);
    if output.len() > 1
        && output[0].distance_squared(*output.last().expect("nonempty polygon")) < 0.0001
    {
        output.pop();
    }
    output
}

pub fn install(app: &mut App) {
    app.init_resource::<Effects>();
}

/// Schedule before input/editor emission. Draw after those producers. In
/// particular an idle-window wakeup must not age newly emitted effects.
pub fn update(time: Res<Time<Real>>, mut effects: ResMut<Effects>) {
    if effects.active() {
        effects.advance(time.delta_secs());
    }
}

#[derive(Default)]
pub struct Rendered {
    mesh: Option<(Entity, Handle<Mesh>)>,
    visible: bool,
}

fn effect_material() -> ColorMaterial {
    // ColorMaterial::from(WHITE) selects Opaque and discards vertex alpha,
    // turning soft halos and fading debris into solid polygons.
    ColorMaterial {
        alpha_mode: bevy::sprite_render::AlphaMode2d::Blend,
        ..default()
    }
}

pub fn draw(
    mut commands: Commands,
    mut effects: ResMut<Effects>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut rendered: Local<Rendered>,
    mut editor: ResMut<super::editor::Editor>,
    cameras: Query<&Projection, With<Camera2d>>,
) {
    if !effects.active() && effects.preview.is_none() {
        if rendered.visible
            && let Some((entity, _)) = &rendered.mesh
        {
            commands.entity(*entity).insert(Visibility::Hidden);
            rendered.visible = false;
        }
        return;
    }
    let world_per_pixel = cameras
        .iter()
        .find_map(|projection| match projection {
            Projection::Orthographic(p) => Some(p.scale),
            _ => None,
        })
        .unwrap_or(1.0);
    let mesh = match geometry(&effects, world_per_pixel) {
        Ok(mesh) => mesh,
        Err(error) => {
            editor.report_error(error);
            effects.clear();
            if let Some((entity, _)) = &rendered.mesh {
                commands.entity(*entity).insert(Visibility::Hidden);
            }
            rendered.visible = false;
            return;
        }
    };
    if let Some((entity, handle)) = &rendered.mesh {
        if let Some(mut target) = meshes.get_mut(handle) {
            *target = mesh;
        }
        if !rendered.visible {
            commands.entity(*entity).insert(Visibility::Visible);
        }
    } else {
        let handle = meshes.add(mesh);
        let entity = commands
            .spawn((
                Mesh2d(handle.clone()),
                MeshMaterial2d(materials.add(effect_material())),
                Transform::from_xyz(0.0, 0.0, 500.0),
                NoFrustumCulling,
            ))
            .id();
        rendered.mesh = Some((entity, handle));
    }
    rendered.visible = true;
}

#[derive(Clone, Copy)]
struct Vertex {
    position: [f32; 3],
    color: [f32; 4],
}

fn polygon(points: &[Vec2]) -> Path {
    polyline(points, true)
}

fn polyline(points: &[Vec2], closed: bool) -> Path {
    let mut path = Path::builder();
    path.begin(point(points[0].x, points[0].y));
    for p in &points[1..] {
        path.line_to(point(p.x, p.y));
    }
    path.end(closed);
    path.build()
}

fn rectangle(center: Vec2, size: Vec2, radius: f32) -> Path {
    let min = center - size * 0.5;
    let max = center + size * 0.5;
    let mut path = Path::builder();
    path.add_rounded_rectangle(
        &Box2D::new(point(min.x, min.y), point(max.x, max.y)),
        &BorderRadii::new(radius),
        Winding::Positive,
    );
    path.build()
}

fn paint(
    buffers: &mut VertexBuffers<Vertex, u32>,
    path: &Path,
    color: Color,
    stroke: Option<f32>,
) -> Result<(), String> {
    let color = color.to_linear().to_f32_array();
    if let Some(width) = stroke {
        stroke_path(buffers, path, color, width, false)?;
    } else {
        FillTessellator::new()
            .tessellate_path(
                path,
                &FillOptions::default(),
                &mut BuffersBuilder::new(buffers, |v: FillVertex| Vertex {
                    position: [v.position().x, v.position().y, 0.0],
                    color,
                }),
            )
            .map_err(|error| format!("特效填充失败：{error:?}"))?;
    }
    Ok(())
}

fn stroke_path(
    buffers: &mut VertexBuffers<Vertex, u32>,
    path: &Path,
    color: [f32; 4],
    width: f32,
    bounded: bool,
) -> Result<(), String> {
    StrokeTessellator::new()
        .tessellate_path(
            path,
            &StrokeOptions::default()
                .with_line_width(width)
                .with_tolerance(0.01)
                .with_line_join(lyon_tessellation::LineJoin::Round),
            &mut BuffersBuilder::new(buffers, |v: StrokeVertex| {
                let mut p = v.position();
                if bounded {
                    // Lyon rounds the outer join but retains a miter on its inner
                    // side. Wide strokes around near-degenerate blades can leave
                    // that inner vertex outside the intended blur radius.
                    let normal = v.normal();
                    let length = normal.length();
                    if length > 1.0 {
                        p = v.position_on_path() + normal * (width * 0.5 / length);
                    }
                }
                Vertex {
                    position: [p.x, p.y, 0.0],
                    color,
                }
            }),
        )
        .map_err(|error| format!("特效描边失败：{error:?}"))?;
    Ok(())
}

fn cut_blade(from: Vec2, to: Vec2, width: f32, progress: f32, world_per_pixel: f32) -> [Vec2; 4] {
    let direction = (to - from).normalize();
    let width = width.clamp(2.0 * world_per_pixel, 8.0 * world_per_pixel) * (1.0 - progress);
    let tail = from.lerp(to, progress);
    let head = to - direction * 20.0_f32.min(tail.distance(to) * 0.5);
    let side = direction.perp() * width * 0.5;
    [tail, head + side, to, head - side]
}

// Approximate Canvas's Gaussian shadowBlur=15 with densely spaced, round
// contour bands, bounded to 15 screen pixels on either side. Unlike the old
// mitered bands this has no long red spikes at acute blade tips.
#[cfg(test)]
fn glow(
    buffers: &mut VertexBuffers<Vertex, u32>,
    path: &Path,
    color: Color,
    core_width: f32,
    world_per_pixel: f32,
) -> Result<(), String> {
    glow_blurred(buffers, path, color, core_width, 10.0 * world_per_pixel)
}

fn glow_blurred(
    buffers: &mut VertexBuffers<Vertex, u32>,
    path: &Path,
    color: Color,
    core_width: f32,
    blur_world: f32,
) -> Result<(), String> {
    if blur_world <= 0.0001 {
        return Ok(());
    }
    let sigma = blur_world * 0.5;
    let peak = if core_width == 0.0 {
        0.5
    } else {
        (core_width / (sigma * 2.5066)).clamp(0.08, 0.65)
    };
    let mut previous = 0.0;
    for layer in (1..=24).rev() {
        let distance = sigma * 3.0 * layer as f32 / 24.0;
        let target = peak * (-0.5 * (distance / sigma).powi(2)).exp();
        let alpha = (target - previous) / (1.0 - previous);
        stroke_path(
            buffers,
            path,
            color
                .with_alpha(alpha * color.alpha())
                .to_linear()
                .to_f32_array(),
            core_width + 2.0 * distance,
            true,
        )?;
        previous = target;
    }
    Ok(())
}

fn preview_geometry(
    buffers: &mut VertexBuffers<Vertex, u32>,
    preview: &CutPreview,
    world_per_pixel: f32,
) -> Result<(), String> {
    let warning = Color::srgba_u8(243, 139, 168, 153);
    for (center, size) in &preview.boxes {
        paint(
            buffers,
            &polyline(&super::theme::corners::outline(*center, *size), true),
            warning,
            Some(1.5 * world_per_pixel),
        )?;
    }
    for (segments, _) in &preview.edges {
        // EdgePaths contains renderer-flattened segments. Join contiguous
        // segments once so alpha does not accumulate at every curve sample.
        let mut points = Vec::new();
        let width = 1.5 * world_per_pixel;
        for [a, b] in segments {
            if points.last().is_some_and(|last| last != a) {
                paint(buffers, &polyline(&points, false), warning, Some(width))?;
                points.clear();
            }
            if points.is_empty() {
                points.push(*a);
            }
            points.push(*b);
        }
        if points.len() > 1 {
            paint(buffers, &polyline(&points, false), warning, Some(width))?;
        }
    }
    let [from, to] = preview.line;
    let path = if from == to {
        polygon(&[
            from + Vec2::X * (4.0 * world_per_pixel),
            from + Vec2::Y * (4.0 * world_per_pixel),
            from - Vec2::X * (4.0 * world_per_pixel),
            from - Vec2::Y * (4.0 * world_per_pixel),
        ])
    } else {
        polyline(&[from, to], false)
    };
    paint(
        buffers,
        &path,
        Color::srgba_u8(243, 139, 168, 179),
        Some(1.5 * world_per_pixel),
    )?;
    Ok(())
}

fn geometry(effects: &Effects, world_per_pixel: f32) -> Result<Mesh, String> {
    let mut buffers = VertexBuffers::new();
    if let Some(preview) = &effects.preview {
        preview_geometry(&mut buffers, preview, world_per_pixel)?;
    }
    for effect in &effects.items {
        let n = effect.frames;
        let alpha = 1.0 - n / effect.duration;
        match &effect.shape {
            Shape::Connection { points } => {
                let progress = 1.0 - alpha * alpha;
                let lengths: Vec<_> = points.windows(2).map(|p| p[0].distance(p[1])).collect();
                let total: f32 = lengths.iter().sum();
                let head = total * progress;
                let tail = (head - total * 0.22).max(0.0);
                let mut offset = 0.0;
                let mut trail = Vec::new();
                for (segment, length) in points.windows(2).zip(lengths) {
                    if length > 0.0 && offset + length >= tail && offset <= head {
                        trail.push(
                            segment[0].lerp(segment[1], ((tail - offset) / length).clamp(0.0, 1.0)),
                        );
                        trail.push(
                            segment[0].lerp(segment[1], ((head - offset) / length).clamp(0.0, 1.0)),
                        );
                    }
                    offset += length;
                }
                if trail.len() >= 2 && head > 0.0 {
                    paint(
                        &mut buffers,
                        &polyline(&trail, false),
                        super::theme::palette(effects.light)
                            .accent
                            .with_alpha(alpha * 0.85),
                        Some(2.5 * world_per_pixel),
                    )?;
                }
            }
            Shape::Note {
                center,
                size,
                expansion,
            } => {
                let frame = (n.floor() as usize).min(14);
                let grow = expansion[frame] + (expansion[frame + 1] - expansion[frame]) * n.fract();
                paint(
                    &mut buffers,
                    &rectangle(*center, *size + Vec2::splat(grow * 2.0), 8.0),
                    super::theme::palette(effects.light)
                        .accent
                        .with_alpha(alpha),
                    Some(2.0),
                )?;
            }
            Shape::CreateFlash {
                center,
                size,
                color,
            } => {
                let outline = super::theme::corners::outline(*center, *size);
                let path = polyline(&outline, true);
                paint(
                    &mut buffers,
                    &path,
                    color.with_alpha(0.6 * alpha * alpha),
                    Some(1.5 * world_per_pixel),
                )?;
            }
            Shape::Trail { points, velocities } => {
                // Sum of master's per-tick v*(1-t/30), extended continuously.
                let displacement = n - n * (n + 1.0) / 60.0;
                for i in 0..2 {
                    let center = points[i] + velocities[i] * displacement + Vec2::new(0.5, -0.5);
                    paint(
                        &mut buffers,
                        &rectangle(center, Vec2::ONE, 0.0),
                        Color::WHITE.with_alpha(alpha),
                        None,
                    )?;
                }
            }
            Shape::Cut {
                from,
                to,
                width,
                glow: glow_color,
            } => {
                let progress = (1.0 - alpha).sqrt();
                let points = cut_blade(*from, *to, *width, progress, world_per_pixel);
                let path = polygon(&points);
                glow_blurred(
                    &mut buffers,
                    &path,
                    glow_color.with_alpha((1.0 - progress) * 0.3),
                    0.0,
                    1.5 * world_per_pixel,
                )?;
                paint(
                    &mut buffers,
                    &path,
                    super::theme::palette(effects.light)
                        .text
                        .with_alpha(1.0 - progress),
                    None,
                )?;
            }
            Shape::Contact { point } => {
                let radius = 8.0 * world_per_pixel * (1.0 - alpha);
                let tint = Color::srgb_u8(180, 190, 254);
                let base = buffers.vertices.len() as u32;
                buffers.vertices.push(Vertex {
                    position: [point.x, point.y, 0.0],
                    color: tint.with_alpha(alpha).to_linear().to_f32_array(),
                });
                for step in 0..32 {
                    let p = *point
                        + Vec2::from_angle(std::f32::consts::TAU * step as f32 / 32.0) * radius;
                    buffers.vertices.push(Vertex {
                        position: [p.x, p.y, 0.0],
                        color: tint.with_alpha(0.0).to_linear().to_f32_array(),
                    });
                    buffers.indices.extend_from_slice(&[
                        base,
                        base + 1 + step,
                        base + 1 + (step + 1) % 32,
                    ]);
                }
            }
            Shape::Shard {
                polygon: points,
                velocity,
                fill,
                border,
            } => {
                let offset = *velocity * n - Vec2::Y * (0.25 * n * n);
                let points: Vec<_> = points.iter().map(|point| *point + offset).collect();
                let path = polygon(&points);
                paint(
                    &mut buffers,
                    &path,
                    fill.with_alpha(fill.alpha() * alpha),
                    None,
                )?;
                paint(
                    &mut buffers,
                    &path,
                    border.with_alpha(alpha),
                    Some(1.5 * world_per_pixel),
                )?;
            }
            Shape::Dust { particles } => {
                let color = if effects.light {
                    Color::srgb_u8(114, 135, 253)
                } else {
                    Color::srgb_u8(180, 190, 254)
                };
                for (point, velocity, size) in particles {
                    paint(
                        &mut buffers,
                        &rectangle(
                            *point + *velocity * n,
                            Vec2::splat(*size * world_per_pixel),
                            0.0,
                        ),
                        color.with_alpha(alpha),
                        None,
                    )?;
                }
            }
        }
    }
    Ok(
        Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::all())
            .with_inserted_attribute(
                Mesh::ATTRIBUTE_POSITION,
                buffers
                    .vertices
                    .iter()
                    .map(|v| v.position)
                    .collect::<Vec<_>>(),
            )
            .with_inserted_attribute(
                Mesh::ATTRIBUTE_COLOR,
                buffers.vertices.iter().map(|v| v.color).collect::<Vec<_>>(),
            )
            .with_inserted_attribute(
                Mesh::ATTRIBUTE_UV_0,
                vec![[0.0, 0.0]; buffers.vertices.len()],
            )
            .with_inserted_indices(Indices::U32(buffers.indices)),
    )
}

#[path = "creation_effects.rs"]
mod creation;

#[cfg(test)]
#[path = "effects_tests.rs"]
mod tests;
