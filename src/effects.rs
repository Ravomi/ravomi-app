//! Canvas feedback effects, expressed in world units (Y up).
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
    StrokeVertex, VertexBuffers, math::point, path::Path,
};
use std::collections::VecDeque;

const MAX_EFFECTS: usize = 512;
const FPS: f32 = 60.0;

#[derive(Resource, Default)]
pub struct Effects {
    pub light: bool,
    items: VecDeque<Effect>,
    preview: Option<CutPreview>,
}

pub struct CutPreview {
    pub line: [Vec2; 2],
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
    },
    Trail {
        node: usize,
        center: Vec2,
        size: Vec2,
        direction: Vec2,
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
        self.push(12.0, Shape::Note { center, size });
    }

    /// Only the successful Enter exit shortcut emits this in master; callers
    /// must not use it for blur, cancellation, or each text input event.
    pub fn commit_edit(&mut self, center: Vec2, size: Vec2, default_zoom: bool) {
        self.begin_edit(center, size, default_zoom);
    }

    /// Supply the rectangle *after* movement and the actual world displacement.
    pub fn move_trail(&mut self, node: usize, center: Vec2, size: Vec2, delta: Vec2) {
        if !valid_rect(center, size) || !delta.is_finite() || delta.length() < 1.0 {
            return;
        }
        let shape = Shape::Trail {
            node,
            center,
            size,
            direction: delta.normalize(),
        };
        // Follow each node instead of accumulating detached particles per frame.
        if let Some(effect) = self
            .items
            .iter_mut()
            .find(|effect| matches!(effect.shape, Shape::Trail { node: id, .. } if id == node))
        {
            effect.frames = 0.0;
            effect.shape = shape;
        } else {
            self.push(10.0, shape);
        }
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
                            .with_alpha(alpha * 0.55),
                        Some(1.5 * world_per_pixel),
                    )?;
                }
            }
            Shape::Note { center, size } => {
                let outline = super::theme::corners::outline_with_radius(
                    *center,
                    *size,
                    super::theme::corners::group_radius(*size, world_per_pixel),
                );
                let path = polyline(&outline, true);
                let tint = super::theme::palette(effects.light).accent;
                // A faint, local edge wash: no expanding rectangle or particles.
                paint(
                    &mut buffers,
                    &path,
                    tint.with_alpha(0.06 * alpha * alpha),
                    Some(3.0 * world_per_pixel),
                )?;
                paint(
                    &mut buffers,
                    &path,
                    tint.with_alpha(0.35 * alpha * alpha),
                    Some(world_per_pixel),
                )?;
            }
            Shape::Trail {
                center,
                size,
                direction,
                ..
            } => {
                let (edge, tangent, length) = if direction.y.abs() > direction.x.abs() {
                    (
                        *center - Vec2::Y * direction.y.signum() * size.y * 0.5,
                        Vec2::X,
                        size.x,
                    )
                } else {
                    (
                        *center - Vec2::X * direction.x.signum() * size.x * 0.5,
                        Vec2::Y,
                        size.y,
                    )
                };
                let half = (length * 0.18).min(18.0 * world_per_pixel);
                let path = polyline(&[edge - tangent * half, edge + tangent * half], false);
                let tint = super::theme::palette(effects.light).accent;
                paint(
                    &mut buffers,
                    &path,
                    tint.with_alpha(0.025 * alpha * alpha),
                    Some(4.0 * world_per_pixel),
                )?;
                paint(
                    &mut buffers,
                    &path,
                    tint.with_alpha(0.16 * alpha * alpha),
                    Some(world_per_pixel),
                )?;
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

#[cfg(test)]
#[path = "effects_tests.rs"]
mod tests;
