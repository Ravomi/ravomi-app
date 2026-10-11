//! Native RaTeX layout. Glyphs are rendered by the canvas MSDF cache;
//! fraction rules and stretchy paths remain vector geometry.
use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};
use ratex_types::{
    display_item::{DisplayItem, DisplayList},
    path_command::PathCommand,
};

#[derive(Component)]
pub struct MathText;

pub fn layout(source: &str) -> Result<DisplayList, String> {
    if source.len() > 65_536 {
        return Err("公式超过 64 KiB，请拆分为多个节点".into());
    }
    let source = source.trim();
    let source = source
        .strip_prefix("$$")
        .and_then(|s| s.strip_suffix("$$"))
        .or_else(|| {
            source
                .strip_prefix("\\[")
                .and_then(|s| s.strip_suffix("\\]"))
        })
        .or_else(|| source.strip_prefix('$').and_then(|s| s.strip_suffix('$')))
        .unwrap_or(source);
    let ast = ratex_parser::parser::parse(source).map_err(|error| error.to_string())?;
    let options = ratex_layout::LayoutOptions {
        // An inherited color sentinel distinguishes theme ink from explicit LaTeX colors.
        color: ratex_types::color::Color::new(-1.0, -1.0, -1.0, 1.0),
        ..Default::default()
    };
    let display = ratex_layout::to_display_list(&ratex_layout::layout(&ast, &options));
    if display.items.len() > 20_000
        || !display.width.is_finite()
        || !display.total_height().is_finite()
    {
        return Err("公式排版结果过大或无效".into());
    }
    Ok(display)
}

pub fn ink(color: ratex_types::color::Color, inherited: Color) -> Color {
    if color.r < 0.0 {
        inherited
    } else {
        Color::srgba(color.r, color.g, color.b, color.a)
    }
}

pub fn geometry(
    display: &DisplayList,
    em: f32,
    offset: Vec2,
    inherited: Color,
) -> Result<Option<Mesh>, String> {
    use lyon_tessellation::{
        BuffersBuilder, FillOptions, FillTessellator, FillVertex, StrokeOptions, StrokeTessellator,
        StrokeVertex, VertexBuffers, math::point, path::Path,
    };
    let mut positions = Vec::<[f32; 3]>::new();
    let mut colors = Vec::new();
    let mut indices = Vec::new();
    for item in &display.items {
        let mut builder = Path::builder();
        let mut started = false;
        let (color, fill) = match item {
            DisplayItem::GlyphPath { .. } => continue,
            DisplayItem::Line {
                x,
                y,
                width,
                thickness,
                color,
                dashed,
            } => {
                let dash = if *dashed {
                    (*thickness * 6.0).max(0.01)
                } else {
                    *width
                };
                let mut start = 0.0;
                while start < *width {
                    let end = (start + dash).min(*width);
                    builder.begin(point((*x + start) as f32, (*y - thickness * 0.5) as f32));
                    builder.line_to(point((*x + end) as f32, (*y - thickness * 0.5) as f32));
                    builder.line_to(point((*x + end) as f32, (*y + thickness * 0.5) as f32));
                    builder.line_to(point((*x + start) as f32, (*y + thickness * 0.5) as f32));
                    builder.close();
                    start += dash * 1.5;
                }
                (*color, true)
            }
            DisplayItem::Rect {
                x,
                y,
                width,
                height,
                color,
            } => {
                builder.begin(point(*x as f32, *y as f32));
                builder.line_to(point((x + width) as f32, *y as f32));
                builder.line_to(point((x + width) as f32, (y + height) as f32));
                builder.line_to(point(*x as f32, (y + height) as f32));
                builder.close();
                (*color, true)
            }
            DisplayItem::Path {
                x,
                y,
                commands,
                fill,
                color,
            } => {
                let p = |px: f64, py: f64| point((x + px) as f32, (y + py) as f32);
                for command in commands {
                    match *command {
                        PathCommand::MoveTo { x, y } => {
                            if started {
                                builder.end(false);
                            }
                            builder.begin(p(x, y));
                            started = true;
                        }
                        PathCommand::LineTo { x, y } => {
                            builder.line_to(p(x, y));
                        }
                        PathCommand::QuadTo { x1, y1, x, y } => {
                            builder.quadratic_bezier_to(p(x1, y1), p(x, y));
                        }
                        PathCommand::CubicTo {
                            x1,
                            y1,
                            x2,
                            y2,
                            x,
                            y,
                        } => {
                            builder.cubic_bezier_to(p(x1, y1), p(x2, y2), p(x, y));
                        }
                        PathCommand::Close => {
                            builder.close();
                            started = false;
                        }
                    }
                }
                if started {
                    builder.end(false);
                }
                (*color, *fill)
            }
        };
        let path = builder.build();
        let mut buffers: VertexBuffers<[f32; 2], u32> = VertexBuffers::new();
        if fill {
            FillTessellator::new().tessellate_path(
                &path,
                &FillOptions::default().with_tolerance(0.001),
                &mut BuffersBuilder::new(&mut buffers, |vertex: FillVertex| {
                    vertex.position().to_array()
                }),
            )
        } else {
            StrokeTessellator::new().tessellate_path(
                &path,
                &StrokeOptions::default()
                    .with_line_width(0.04)
                    .with_tolerance(0.001),
                &mut BuffersBuilder::new(&mut buffers, |vertex: StrokeVertex| {
                    vertex.position().to_array()
                }),
            )
        }
        .map_err(|error| format!("公式线条生成失败：{error:?}"))?;
        let base = positions.len() as u32;
        colors.extend(std::iter::repeat_n(
            ink(color, inherited).to_linear().to_f32_array(),
            buffers.vertices.len(),
        ));
        positions.extend(
            buffers
                .vertices
                .into_iter()
                .map(|p| [p[0] * em + offset.x, -p[1] * em + offset.y, 0.0]),
        );
        indices.extend(buffers.indices.into_iter().map(|index| base + index));
    }
    if positions.is_empty() {
        return Ok(None);
    }
    let uvs = vec![[0.5, 0.5]; positions.len()];
    Ok(Some(
        Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::all())
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
            .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
            .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
            .with_inserted_indices(Indices::U32(indices)),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn force_equation_has_fraction_bold_and_chinese_glyphs() {
        let display = layout(r"\begin{equation}\mathbf{F} = \frac{d\mathbf{p}}{dt}, \quad \text{其中 } \mathbf{p} = m\mathbf{v}\end{equation}").unwrap();
        assert!(
            display
                .items
                .iter()
                .any(|item| matches!(item, DisplayItem::Line { .. }))
        );
        assert!(display.items.iter().any(|item| matches!(item, DisplayItem::GlyphPath { font, char_code, .. } if font == "CJK-Regular" && *char_code == '中' as u32)));
        assert!(display.items.iter().any(
            |item| matches!(item, DisplayItem::GlyphPath { font, .. } if font.contains("Bold"))
        ));
        assert!(
            geometry(&display, 32.0, Vec2::ZERO, Color::WHITE)
                .unwrap()
                .is_some()
        );
        assert!(layout(r"\frac{1}{").is_err());
    }
}
