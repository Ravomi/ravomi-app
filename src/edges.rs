use super::*;
use bevy::{
    asset::embedded_asset,
    mesh::{Indices, PrimitiveTopology},
    render::render_resource::AsBindGroup,
    shader::ShaderRef,
    sprite_render::{AlphaMode2d, Material2d, Material2dPlugin},
};
use lyon_tessellation::{
    BuffersBuilder, FillOptions, FillTessellator, FillVertex, StrokeOptions, StrokeTessellator,
    StrokeVertex, VertexBuffers, math::point, path::Path,
};
use project_graph_bevy_canvas::GraphEdge;
use std::collections::BTreeMap;

#[derive(Resource, Default)]
pub struct EdgePaths(pub HashMap<String, EdgePath>);
pub struct EdgePath {
    pub anchor: Vec2,
    pub segments: Vec<[Vec2; 2]>,
}
impl EdgePaths {
    pub fn nearest(&self, graph: &Graph, point: Vec2, tolerance: f32) -> Option<usize> {
        graph
            .edges
            .iter()
            .enumerate()
            .filter_map(|(index, edge)| {
                let path = self.0.get(&edge.id)?;
                let distance = path
                    .segments
                    .iter()
                    .map(|[a, b]| {
                        let a = *a + path.anchor;
                        let segment = *b + path.anchor - a;
                        let t = ((point - a).dot(segment)
                            / segment.length_squared().max(f32::EPSILON))
                        .clamp(0.0, 1.0);
                        point.distance_squared(a + segment * t)
                    })
                    .fold(f32::INFINITY, f32::min);
                (distance <= tolerance * tolerance).then_some((index, distance))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(index, _)| index)
    }
}

#[derive(Asset, TypePath, AsBindGroup, Clone, Debug)]
pub struct EdgeMaterial {
    #[uniform(0)]
    pattern: u32,
}
impl Material2d for EdgeMaterial {
    fn fragment_shader() -> ShaderRef {
        "embedded://project_graph_bevy_canvas/edges.wgsl".into()
    }
    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }
}
#[derive(Component)]
pub struct EdgeRoot {
    id: String,
    edge: GraphEdge,
    rects: Vec<[f32; 4]>,
    shift: i32,
    light: bool,
    render_scale: f32,
    label: Option<Entity>,
}
#[derive(Component)]
pub struct EdgeLabel;
#[derive(Component)]
pub struct LabelBackdrop;
#[derive(Resource)]
pub(super) struct EdgeAssets {
    font: Handle<Font>,
    materials: [Handle<EdgeMaterial>; 3],
}
#[derive(Resource, Default)]
pub struct EdgeStats {
    pub rebuilds: usize,
}

pub fn install(app: &mut App) {
    embedded_asset!(app, "edges.wgsl");
    app.add_plugins(Material2dPlugin::<EdgeMaterial>::default())
        .init_resource::<EdgePaths>()
        .init_resource::<EdgeStats>();
}
pub fn setup(commands: &mut Commands, font: Handle<Font>, materials: &mut Assets<EdgeMaterial>) {
    commands.insert_resource(EdgeAssets {
        font,
        materials: std::array::from_fn(|i| materials.add(EdgeMaterial { pattern: i as u32 })),
    });
}

#[derive(Component)]
pub struct StrokeCache {
    points: Vec<[f32; 3]>,
    size: [f32; 2],
    color: [f32; 4],
    mesh: Handle<Mesh>,
}

fn stroke_mesh(node: &GraphNode) -> Result<Mesh> {
    let size = Vec2::new(node.rect[2], node.rect[3]);
    let points: Vec<_> = node
        .stroke
        .iter()
        .map(|p| Vec2::new(p[0], p[1]) * size - size * 0.5)
        .collect();
    let mut drawing = Drawing::default();
    if let Some(first) = points.first() {
        let mut start = *first;
        for i in 1..points.len() {
            let mut path = Path::builder();
            path.begin(point(start.x, start.y));
            let end = if i + 1 < points.len() {
                let end = (points[i] + points[i + 1]) * 0.5;
                path.quadratic_bezier_to(point(points[i].x, points[i].y), point(end.x, end.y));
                end
            } else {
                path.line_to(point(points[i].x, points[i].y));
                points[i]
            };
            path.end(false);
            let pressure = node.stroke[i.min(points.len() - 2)][2];
            if pressure > 0.0 {
                drawing.stroke(&path.build(), pressure * 5.0, color(node.color), false)?;
            }
            start = end;
        }
    }
    Ok(drawing.mesh())
}

pub fn sync_strokes(
    mut commands: Commands,
    scene: Res<Scene>,
    assets: Res<EdgeAssets>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut editor: ResMut<editor::Editor>,
    roots: Query<(Entity, &NodeVisual, Option<&StrokeCache>)>,
) {
    for (entity, index, cached) in &roots {
        let Some(node) = scene.0.nodes.get(index.0).filter(|n| n.kind == "PenStroke") else {
            continue;
        };
        let size = [node.rect[2], node.rect[3]];
        if cached
            .is_some_and(|c| c.points == node.stroke && c.size == size && c.color == node.color)
        {
            continue;
        }
        let mesh = match stroke_mesh(node) {
            Ok(mesh) => mesh,
            Err(error) => {
                editor.report_error(error.to_string());
                return;
            }
        };
        let handle = if let Some(cached) = cached {
            if let Some(mut existing) = meshes.get_mut(&cached.mesh) {
                *existing = mesh;
            }
            cached.mesh.clone()
        } else {
            let handle = meshes.add(mesh);
            commands.entity(entity).with_children(|parent| {
                parent.spawn((
                    Mesh2d(handle.clone()),
                    MeshMaterial2d(assets.materials[0].clone()),
                    Transform::default(),
                ));
            });
            handle
        };
        commands.entity(entity).insert(StrokeCache {
            points: node.stroke.clone(),
            size,
            color: node.color,
            mesh: handle,
        });
    }
}

// Canonical endpoint groups mirror master StageManager, including reversed edges.
pub fn shifts(graph: &Graph) -> Vec<i32> {
    let mut groups: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (i, e) in graph.edges.iter().enumerate() {
        if e.kind != "LineEdge" || e.members[0] == e.members[1] {
            continue;
        }
        let a = &graph.nodes[e.members[0]].id;
        let b = &graph.nodes[e.members[1]].id;
        let (a, b, r, s) = if a <= b {
            (a, b, e.rates[0], e.rates[1])
        } else {
            (b, a, e.rates[1], e.rates[0])
        };
        groups
            .entry(format!(
                "{a}|{b}|{:.2},{:.2}|{:.2},{:.2}",
                r[0], r[1], s[0], s[1]
            ))
            .or_default()
            .push(i);
    }
    let mut out = vec![0; graph.edges.len()];
    for group in groups.values_mut() {
        group.sort_by_key(|&i| &graph.edges[i].id);
        let half = group.len() as i32 / 2;
        for (j, &i) in group.iter().enumerate() {
            out[i] = j as i32 - half + i32::from(group.len() % 2 == 0 && j as i32 >= half);
        }
    }
    out
}
fn xy(p: Vec2) -> lyon_tessellation::math::Point {
    point(p.x, p.y)
}
fn center_rect(r: [f32; 4]) -> Vec2 {
    Vec2::new(r[0] + r[2] * 0.5, r[1] + r[3] * 0.5)
}
fn normal_rate(r: [f32; 2]) -> Option<Vec2> {
    if r[0] == 0.01 {
        Some(-Vec2::X)
    } else if r[0] == 0.99 {
        Some(Vec2::X)
    } else if r[1] == 0.01 {
        Some(-Vec2::Y)
    } else if r[1] == 0.99 {
        Some(Vec2::Y)
    } else {
        None
    }
}
pub fn endpoint(rect: [f32; 4], rate: [f32; 2], point_node: bool, toward: Vec2) -> Vec2 {
    let c = center_rect(rect);
    if point_node {
        return c;
    }
    if let Some(n) = normal_rate(rate) {
        return c + n * Vec2::new(rect[2], rect[3]) * 0.5;
    }
    if rate != [0.5; 2] {
        return Vec2::new(rect[0] + rect[2] * rate[0], rect[1] + rect[3] * rate[1]);
    }
    let d = toward - c;
    let t = (rect[2] * 0.5 / d.x.abs().max(1e-6)).min(rect[3] * 0.5 / d.y.abs().max(1e-6));
    c + d * t.min(1.0)
}
fn multi_endpoint(rect: [f32; 4], rate: [f32; 2], hub: Vec2) -> Vec2 {
    let inner = Vec2::new(rect[0] + rect[2] * rate[0], rect[1] + rect[3] * rate[1]);
    let delta = hub - inner;
    // MultiTarget uses Rectangle.getLineIntersectionPoint even for sentinel rates.
    for (axis, boundary) in [
        (1, rect[1]),
        (1, rect[1] + rect[3]),
        (0, rect[0]),
        (0, rect[0] + rect[2]),
    ] {
        if delta[axis].abs() <= f32::EPSILON {
            continue;
        }
        let t = (boundary - inner[axis]) / delta[axis];
        let intersection = inner + delta * t;
        let other = 1 - axis;
        if (0.0..=1.0).contains(&t)
            && intersection[other] >= rect[other]
            && intersection[other] <= rect[other] + rect[other + 2]
        {
            return intersection;
        }
    }
    center_rect(rect)
}
fn normal(rect: [f32; 4], rate: [f32; 2], p: Vec2, fallback: Vec2) -> Vec2 {
    if let Some(n) = normal_rate(rate) {
        return n;
    }
    if (p.x - rect[0]).abs() < 0.001 {
        -Vec2::X
    } else if (p.x - rect[0] - rect[2]).abs() < 0.001 {
        Vec2::X
    } else if (p.y - rect[1]).abs() < 0.001 {
        -Vec2::Y
    } else if (p.y - rect[1] - rect[3]).abs() < 0.001 {
        Vec2::Y
    } else {
        fallback
    }
}
#[derive(Clone, Copy, Default)]
struct Vertex {
    position: [f32; 3],
    uv: [f32; 2],
    color: [f32; 4],
}
#[derive(Default)]
struct Drawing {
    buffers: VertexBuffers<Vertex, u32>,
    double_gap: Option<f32>,
    segments: Vec<[Vec2; 2]>,
    dash_scale: f32,
}
impl Drawing {
    fn stroke(&mut self, path: &Path, width: f32, color: Color, patterned: bool) -> Result<()> {
        if patterned {
            use lyon_tessellation::path::{Event, iterator::PathIterator};
            for event in path.iter().flattened(0.15) {
                let (from, to) = match event {
                    Event::Line { from, to } => (from, to),
                    Event::End {
                        last,
                        first,
                        close: true,
                    } => (last, first),
                    _ => continue,
                };
                self.segments
                    .push([Vec2::new(from.x, -from.y), Vec2::new(to.x, -to.y)]);
            }
        }
        let c = color.to_linear().to_f32_array();
        let cross_scale = self
            .double_gap
            .map(|gap| width / gap.max(0.0001))
            .unwrap_or(1.0);
        StrokeTessellator::new()
            .tessellate_path(
                path,
                &StrokeOptions::default()
                    .with_line_width(width)
                    .with_tolerance(0.15),
                &mut BuffersBuilder::new(&mut self.buffers, |v: StrokeVertex| Vertex {
                    position: [v.position().x, -v.position().y, 0.0],
                    uv: [
                        if patterned {
                            v.advancement() / self.dash_scale.max(1.0)
                        } else {
                            -1.0
                        },
                        cross_scale * if v.side().is_positive() { 1.0 } else { -1.0 },
                    ],
                    color: c,
                }),
            )
            .map_err(|e| format!("连线描边失败：{e:?}"))?;
        Ok(())
    }
    fn polygon(
        &mut self,
        points: &[Vec2],
        fill: Color,
        outline: Option<(f32, Color)>,
    ) -> Result<()> {
        let path = polyline(points, true);
        let c = fill.to_linear().to_f32_array();
        FillTessellator::new()
            .tessellate_path(
                &path,
                &FillOptions::default(),
                &mut BuffersBuilder::new(&mut self.buffers, |v: FillVertex| Vertex {
                    position: [v.position().x, -v.position().y, 0.0],
                    uv: [-1.0, 0.0],
                    color: c,
                }),
            )
            .map_err(|e| format!("箭头填充失败：{e:?}"))?;
        if let Some((w, c)) = outline {
            self.stroke(&path, w, c, false)?;
        }
        Ok(())
    }
    fn mesh(self) -> Mesh {
        Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::all())
            .with_inserted_attribute(
                Mesh::ATTRIBUTE_POSITION,
                self.buffers
                    .vertices
                    .iter()
                    .map(|v| v.position)
                    .collect::<Vec<_>>(),
            )
            .with_inserted_attribute(
                Mesh::ATTRIBUTE_UV_0,
                self.buffers
                    .vertices
                    .iter()
                    .map(|v| v.uv)
                    .collect::<Vec<_>>(),
            )
            .with_inserted_attribute(
                Mesh::ATTRIBUTE_COLOR,
                self.buffers
                    .vertices
                    .iter()
                    .map(|v| v.color)
                    .collect::<Vec<_>>(),
            )
            .with_inserted_indices(Indices::U32(self.buffers.indices))
    }
}
fn polyline(points: &[Vec2], closed: bool) -> Path {
    let mut b = Path::builder();
    if let Some(first) = points.first() {
        b.begin(xy(*first));
        for p in &points[1..] {
            b.line_to(xy(*p));
        }
        b.end(closed);
    }
    b.build()
}
fn cubic(a: Vec2, c1: Vec2, c2: Vec2, b: Vec2) -> Path {
    let mut p = Path::builder();
    p.begin(xy(a));
    p.cubic_bezier_to(xy(c1), xy(c2), xy(b));
    p.end(false);
    p.build()
}

// World units per logical window pixel, rounded up in small steps. Keeping the
// bucket in EdgeRoot avoids rebuilding all meshes for sub-pixel wheel updates.
fn render_scale(projection: Option<&Projection>, exporting: bool) -> f32 {
    if exporting {
        return 0.0;
    }
    let Some(Projection::Orthographic(projection)) = projection else {
        return 0.0;
    };
    let scale = projection.scale;
    if !scale.is_finite() || scale <= 0.0 {
        return 0.0;
    }
    1.05_f32.powf((scale.ln() / 1.05_f32.ln()).ceil())
}

fn visible_width(width: f32, scale: f32) -> f32 {
    width.max(1.2 * scale)
}

#[derive(Clone, Copy)]
struct ArrowSize {
    size: f32,
    outline: f32,
}

fn arrow_size(edge: &GraphEdge, width: f32, scale: f32, length: f32) -> ArrowSize {
    let extent = if edge.arrow_type.ends_with("diamond") {
        2.4
    } else if edge.arrow_type.ends_with("triangle") {
        1.8
    } else {
        1.0
    };
    if scale == 0.0 {
        return ArrowSize {
            size: 8.0 * width,
            outline: width,
        };
    }
    // Cap the entire decoration, not just the base of triangles/diamonds, and
    // leave room between nearby nodes when the graph is viewed as an overview.
    let size = (8.0 * width * extent)
        .clamp(6.0 * scale, 24.0 * scale)
        .min(length * 0.35)
        / extent;
    ArrowSize {
        size,
        outline: visible_width(width, scale).min(size * 0.2),
    }
}

#[allow(clippy::too_many_arguments)]
fn arrow(
    drawing: &mut Drawing,
    e: &GraphEdge,
    a: Vec2,
    b: Vec2,
    start_dir: Vec2,
    end_dir: Vec2,
    dimensions: ArrowSize,
    c: Color,
    bg: Color,
) -> Result<()> {
    if !e.arrow || e.arrow_type == "none" {
        return Ok(());
    }
    let size = dimensions.size;
    let back = -end_dir.normalize_or_zero();
    let perp = Vec2::new(-back.y, back.x);
    let hollow = e.arrow_type.starts_with("hollow");
    let points = if e.arrow_type.ends_with("diamond") {
        let dir = start_dir.normalize_or_zero();
        let ds = size * 1.2;
        let perp = Vec2::new(-dir.y, dir.x);
        vec![
            a,
            a + dir * ds + perp * ds * 0.5,
            a + dir * ds * 2.0,
            a + dir * ds - perp * ds * 0.5,
        ]
    } else {
        let s = if e.arrow_type.ends_with("triangle") {
            size * 1.8
        } else {
            size
        };
        let l = b + back * s * 15f32.to_radians().cos() + perp * s * 15f32.to_radians().sin();
        let r = b + back * s * 15f32.to_radians().cos() - perp * s * 15f32.to_radians().sin();
        if e.arrow_type.ends_with("triangle") {
            vec![b, l, r]
        } else {
            vec![b, l, b + back * s * 0.5, r]
        }
    };
    drawing.polygon(
        &points,
        if hollow { bg } else { c },
        hollow.then_some((dimensions.outline, c)),
    )
}

struct Geometry {
    mesh: Mesh,
    label: Vec2,
    font_size: f32,
    segments: Vec<[Vec2; 2]>,
}

struct ClippedArc {
    center: Vec2,
    radius: f32,
    start: f32,
    sweep: f32,
}
impl ClippedArc {
    fn point(&self, t: f32) -> Vec2 {
        let angle = self.start + self.sweep * t;
        self.center + Vec2::new(angle.cos(), angle.sin()) * self.radius
    }
    fn tangent(&self, t: f32) -> Vec2 {
        let angle = self.start + self.sweep * t;
        Vec2::new(-angle.sin(), angle.cos()) * self.sweep.signum()
    }
}

fn clipped_arc(source: [f32; 4], target: [f32; 4], offset: f32) -> Option<ClippedArc> {
    let a = center_rect(source);
    let b = center_rect(target);
    let chord = b - a;
    let length = chord.length();
    if offset.abs() < 0.001 || length < 0.001 {
        return None;
    }
    let normal = Vec2::new(-chord.y, chord.x) / length;
    // The third point is the chord midpoint displaced by the saved sagitta.
    let center_offset = (offset * offset - length * length * 0.25) / (2.0 * offset);
    let center = (a + b) * 0.5 + normal * center_offset;
    let radius = center.distance(a);
    if !radius.is_finite() || radius > 1e6 {
        return None;
    }
    let start = (a.y - center.y).atan2(a.x - center.x);
    let sweep = -offset.signum() * 4.0 * (2.0 * offset.abs() / length).atan();
    let progress = |p: Vec2| {
        ((p.y - center.y).atan2(p.x - center.x) - start)
            .mul_add(sweep.signum(), 0.0)
            .rem_euclid(std::f32::consts::TAU)
    };
    let intersections = |r: [f32; 4]| {
        let mut angles = Vec::new();
        for axis in 0..2 {
            let other = 1 - axis;
            for coordinate in [r[axis], r[axis] + r[axis + 2]] {
                let squared = radius * radius - (coordinate - center[axis]).powi(2);
                if squared < 0.0 {
                    continue;
                }
                for value in [
                    center[other] - squared.sqrt(),
                    center[other] + squared.sqrt(),
                ] {
                    if value < r[other] - 0.001 || value > r[other] + r[other + 2] + 0.001 {
                        continue;
                    }
                    let mut p = Vec2::ZERO;
                    p[axis] = coordinate;
                    p[other] = value;
                    let t = progress(p);
                    if t <= sweep.abs() + 0.0001 {
                        angles.push(t);
                    }
                }
            }
        }
        angles
    };
    let from = intersections(source)
        .into_iter()
        .min_by(f32::total_cmp)
        .unwrap_or(0.0);
    let to = intersections(target)
        .into_iter()
        .max_by(f32::total_cmp)
        .unwrap_or(sweep.abs());
    if from >= to {
        return None;
    }
    Some(ClippedArc {
        center,
        radius,
        start: start + from * sweep.signum(),
        sweep: (to - from) * sweep.signum(),
    })
}

#[cfg(test)]
fn geometry(
    graph: &Graph,
    e: &GraphEdge,
    rects: &[[f32; 4]],
    shift: i32,
    light: bool,
) -> Result<Geometry> {
    geometry_at_scale(graph, e, rects, shift, light, 0.0)
}

fn geometry_at_scale(
    graph: &Graph,
    e: &GraphEdge,
    rects: &[[f32; 4]],
    shift: i32,
    light: bool,
    scale: f32,
) -> Result<Geometry> {
    let p = theme::palette(light);
    let c = if e.color[3] == 0.0 {
        p.border
    } else {
        color(e.color)
    };
    let mut d = Drawing {
        dash_scale: (0.4 * scale).max(1.0),
        ..default()
    };
    let w = if e.kind != "MultiTargetUndirectedEdge" && graph.nodes[e.members[0]].kind == "TextNode"
    {
        graph.nodes[e.members[0]].font_size / 32.0 * 2.0
    } else {
        2.0
    };
    let visible = visible_width(w, scale);
    let gap = (5.0 - w).max(0.8 * scale);
    let stroke_width = if e.line_type == "double" {
        if scale == 0.0 {
            w + 5.0
        } else {
            2.0 * visible + gap
        }
    } else {
        visible
    };
    let font_size = 32.0 * w / 2.0;
    if e.line_type == "double" {
        d.double_gap = Some(gap);
    }
    let label;
    if e.kind == "ArcEdge" {
        if let Some(mut arc) = clipped_arc(rects[0], rects[1], e.arc_offset) {
            label = arc.point(e.text_position);
            let tips = [arc.point(0.0), arc.point(1.0)];
            let directions = [arc.tangent(0.0), arc.tangent(1.0)];
            let decoration = arrow_size(e, w * 15.0 / 16.0, scale, arc.radius * arc.sweep.abs());
            if scale == 0.0 && e.arrow && e.arrow_type.ends_with("diamond") {
                let trim = decoration.size * 2.4;
                let angle = (trim / arc.radius).min(arc.sweep.abs()) * arc.sweep.signum();
                arc.start += angle;
                arc.sweep -= angle;
            }
            d.stroke(
                &circle_path(arc.center, arc.radius, arc.start, arc.sweep),
                stroke_width,
                c,
                true,
            )?;
            // Like master's Canvas2D renderer, paint the decoration last so
            // hollow arrows mask the line body instead of being crossed by it.
            arrow(
                &mut d,
                e,
                tips[0],
                tips[1],
                directions[0],
                directions[1],
                decoration,
                c,
                p.canvas,
            )?;
        } else {
            let a = endpoint(rects[0], [0.5; 2], false, center_rect(rects[1]));
            let b = endpoint(rects[1], [0.5; 2], false, center_rect(rects[0]));
            let dir = (b - a).normalize_or_zero();
            label = a.lerp(b, e.text_position);
            d.stroke(&polyline(&[a, b], false), stroke_width, c, true)?;
            arrow(
                &mut d,
                e,
                a,
                b,
                dir,
                dir,
                arrow_size(e, w, scale, a.distance(b)),
                c,
                p.canvas,
            )?;
        }
    } else if e.kind == "MultiTargetUndirectedEdge" {
        let lo = rects.iter().fold(Vec2::splat(f32::INFINITY), |a, r| {
            a.min(Vec2::new(r[0], r[1]))
        });
        let hi = rects.iter().fold(Vec2::splat(f32::NEG_INFINITY), |a, r| {
            a.max(Vec2::new(r[0] + r[2], r[1] + r[3]))
        });
        label = lo + (hi - lo) * Vec2::from_array(e.center_rate);
        if e.render_type == "line" {
            for (i, r) in rects.iter().enumerate() {
                let a = multi_endpoint(*r, e.rates[i], label);
                d.stroke(&polyline(&[a, label], false), stroke_width, c, true)?;
                if e.multi_arrow != "none" {
                    let mut decoration = e.clone();
                    decoration.arrow = true;
                    let (a, b) = if e.multi_arrow == "inner" {
                        (a, label)
                    } else {
                        (label, a)
                    };
                    arrow(
                        &mut d,
                        &decoration,
                        a,
                        b,
                        (b - a).normalize_or_zero(),
                        (b - a).normalize_or_zero(),
                        arrow_size(&decoration, w, scale, a.distance(b)),
                        c,
                        p.canvas,
                    )?;
                }
            }
        } else {
            let mut corners = vec![];
            for r in rects {
                let lo = Vec2::new(r[0], r[1]) - Vec2::splat(e.padding);
                let hi = Vec2::new(r[0] + r[2], r[1] + r[3]) + Vec2::splat(e.padding);
                corners.extend([lo, Vec2::new(hi.x, lo.y), hi, Vec2::new(lo.x, hi.y)]);
            }
            let path = if e.render_type == "circle" {
                let center = corners.iter().copied().sum::<Vec2>() / corners.len() as f32;
                let radius = corners
                    .iter()
                    .map(|p| p.distance(center))
                    .fold(0.0, f32::max);
                circle_path(center, radius, 0.0, std::f32::consts::TAU)
            } else {
                let hull = avian2d::parry::transformation::convex_hull(&corners);
                polyline(
                    &hull.iter().map(|p| Vec2::new(p.x, p.y)).collect::<Vec<_>>(),
                    true,
                )
            };
            d.stroke(
                &path,
                if e.line_type == "double" {
                    stroke_width
                } else {
                    visible_width(8.0, scale)
                },
                c.with_alpha(c.alpha() * 0.5),
                true,
            )?;
        }
    } else if e.members[0] == e.members[1] {
        let r = rects[0];
        let origin = Vec2::new(r[0], r[1]);
        let radius = (r[3] * 0.5).max(1.0);
        d.stroke(
            &circle_path(
                origin,
                radius,
                std::f32::consts::FRAC_PI_2,
                std::f32::consts::PI * 1.5,
            ),
            stroke_width,
            c,
            true,
        )?;
        let b = origin + Vec2::new(0.0, radius);
        arrow(
            &mut d,
            e,
            b,
            b,
            Vec2::X,
            Vec2::X,
            arrow_size(e, w, scale, radius * std::f32::consts::PI * 1.5),
            c,
            p.canvas,
        )?;
        label = origin - Vec2::new(0.0, 50.0);
    } else {
        let (r, s) = (rects[0], rects[1]);
        let (ra, rb) = (e.rates[0], e.rates[1]);
        let ca = center_rect(r);
        let cb = center_rect(s);
        let (mut a, mut b, da, db, bend) = if shift != 0 {
            let canonical = if graph.nodes[e.members[0]].id <= graph.nodes[e.members[1]].id {
                cb - ca
            } else {
                ca - cb
            };
            let v = canonical.normalize_or_zero();
            let mid = (ca + cb) * 0.5 + Vec2::new(-v.y, v.x) * shift as f32 * 60.0;
            let a = endpoint(r, [0.5; 2], false, mid);
            let b = endpoint(s, [0.5; 2], false, mid);
            (
                a,
                b,
                (mid - ca).normalize_or_zero(),
                (mid - cb).normalize_or_zero(),
                a.distance(b) * 0.5,
            )
        } else {
            let inner_a = Vec2::new(r[0] + r[2] * ra[0], r[1] + r[3] * ra[1]);
            let inner_b = Vec2::new(s[0] + s[2] * rb[0], s[1] + s[3] * rb[1]);
            let a = endpoint(
                r,
                ra,
                graph.nodes[e.members[0]].kind == "ConnectPoint",
                inner_b,
            );
            let b = endpoint(
                s,
                rb,
                graph.nodes[e.members[1]].kind == "ConnectPoint",
                inner_a,
            );
            let direction = (b - a).normalize_or_zero();
            (
                a,
                b,
                normal(r, ra, a, direction),
                normal(s, rb, b, -direction),
                (w * 25.0).max((a.x - b.x).abs().min((a.y - b.y).abs()) * 0.5),
            )
        };
        let tips = [a, b];
        let decoration = arrow_size(e, w, scale, a.distance(b));
        let label_a = if e.arrow && e.arrow_type.ends_with("diamond") {
            a + da * 8.0 * w * 2.4
        } else {
            a
        };
        let label_b = if e.arrow && e.arrow_type != "none" && !e.arrow_type.ends_with("diamond") {
            b + db
                * 8.0
                * w
                * if e.arrow_type.ends_with("triangle") {
                    1.8
                } else {
                    0.5
                }
        } else {
            b
        };
        label =
            (label_a + 3.0 * (label_a + da * bend) + 3.0 * (label_b + db * bend) + label_b) / 8.0;
        // Desktop strokes keep the same cubic and hit path at every zoom.
        // Decorations are painted on top; changing their screen size must not
        // move either control point. Exports retain the original master trim.
        if scale == 0.0 && e.arrow && e.arrow_type != "none" {
            if e.arrow_type.ends_with("diamond") {
                a += da * decoration.size * 2.4;
            } else {
                b += db
                    * decoration.size
                    * if e.arrow_type.ends_with("triangle") {
                        1.8
                    } else {
                        0.5
                    };
            }
        }
        let c1 = a + da * bend;
        let c2 = b + db * bend;
        d.stroke(&cubic(a, c1, c2, b), stroke_width, c, true)?;
        arrow(
            &mut d, e, tips[0], tips[1], da, -db, decoration, c, p.canvas,
        )?;
    }
    Ok(Geometry {
        segments: std::mem::take(&mut d.segments),
        mesh: d.mesh(),
        label: Vec2::new(label.x, -label.y),
        font_size,
    })
}
fn circle_path(center: Vec2, radius: f32, start: f32, sweep: f32) -> Path {
    use lyon_tessellation::geom::{Angle, Arc};
    let arc = Arc {
        center: xy(center),
        radii: lyon_tessellation::math::vector(radius, radius),
        start_angle: Angle::radians(start),
        sweep_angle: Angle::radians(sweep),
        x_rotation: Angle::zero(),
    };
    let mut b = Path::builder();
    b.begin(arc.from());
    arc.for_each_cubic_bezier(&mut |s| {
        b.cubic_bezier_to(s.ctrl1, s.ctrl2, s.to);
    });
    b.end(sweep.abs() >= std::f32::consts::TAU);
    b.build()
}

#[allow(clippy::too_many_arguments)]
pub fn sync(
    mut commands: Commands,
    scene: Res<Scene>,
    display: Option<Res<sections::Display>>,
    physics: Res<physics::PhysicsEditor>,
    interaction: Res<Interaction>,
    mut editor: ResMut<editor::Editor>,
    assets: Res<EdgeAssets>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut roots: Query<(Entity, &mut EdgeRoot, &mut Transform, &Mesh2d), Without<EdgeLabel>>,
    mut labels: Query<(&mut Transform, &mut TextFont, &mut TextColor), With<EdgeLabel>>,
    mut stats: ResMut<EdgeStats>,
    mut paths: ResMut<EdgePaths>,
    mut grouping: Local<(Vec<project_graph_bevy_canvas::GraphEdge>, Vec<i32>)>,
    camera: Query<&Projection, With<Camera2d>>,
    export: Option<Res<export_image::ExportImage>>,
) {
    let scale = render_scale(camera.single().ok(), export.is_some());
    let graph = display
        .as_deref()
        .map_or(&scene.0.graph, |display| display.graph(&scene));
    let needs_sync = interaction.dirty
        || roots.iter().count() != graph.edges.len()
        || roots
            .iter()
            .any(|(_, r, _, _)| r.light != editor.light || r.render_scale != scale);
    if !needs_sync {
        return;
    }
    if grouping.0 != graph.edges || roots.is_empty() {
        grouping.0.clone_from(&graph.edges);
        grouping.1 = shifts(graph);
    }
    let shifts = &grouping.1;
    let existing: HashMap<_, _> = roots.iter().map(|(e, r, _, _)| (r.id.clone(), e)).collect();
    let live: HashSet<_> = graph.edges.iter().map(|e| e.id.as_str()).collect();
    paths.0.retain(|id, _| live.contains(id.as_str()));
    for (id, entity) in &existing {
        if !live.contains(id.as_str()) {
            commands.entity(*entity).despawn();
        }
    }
    for (i, edge) in graph.edges.iter().enumerate() {
        let anchor =
            physics::visual_center(&physics, edge.members[0], &graph.nodes[edge.members[0]]);
        let rects: Vec<_> = edge
            .members
            .iter()
            .map(|&i| {
                let n = &graph.nodes[i];
                let c = physics::visual_center(&physics, i, n) - anchor;
                [
                    c.x - n.rect[2] * 0.5,
                    -c.y - n.rect[3] * 0.5,
                    n.rect[2],
                    n.rect[3],
                ]
            })
            .collect();
        let transform = Transform::from_translation(anchor.extend(0.4));
        if let Some(path) = paths.0.get_mut(&edge.id) {
            path.anchor = anchor;
        }
        if let Some(&entity) = existing.get(&edge.id)
            && let Ok((_, root, mut old_transform, _)) = roots.get_mut(entity)
        {
            old_transform.set_if_neq(transform);
            if root.edge == *edge
                && root.shift == shifts[i]
                && root.light == editor.light
                && root.render_scale == scale
                && root
                    .rects
                    .iter()
                    .zip(&rects)
                    .all(|(a, b)| a.iter().zip(b).all(|(a, b)| (a - b).abs() < 0.02))
            {
                continue;
            }
        }
        let geometry = match geometry_at_scale(graph, edge, &rects, shifts[i], editor.light, scale)
        {
            Ok(g) => g,
            Err(e) => {
                editor.report_error(e.to_string());
                continue;
            }
        };
        stats.rebuilds += 1;
        paths.0.insert(
            edge.id.clone(),
            EdgePath {
                anchor,
                segments: geometry.segments,
            },
        );
        let pattern = match edge.line_type.as_str() {
            "dashed" => 1,
            "double" => 2,
            _ => 0,
        };
        if let Some(&entity) = existing.get(&edge.id)
            && let Ok((_, mut root, _, mesh)) = roots.get_mut(entity)
        {
            if let Some(mut old) = meshes.get_mut(&mesh.0) {
                *old = geometry.mesh;
            }
            commands
                .entity(entity)
                .remove::<bevy::camera::primitives::Aabb>()
                .insert(MeshMaterial2d(assets.materials[pattern].clone()));
            if let Some(label) = root.label
                && let Ok((mut t, mut f, mut c)) = labels.get_mut(label)
            {
                t.set_if_neq(Transform::from_translation(geometry.label.extend(2.0)));
                let size = FontSize::Px(geometry.font_size);
                if f.font_size != size {
                    f.font_size = size;
                }
                c.set_if_neq(TextColor(if edge.color[3] == 0.0 {
                    theme::palette(editor.light).border
                } else {
                    color(edge.color)
                }));
            }
            root.edge = edge.clone();
            root.rects = rects;
            root.shift = shifts[i];
            root.light = editor.light;
            root.render_scale = scale;
            continue;
        }
        let entity = commands
            .spawn((
                Mesh2d(meshes.add(geometry.mesh)),
                MeshMaterial2d(assets.materials[pattern].clone()),
                transform,
            ))
            .id();
        let mut label = None;
        if !edge.text.is_empty() {
            let p = theme::palette(editor.light);
            let c = if edge.color[3] == 0.0 {
                p.border
            } else {
                color(edge.color)
            };
            label = Some(
                commands
                    .spawn((
                        EdgeLabel,
                        MsdfText::new(&edge.text),
                        TextFont {
                            font: assets.font.clone().into(),
                            font_size: FontSize::Px(geometry.font_size),
                            ..default()
                        },
                        TextColor(c),
                        Transform::from_translation(geometry.label.extend(2.0)),
                        ChildOf(entity),
                    ))
                    .id(),
            );
        }
        commands.entity(entity).insert(EdgeRoot {
            id: edge.id.clone(),
            edge: edge.clone(),
            rects,
            shift: shifts[i],
            light: editor.light,
            render_scale: scale,
            label,
        });
    }
}

#[allow(clippy::type_complexity)]
pub fn label_backdrops(
    mut commands: Commands,
    editor: Res<editor::Editor>,
    labels: Query<
        (Entity, &bevy::text::TextLayoutInfo),
        (With<EdgeLabel>, Changed<bevy::text::TextLayoutInfo>),
    >,
    mut backgrounds: Query<(&ChildOf, &mut Sprite), With<LabelBackdrop>>,
) {
    for (_, mut sprite) in &mut backgrounds {
        let canvas = theme::palette(editor.light).canvas;
        if sprite.color != canvas {
            sprite.color = canvas;
        }
    }
    for (entity, info) in &labels {
        let size = info.size + Vec2::splat(6.0);
        if let Some((_, mut sprite)) = backgrounds
            .iter_mut()
            .find(|(parent, _)| parent.parent() == entity)
        {
            sprite.custom_size = Some(size);
        } else {
            commands.spawn((
                LabelBackdrop,
                Sprite::from_color(theme::palette(editor.light).canvas, size),
                Transform::from_xyz(0.0, 0.0, -0.1),
                ChildOf(entity),
            ));
        }
    }
}

pub fn hit_filtered(
    graph: &Graph,
    p: Vec2,
    selecting: bool,
    visible: impl Fn(usize) -> bool,
) -> Option<usize> {
    graph
        .nodes
        .iter()
        .enumerate()
        .rev()
        .filter(|(index, n)| visible(*index) && n.kind != "Section" && (selecting || !n.locked))
        .find(|(_, n)| contains(n, p))
        .map(|(i, _)| i)
        .or_else(|| {
            graph
                .nodes
                .iter()
                .enumerate()
                .rev()
                .find(|(index, n)| {
                    n.kind == "Section"
                        && visible(*index)
                        && (selecting || !n.locked)
                        && contains(n, p)
                        && (n.collapsed
                            || (if selecting {
                                !n.text.is_empty() && -p.y - n.rect[1] < 50.0
                            } else {
                                -p.y - n.rect[1] < 40.0
                            })
                            || (p.x - n.rect[0]) < 12.0
                            || (n.rect[0] + n.rect[2] - p.x) < 12.0
                            || (n.rect[1] + n.rect[3] + p.y) < 12.0)
                })
                .map(|(i, _)| i)
        })
}
pub fn connection_rate(node: &GraphNode, p: Vec2) -> [f32; 2] {
    if node.kind == "ConnectPoint" {
        return [0.5; 2];
    }
    let x = ((p.x - node.rect[0]) / node.rect[2].max(1.0)).clamp(0.0, 1.0);
    let y = ((-p.y - node.rect[1]) / node.rect[3].max(1.0)).clamp(0.0, 1.0);
    if node.kind == "ImageNode" {
        return [x, y];
    }
    if (x - 0.5).abs() < 0.2 && (y - 0.5).abs() < 0.2 {
        return [0.5; 2];
    }
    let distances = [
        x * node.rect[2],
        (1.0 - x) * node.rect[2],
        y * node.rect[3],
        (1.0 - y) * node.rect[3],
    ];
    let i = (0..4)
        .min_by(|&a, &b| distances[a].total_cmp(&distances[b]))
        .unwrap_or(0);
    [[0.01, 0.5], [0.99, 0.5], [0.5, 0.01], [0.5, 0.99]][i]
}
pub fn connection_points(
    graph: &Graph,
    display: Option<&Graph>,
    edge_index: usize,
    scale: f32,
) -> Result<Vec<Vec2>> {
    let edge = &graph.edges[edge_index];
    let rects: Vec<_> = edge
        .members
        .iter()
        .map(|&index| display.unwrap_or(graph).nodes[index].rect)
        .collect();
    let geometry = geometry_at_scale(graph, edge, &rects, shifts(graph)[edge_index], false, scale)?;
    let mut points = Vec::new();
    for [a, b] in geometry.segments {
        if points.is_empty() {
            points.push(a);
        }
        points.push(b);
    }
    Ok(points)
}

fn preview_points(
    graph: &Graph,
    from: usize,
    rate: [f32; 2],
    cursor: Vec2,
    target: Option<usize>,
) -> Vec<Vec2> {
    use lyon_tessellation::path::iterator::PathIterator;
    let source = &graph.nodes[from];
    let cursor = Vec2::new(cursor.x, -cursor.y);
    let b = if let Some(i) = target {
        endpoint(
            graph.nodes[i].rect,
            connection_rate(&graph.nodes[i], Vec2::new(cursor.x, -cursor.y)),
            graph.nodes[i].kind == "ConnectPoint",
            Vec2::new(
                source.rect[0] + source.rect[2] * rate[0],
                source.rect[1] + source.rect[3] * rate[1],
            ),
        )
    } else {
        cursor
    };
    let a = endpoint(source.rect, rate, source.kind == "ConnectPoint", b);
    let dir = (b - a).normalize_or_zero();
    let da = normal(source.rect, rate, a, dir);
    let db = target
        .map(|i| {
            normal(
                graph.nodes[i].rect,
                connection_rate(&graph.nodes[i], Vec2::new(cursor.x, -cursor.y)),
                b,
                -dir,
            )
        })
        .unwrap_or(-dir);
    let width = if source.kind == "TextNode" {
        source.font_size / 32.0 * 2.0
    } else {
        2.0
    };
    let bend = (width * 25.0).max((a.x - b.x).abs().min((a.y - b.y).abs()) * 0.5);
    let mut points = vec![Vec2::new(a.x, -a.y)];
    for event in cubic(a, a + da * bend, b + db * bend, b)
        .iter()
        .flattened(0.25)
    {
        if let lyon_tessellation::path::Event::Line { to, .. } = event {
            points.push(Vec2::new(to.x, -to.y));
        }
    }
    points
}

#[allow(clippy::too_many_arguments)]
pub fn preview(
    graph: &Graph,
    from: usize,
    rate: [f32; 2],
    cursor: Vec2,
    target: Option<usize>,
    gizmos: &mut Gizmos,
    c: Color,
    scale: f32,
) {
    let target = target.filter(|&index| index != from);
    let points = preview_points(graph, from, rate, cursor, target);
    gizmos.linestrip_2d(points.iter().copied(), c.with_alpha(0.8));
    if let [.., previous, tip] = points.as_slice() {
        let direction = (*tip - *previous).normalize_or_zero();
        let normal = Vec2::new(-direction.y, direction.x);
        let extent = (8.0 * scale).min(points[0].distance(*tip) * 0.25);
        gizmos.linestrip_2d(
            [
                *tip - direction * extent + normal * extent * 0.45,
                *tip,
                *tip - direction * extent - normal * extent * 0.45,
            ],
            c,
        );
    }
    if let Some(index) = target {
        let node = &graph.nodes[index];
        let size = Vec2::new(node.rect[2], node.rect[3]);
        gizmos.linestrip_2d(
            theme::corners::outline_with_radius(
                center(node),
                size,
                theme::corners::group_radius(size, scale),
            ),
            c.with_alpha(0.65),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connection_preview_matches_committed_curve_endpoints() {
        let mut graph = stress::synthetic(2, false, false, false).unwrap().graph;
        graph.nodes[0].rect = [0.0, 0.0, 100.0, 60.0];
        graph.nodes[1].rect = [280.0, 150.0, 120.0, 80.0];
        graph.edges[0].rates = vec![[0.99, 0.5], [0.01, 0.5]];
        let preview = preview_points(&graph, 0, [0.99, 0.5], Vec2::new(281.0, -190.0), Some(1));
        let committed = connection_points(&graph, None, 0, 1.0).unwrap();
        assert!(
            preview
                .first()
                .unwrap()
                .distance(*committed.first().unwrap())
                < 0.001
        );
        assert!(preview.last().unwrap().distance(*committed.last().unwrap()) < 0.001);
        assert!(preview.len() > 3);
    }

    #[test]
    fn overview_strokes_keep_a_screen_pixel_floor_without_resizing_labels() {
        use bevy::mesh::VertexAttributeValues;
        let mut doc = stress::synthetic(2, false, false, false).unwrap();
        doc.edges[0].arrow = false;
        let rects = [[0.0, 0.0, 100.0, 100.0], [10_000.0, 0.0, 100.0, 100.0]];
        let original_nodes = doc.nodes.clone();
        let original_edges = doc.edges.clone();
        let reference = geometry(&doc, &doc.edges[0], &rects, 0, false).unwrap();
        for scale in [0.5, 1.0, 2.0, 10.0, 25.0, 50.0, 100.0] {
            let geometry = geometry_at_scale(&doc, &doc.edges[0], &rects, 0, false, scale).unwrap();
            let Some(VertexAttributeValues::Float32x3(vertices)) =
                geometry.mesh.attribute(Mesh::ATTRIBUTE_POSITION)
            else {
                panic!("missing positions");
            };
            let min_y = vertices.iter().map(|p| p[1]).fold(f32::INFINITY, f32::min);
            let max_y = vertices
                .iter()
                .map(|p| p[1])
                .fold(f32::NEG_INFINITY, f32::max);
            assert!((max_y - min_y) / scale >= 1.2 - 0.0001);
            assert_eq!(geometry.font_size, reference.font_size);
            assert_eq!(geometry.label, reference.label);
            assert_eq!(
                geometry.segments, reference.segments,
                "stroke changes must not bend the path"
            );
        }
        assert_eq!(doc.nodes, original_nodes);
        assert_eq!(doc.edges, original_edges);
    }

    #[test]
    fn overview_arrows_have_bounded_screen_size_and_leave_room_on_short_edges() {
        let mut edge = GraphEdge {
            arrow: true,
            ..default()
        };
        for kind in [
            "default",
            "hollow-triangle",
            "filled-triangle",
            "hollow-diamond",
            "filled-diamond",
        ] {
            edge.arrow_type = kind.into();
            for scale in [0.1, 1.0, 10.0, 100.0] {
                let dimensions = arrow_size(&edge, 2.0, scale, 100_000.0);
                let extent = if kind.ends_with("diamond") {
                    2.4
                } else if kind.ends_with("triangle") {
                    1.8
                } else {
                    1.0
                };
                assert!((6.0 - 0.001..=24.0 + 0.001).contains(&(dimensions.size * extent / scale)));
                let short = arrow_size(&edge, 2.0, scale, 10.0);
                assert!(short.size * extent <= 3.5 + 0.0001);
                let mut drawing = Drawing::default();
                arrow(
                    &mut drawing,
                    &edge,
                    Vec2::ZERO,
                    Vec2::new(100_000.0, 0.0),
                    Vec2::X,
                    Vec2::X,
                    dimensions,
                    Color::WHITE,
                    Color::BLACK,
                )
                .unwrap();
                assert!(!drawing.buffers.vertices.is_empty());
                assert!(
                    drawing
                        .buffers
                        .vertices
                        .iter()
                        .all(|v| v.position.iter().all(|x| x.is_finite()))
                );
            }
        }
    }

    #[test]
    fn adaptive_arrow_sizes_do_not_change_curve_or_label_geometry() {
        let mut doc = stress::synthetic(2, false, false, false).unwrap();
        let rects = [[0.0, 0.0, 100.0, 100.0], [600.0, 200.0, 100.0, 100.0]];
        for kind in ["LineEdge", "ArcEdge"] {
            doc.edges[0].kind = kind.into();
            doc.edges[0].arc_offset = 150.0;
            for arrow in ["default", "hollow-triangle", "hollow-diamond"] {
                doc.edges[0].arrow_type = arrow.into();
                let original =
                    geometry_at_scale(&doc, &doc.edges[0], &rects, 0, false, 1.0).unwrap();
                for scale in [5.0, 20.0, 100.0] {
                    let zoomed =
                        geometry_at_scale(&doc, &doc.edges[0], &rects, 0, false, scale).unwrap();
                    assert_eq!(zoomed.segments, original.segments);
                    assert_eq!(zoomed.label, original.label);
                    assert_eq!(zoomed.font_size, original.font_size);
                }
            }
        }
    }

    #[test]
    fn zoom_buckets_refresh_edges_but_reuse_labels_and_pan_geometry() {
        #[derive(Resource, Default)]
        struct LabelChanges(usize);
        let mut doc = stress::synthetic(2, false, false, false).unwrap();
        doc.edges[0].text = "unchanged label".into();
        let mut app = App::new();
        app.insert_resource(Scene(doc))
            .init_resource::<physics::PhysicsEditor>()
            .init_resource::<Interaction>()
            .init_resource::<editor::Editor>()
            .init_resource::<Assets<Mesh>>()
            .init_resource::<EdgeStats>()
            .init_resource::<EdgePaths>()
            .init_resource::<LabelChanges>()
            .insert_resource(EdgeAssets {
                font: default(),
                materials: default(),
            })
            .add_systems(
                Update,
                (
                    sync,
                    |labels: Query<(), (With<EdgeLabel>, Changed<TextFont>)>,
                     mut changes: ResMut<LabelChanges>| {
                        changes.0 += labels.iter().count();
                    },
                )
                    .chain(),
            );
        let camera = app
            .world_mut()
            .spawn((
                Camera2d,
                Projection::Orthographic(OrthographicProjection {
                    scale: 10.1,
                    ..OrthographicProjection::default_2d()
                }),
            ))
            .id();
        app.update();
        let label = app
            .world_mut()
            .query_filtered::<Entity, With<EdgeLabel>>()
            .single(app.world())
            .unwrap();
        let font_size = app.world().get::<TextFont>(label).unwrap().font_size;
        let label_position = *app.world().get::<Transform>(label).unwrap();
        let rebuilds = app.world().resource::<EdgeStats>().rebuilds;
        let label_changes = app.world().resource::<LabelChanges>().0;
        for scale in [10.1, 10.11, 10.12] {
            if let Some(mut projection) = app.world_mut().get_mut::<Projection>(camera)
                && let Projection::Orthographic(p) = &mut *projection
            {
                p.scale = scale;
            }
            app.world_mut()
                .get_mut::<Transform>(camera)
                .unwrap()
                .translation
                .x += 50.0;
            app.world_mut().resource_mut::<Interaction>().dirty = false;
            app.update();
            assert_eq!(app.world().resource::<EdgeStats>().rebuilds, rebuilds);
        }
        if let Some(mut projection) = app.world_mut().get_mut::<Projection>(camera)
            && let Projection::Orthographic(p) = &mut *projection
        {
            p.scale = 20.0;
        }
        app.update();
        assert_eq!(app.world().resource::<EdgeStats>().rebuilds, rebuilds + 1);
        assert_eq!(
            app.world().get::<TextFont>(label).unwrap().font_size,
            font_size
        );
        assert_eq!(
            *app.world().get::<Transform>(label).unwrap(),
            label_position
        );
        assert_eq!(app.world().resource::<LabelChanges>().0, label_changes);
        assert_eq!(
            render_scale(app.world().get::<Projection>(camera), true),
            0.0,
            "PNG export keeps document widths, independently of its camera"
        );
    }

    fn topmost_color_at(mesh: &Mesh, point: Vec2) -> Option<[f32; 4]> {
        use bevy::mesh::VertexAttributeValues;
        let Some(VertexAttributeValues::Float32x3(positions)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("missing positions");
        };
        let Some(VertexAttributeValues::Float32x4(colors)) = mesh.attribute(Mesh::ATTRIBUTE_COLOR)
        else {
            panic!("missing colors");
        };
        let Some(Indices::U32(indices)) = mesh.indices() else {
            panic!("missing triangle indices");
        };
        indices
            .as_chunks::<3>()
            .0
            .iter()
            .rev()
            .find_map(|triangle| {
                let [a, b, c] = std::array::from_fn(|i| {
                    let p = positions[triangle[i] as usize];
                    Vec2::new(p[0], p[1])
                });
                let area = (b - a).perp_dot(c - a);
                if area.abs() < 1e-6 {
                    return None;
                }
                let u = (b - point).perp_dot(c - point) / area;
                let v = (c - point).perp_dot(a - point) / area;
                let w = 1.0 - u - v;
                (u >= 0.0 && v >= 0.0 && w >= 0.0).then_some(colors[triangle[0] as usize])
            })
    }

    #[test]
    fn hollow_arc_arrow_covers_the_line_inside_its_tip() {
        let mut doc = stress::synthetic(2, false, false, false).unwrap();
        doc.edges[0].kind = "ArcEdge".into();
        doc.edges[0].arc_offset = 50.0;
        doc.edges[0].arrow_type = "hollow-triangle".into();
        let rects = [[-10.0, -10.0, 20.0, 20.0], [190.0, -10.0, 20.0, 20.0]];
        let arc = clipped_arc(rects[0], rects[1], 50.0).unwrap();
        let inside = arc.point(0.96);
        for light in [false, true] {
            let geometry = geometry(&doc, &doc.edges[0], &rects, 0, light).unwrap();
            assert_eq!(
                topmost_color_at(&geometry.mesh, Vec2::new(inside.x, -inside.y)),
                Some(theme::palette(light).canvas.to_linear().to_f32_array()),
                "the line must not paint across the hollow arrow"
            );
        }
    }

    #[test]
    fn open_arc_does_not_draw_or_select_its_chord() {
        let mut doc = stress::synthetic(2, false, false, false).unwrap();
        doc.edges[0].kind = "ArcEdge".into();
        doc.edges[0].arrow = false;
        let rects = [[-10.0, -10.0, 20.0, 20.0], [190.0, -10.0, 20.0, 20.0]];
        for offset in [-200.0, -50.0, 50.0, 200.0] {
            doc.edges[0].arc_offset = offset;
            let arc = clipped_arc(rects[0], rects[1], offset).unwrap();
            let mid = (arc.point(0.0) + arc.point(1.0)) * 0.5;
            let mid = Vec2::new(mid.x, -mid.y);
            let g = geometry(&doc, &doc.edges[0], &rects, 0, false).unwrap();
            assert!(topmost_color_at(&g.mesh, mid).is_none());
            let paths = EdgePaths(HashMap::from([(
                doc.edges[0].id.clone(),
                EdgePath {
                    anchor: Vec2::ZERO,
                    segments: g.segments,
                },
            )]));
            assert_eq!(paths.nearest(&doc, mid, 3.0), None);
        }
    }
    #[test]
    fn arc_geometry_matches_saved_sagitta_and_clips_both_rectangles() {
        let source = [-10.0, -10.0, 20.0, 20.0];
        let target = [190.0, -10.0, 20.0, 20.0];
        for offset in [-200.0, -50.0, 50.0, 200.0] {
            let arc = clipped_arc(source, target, offset).unwrap();
            let mid = arc.point(0.5);
            assert!((mid.x - 100.0).abs() < 0.001);
            assert!((mid.y - offset).abs() < 0.001);
            for (p, r) in [(arc.point(0.0), source), (arc.point(1.0), target)] {
                let distances = [
                    (p.x - r[0]).abs(),
                    (p.x - r[0] - r[2]).abs(),
                    (p.y - r[1]).abs(),
                    (p.y - r[1] - r[3]).abs(),
                ];
                assert!(distances.into_iter().any(|d| d < 0.001));
            }
            assert!((arc.tangent(0.0).length() - 1.0).abs() < 0.0001);
        }
        assert!(clipped_arc(source, target, 0.0).is_none());
        assert!(clipped_arc(source, source, 50.0).is_none());
    }

    #[test]
    fn arc_styles_have_finite_meshes_and_selectable_curves() {
        let mut doc = stress::synthetic(2, false, false, false).unwrap();
        let rects: Vec<_> = doc.nodes.iter().map(|n| n.rect).collect();
        let e = &mut doc.edges[0];
        e.kind = "ArcEdge".into();
        for offset in [-200.0, 0.0, 200.0] {
            for style in ["solid", "dashed", "double"] {
                doc.edges[0].arc_offset = offset;
                doc.edges[0].line_type = style.into();
                let g = geometry(&doc, &doc.edges[0], &rects, 0, false).unwrap();
                assert!(g.mesh.count_vertices() > 6);
                assert!(g.label.is_finite());
                assert!(!g.segments.is_empty());
                assert!(g.segments.iter().flatten().all(|p| p.is_finite()));
            }
        }
    }
    #[test]
    fn pen_mesh_uses_local_curve_geometry_and_pressure() {
        use bevy::mesh::VertexAttributeValues;
        let mut doc = stress::synthetic(1, false, false, false).unwrap();
        let node = &mut doc.nodes[0];
        node.kind = "PenStroke".into();
        node.rect = [1000.0, -2000.0, 100.0, 80.0];
        node.stroke = vec![[0.0, 0.0, 0.2], [0.5, 1.0, 1.0], [1.0, 0.0, 0.5]];
        let mesh = stroke_mesh(node).unwrap();
        let Some(VertexAttributeValues::Float32x3(vertices)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("missing vertices");
        };
        assert!(vertices.len() > 6);
        assert!(
            vertices
                .iter()
                .all(|p| p.iter().all(|v| v.is_finite()) && p[0].abs() < 54.0 && p[1].abs() < 44.0)
        );
        let original = vertices.clone();
        node.rect[0] += 500.0;
        node.rect[1] -= 500.0;
        let moved = stroke_mesh(node).unwrap();
        let Some(VertexAttributeValues::Float32x3(vertices)) =
            moved.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("missing vertices");
        };
        assert_eq!(vertices, &original);
        node.stroke.iter_mut().for_each(|p| p[2] = 0.0);
        assert_eq!(stroke_mesh(node).unwrap().count_vertices(), 0);
    }
    #[test]
    fn master_endpoints_and_reverse_parallel_edges() {
        assert_eq!(
            endpoint([10.0, 20.0, 1000.0, 200.0], [0.01, 0.5], false, Vec2::ZERO),
            Vec2::new(10.0, 120.0)
        );
        assert_eq!(
            endpoint(
                [0.0, 0.0, 100.0, 100.0],
                [0.5; 2],
                false,
                Vec2::new(250.0, 50.0)
            ),
            Vec2::new(100.0, 50.0)
        );
        let mut doc = stress::synthetic(2, false, false, false).unwrap();
        let mut reverse = doc.edges[0].clone();
        reverse.id = "reverse".into();
        reverse.members.reverse();
        reverse.rates.reverse();
        doc.edges.push(reverse);
        let shifts = shifts(&doc);
        assert_eq!(shifts, [-1, 1]);
        for (i, e) in doc.edges.iter().enumerate() {
            let rects: Vec<_> = e.members.iter().map(|&i| doc.nodes[i].rect).collect();
            let g = geometry(&doc, e, &rects, shifts[i], false).unwrap();
            assert!(g.mesh.count_vertices() > 6);
        }
    }
    #[test]
    fn multi_target_rates_clip_from_the_inner_point_without_line_edge_sentinels() {
        let rect = [0.0, 0.0, 100.0, 100.0];
        let point = multi_endpoint(rect, [0.25, 0.75], Vec2::new(200.0, 25.0));
        assert!(point.distance(Vec2::new(100.0, 75.0 - 50.0 * 75.0 / 175.0)) < 1e-4);
        let sentinel = multi_endpoint(rect, [0.01, 0.5], Vec2::new(250.0, 100.0));
        assert!(sentinel.distance(Vec2::new(100.0, 50.0 + 50.0 * 99.0 / 249.0)) < 1e-4);
        assert_ne!(
            sentinel,
            endpoint(rect, [0.01, 0.5], false, Vec2::new(250.0, 100.0))
        );
        assert_eq!(
            multi_endpoint(rect, [0.25, 0.75], Vec2::new(50.0, 50.0)),
            Vec2::splat(50.0)
        );
    }
    #[test]
    fn translation_reuses_meshes_but_endpoint_changes_rebuild_only_incident_edge() {
        let doc = stress::synthetic(4, false, false, false).unwrap();
        let mut app = App::new();
        app.insert_resource(Scene(doc))
            .init_resource::<physics::PhysicsEditor>()
            .init_resource::<Interaction>()
            .init_resource::<editor::Editor>()
            .init_resource::<Assets<Mesh>>()
            .init_resource::<EdgeStats>()
            .init_resource::<EdgePaths>()
            .insert_resource(EdgeAssets {
                font: default(),
                materials: default(),
            })
            .add_systems(Update, sync);
        app.update();
        assert_eq!(app.world().resource::<EdgeStats>().rebuilds, 2);
        let id = app.world().resource::<Scene>().0.edges[0].id.clone();
        let path_before = &app.world().resource::<EdgePaths>().0[&id];
        let anchor_before = path_before.anchor;
        let segments_before = path_before.segments.clone();
        {
            let mut scene = app.world_mut().resource_mut::<Scene>();
            for n in &mut scene.0.nodes {
                n.rect[0] += 30.0;
                n.rect[1] += 20.0;
            }
        }
        app.world_mut().resource_mut::<Interaction>().dirty = true;
        app.update();
        assert_eq!(app.world().resource::<EdgeStats>().rebuilds, 2);
        let path_after = &app.world().resource::<EdgePaths>().0[&id];
        assert_eq!(path_after.segments, segments_before);
        assert_eq!(path_after.anchor, anchor_before + Vec2::new(30.0, -20.0));
        app.world_mut().resource_mut::<Scene>().0.nodes[0].rect[0] += 15.0;
        app.update();
        assert_eq!(app.world().resource::<EdgeStats>().rebuilds, 3);
        app.world_mut().resource_mut::<Scene>().0.edges.pop();
        app.update();
        assert_eq!(
            app.world_mut()
                .query::<&EdgeRoot>()
                .iter(app.world())
                .count(),
            1
        );
    }

    #[test]
    fn editing_line_style_updates_the_existing_edges_material() {
        let mut materials = Assets::<EdgeMaterial>::default();
        let handles = std::array::from_fn(|i| materials.add(EdgeMaterial { pattern: i as u32 }));
        let mut app = App::new();
        app.insert_resource(Scene(stress::synthetic(2, false, false, false).unwrap()))
            .init_resource::<physics::PhysicsEditor>()
            .init_resource::<Interaction>()
            .init_resource::<editor::Editor>()
            .init_resource::<Assets<Mesh>>()
            .init_resource::<EdgeStats>()
            .init_resource::<EdgePaths>()
            .insert_resource(EdgeAssets {
                font: default(),
                materials: handles.clone(),
            })
            .add_systems(Update, sync);
        for (style, expected) in [("solid", 0), ("dashed", 1), ("double", 2), ("solid", 0)] {
            app.world_mut().resource_mut::<Scene>().0.edges[0].line_type = style.into();
            app.world_mut().resource_mut::<Interaction>().dirty = true;
            app.update();
            let material = app
                .world_mut()
                .query::<&MeshMaterial2d<EdgeMaterial>>()
                .single(app.world())
                .unwrap();
            assert_eq!(material.0, handles[expected], "line style {style}");
        }
    }
    #[test]
    fn self_loop_and_multi_target_paths_are_finite() {
        let doc = stress::synthetic(4, false, false, false).unwrap();
        let mut e = doc.edges[0].clone();
        let check = |e: &GraphEdge| {
            let rects: Vec<_> = e.members.iter().map(|&i| doc.nodes[i].rect).collect();
            let g = geometry(&doc, e, &rects, 0, false).unwrap();
            assert!(g.label.is_finite());
            assert_eq!(
                g.font_size,
                if e.kind == "LineEdge" {
                    doc.nodes[e.members[0]].font_size
                } else {
                    32.0
                }
            );
            assert!(g.mesh.count_vertices() > 0);
            let Some(bevy::mesh::VertexAttributeValues::Float32x3(positions)) =
                g.mesh.attribute(Mesh::ATTRIBUTE_POSITION)
            else {
                panic!("edge mesh must contain positions")
            };
            assert!(positions.iter().flatten().all(|v| v.is_finite()));
            let Some(bevy::mesh::VertexAttributeValues::Float32x2(uvs)) =
                g.mesh.attribute(Mesh::ATTRIBUTE_UV_0)
            else {
                panic!("edge mesh must contain pattern coordinates")
            };
            assert!(uvs.iter().flatten().all(|v| v.is_finite()));
        };
        for line_type in ["solid", "dashed", "double"] {
            e.line_type = line_type.into();
            for arrow_type in [
                "none",
                "default",
                "hollow-triangle",
                "filled-triangle",
                "hollow-diamond",
                "filled-diamond",
            ] {
                e.arrow_type = arrow_type.into();
                e.kind = "LineEdge".into();
                e.rates = vec![[0.5; 2]; 2];
                for members in [vec![0, 0], vec![0, 1]] {
                    e.members = members;
                    check(&e);
                }
                e.kind = "MultiTargetUndirectedEdge".into();
                e.members = vec![0, 1, 2];
                e.rates = vec![[0.5; 2]; 3];
                for kind in ["line", "convex", "circle"] {
                    e.render_type = kind.into();
                    for direction in ["none", "inner", "outer"] {
                        e.multi_arrow = direction.into();
                        check(&e);
                    }
                }
            }
        }
    }
}
