//! Per-glyph interface to the maintained Kayak UI MSDF generator.

// Keep the upstream geometry API's explicit coordinate arguments.
#[allow(dead_code, clippy::too_many_arguments)]
mod msdf;

use msdf::{bitmap::FloatRGBBmp, ttf_parser::ShapeBuilder, vector::Vector2};

pub const TEXELS_PER_EM: f32 = 64.0;
pub const PIXEL_RANGE: f32 = 8.0;

#[derive(Debug)]
pub struct GlyphBitmap {
    pub width: u32,
    pub height: u32,
    /// Left, bottom, right, top relative to the baseline, in em units.
    pub plane: [f32; 4],
    pub rgba: Vec<u8>,
}

#[derive(Debug)]
pub enum GeneratedGlyph {
    Outline(Option<GlyphBitmap>),
    Color,
}

pub fn generate(
    data: &[u8],
    face_index: u32,
    glyph_id: u16,
) -> Result<Option<GlyphBitmap>, String> {
    match generate_for_rendering(data, face_index, glyph_id)? {
        GeneratedGlyph::Outline(bitmap) => Ok(bitmap),
        GeneratedGlyph::Color => Err(format!(
            "Glyph {glyph_id} uses a color or bitmap representation"
        )),
    }
}

pub fn generate_for_rendering(
    data: &[u8],
    face_index: u32,
    glyph_id: u16,
) -> Result<GeneratedGlyph, String> {
    // Upstream assumes well-formed contours. A font must not take down the editor.
    std::panic::catch_unwind(|| generate_inner(data, face_index, glyph_id))
        .map_err(|_| format!("Kayak MSDF failed for glyph {glyph_id}"))?
}

fn generate_inner(data: &[u8], face_index: u32, glyph_id: u16) -> Result<GeneratedGlyph, String> {
    let face = ttf_parser::Face::parse(data, face_index).map_err(|e| e.to_string())?;
    if glyph_id >= face.number_of_glyphs() {
        return Err(format!("Invalid glyph index {glyph_id}"));
    }
    let id = ttf_parser::GlyphId(glyph_id);
    if face.is_color_glyph(id)
        || face.glyph_svg_image(id).is_some()
        || face.glyph_raster_image(id, u16::MAX).is_some()
    {
        return Ok(GeneratedGlyph::Color);
    }
    let mut builder = ShapeBuilder {
        pixel_scale: f64::from(TEXELS_PER_EM) / f64::from(face.units_per_em()),
        ..Default::default()
    };
    if face
        .outline_glyph(ttf_parser::GlyphId(glyph_id), &mut builder)
        .is_none()
    {
        if face
            .glyph_bounding_box(ttf_parser::GlyphId(glyph_id))
            .is_some()
        {
            return Err(format!("Glyph {glyph_id} has no supported vector outline"));
        }
        return Ok(GeneratedGlyph::Outline(None));
    }
    let mut shape = builder.build();
    shape.contours.retain(|contour| !contour.edges.is_empty());
    if shape.contours.is_empty() {
        return Ok(GeneratedGlyph::Outline(None));
    }
    shape.normalized();
    shape.inverse_y_axis = true;
    let (left, bottom, right, top) = shape.get_bounds();
    let margin = f64::from(PIXEL_RANGE) * 0.5 + 1.0;
    let left = left.floor() - margin;
    let bottom = bottom.floor() - margin;
    let width = (right.ceil() + margin - left) as u32;
    let height = (top.ceil() + margin - bottom) as u32;
    if width == 0 || height == 0 || width > 512 || height > 512 {
        return Err(format!(
            "MSDF glyph exceeds the 512×512 limit: {width}×{height}"
        ));
    }
    msdf::edge_coloring::simple(&mut shape, 3.0, 0);
    let mut bitmap = FloatRGBBmp::new(width as usize, height as usize);
    msdf::gen::generate_msdf(
        &mut bitmap,
        &shape,
        f64::from(PIXEL_RANGE),
        Vector2::new(1.0, 1.0),
        Vector2::new(-left, -bottom),
        1.111_111_111_111_111_2,
    );
    // CFF and TrueType use opposite contour orientations. The padded corner is
    // outside the glyph, so its median establishes the sign expected by WGSL.
    let corner = bitmap.get_pixel(0, 0);
    let invert = corner
        .r
        .max(corner.g.min(corner.b))
        .min(corner.g.max(corner.b))
        > 0.5;
    let mut rgba = Vec::with_capacity(width as usize * height as usize * 4);
    for y in 0..height as usize {
        for x in 0..width as usize {
            let pixel = bitmap.get_pixel(x, y);
            if !pixel.r.is_finite() || !pixel.g.is_finite() || !pixel.b.is_finite() {
                return Err(format!("Non-finite MSDF distance for glyph {glyph_id}"));
            }
            rgba.extend([pixel.r, pixel.g, pixel.b].map(|v| {
                let v = if invert { 1.0 - v } else { v };
                (v.clamp(0.0, 1.0) * 255.0).round() as u8
            }));
            rgba.push(255);
        }
    }
    Ok(GeneratedGlyph::Outline(Some(GlyphBitmap {
        width,
        height,
        plane: [
            left as f32,
            bottom as f32,
            left as f32 + width as f32,
            bottom as f32 + height as f32,
        ]
        .map(|v| v / TEXELS_PER_EM),
        rgba,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ttf_parser::OutlineBuilder;

    #[test]
    fn closes_font_contours_and_preserves_edges_and_bounds() {
        let mut builder = ShapeBuilder {
            pixel_scale: 1.0,
            ..Default::default()
        };
        builder.move_to(2.0, 3.0);
        builder.line_to(8.0, 3.0);
        builder.line_to(8.0, 9.0);
        builder.line_to(2.0, 9.0);
        builder.close();
        let mut shape = builder.build();
        assert_eq!(shape.contours[0].edges.len(), 4);
        shape.normalized();
        assert_eq!(shape.contours[0].edges.len(), 4);
        assert_eq!(shape.get_bounds(), (2.0, 3.0, 8.0, 9.0));
        let mut second = msdf::contour::Contour::new();
        second.add_line(-4.0, -5.0, 14.0, 15.0);
        shape.contours.push(second);
        assert_eq!(shape.get_bounds(), (-4.0, -5.0, 14.0, 15.0));
    }

    #[test]
    fn rejects_invalid_font_without_panicking() {
        assert!(generate(b"invalid", 0, 0).is_err());
    }

    #[test]
    #[ignore = "requires installed Noto Sans CJK font"]
    fn cjk_distance_sign_matches_outline() {
        let data =
            std::fs::read("/usr/share/fonts/google-noto-sans-cjk-fonts/NotoSansCJK-Regular.ttc")
                .unwrap();
        let face = ttf_parser::Face::parse(&data, 0).unwrap();
        for character in "路径打开中文编辑文字节点图缩放选择AVWXYZ08@".chars() {
            let glyph = face.glyph_index(character).unwrap();
            let bitmap = generate(&data, 0, glyph.0).unwrap().unwrap();
            let mut builder = ShapeBuilder {
                pixel_scale: TEXELS_PER_EM as f64 / face.units_per_em() as f64,
                ..Default::default()
            };
            face.outline_glyph(glyph, &mut builder).unwrap();
            let shape = builder.build();
            // An independent polygon winding check avoids trusting MSDF's own
            // nearest-edge sign when looking for detached ink and missing strokes.
            let segments: Vec<_> = shape
                .contours
                .iter()
                .flat_map(|c| &c.edges)
                .flat_map(|edge| {
                    (0..64).map(move |i| {
                        (
                            edge.point(i as f64 / 64.0),
                            edge.point((i + 1) as f64 / 64.0),
                        )
                    })
                })
                .collect();
            let mut wrong = Vec::new();
            let samples = if "路径".contains(character) { 4 } else { 1 };
            for y in 0..bitmap.height * samples {
                for x in 0..bitmap.width * samples {
                    let sx = (x as f64 + 0.5) / samples as f64;
                    let sy = (y as f64 + 0.5) / samples as f64;
                    let p = Vector2::new(
                        bitmap.plane[0] as f64 * TEXELS_PER_EM as f64 + sx,
                        bitmap.plane[3] as f64 * TEXELS_PER_EM as f64 - sy,
                    );
                    let mut winding = 0;
                    let mut distance = f64::INFINITY;
                    for &(a, b) in &segments {
                        let ab = b - a;
                        let length2 = Vector2::dot_product(ab, ab);
                        if length2 == 0.0 {
                            continue;
                        }
                        let t = (Vector2::dot_product(p - a, ab) / length2).clamp(0.0, 1.0);
                        distance = distance.min((p - (a + ab * t)).length());
                        let cross = Vector2::cross_product(ab, p - a);
                        if a.y <= p.y && b.y > p.y && cross > 0.0 {
                            winding += 1;
                        }
                        if a.y > p.y && b.y <= p.y && cross < 0.0 {
                            winding -= 1;
                        }
                    }
                    let tx = (sx - 0.5).max(0.0);
                    let ty = (sy - 0.5).max(0.0);
                    let sample = |x: u32, y: u32, channel: usize| {
                        bitmap.rgba[((y.min(bitmap.height - 1) * bitmap.width
                            + x.min(bitmap.width - 1))
                            * 4) as usize
                            + channel] as f64
                    };
                    let mut rgb = std::array::from_fn::<_, 3, _>(|channel| {
                        let a = sample(tx as u32, ty as u32, channel) * (1.0 - tx.fract())
                            + sample(tx as u32 + 1, ty as u32, channel) * tx.fract();
                        let b = sample(tx as u32, ty as u32 + 1, channel) * (1.0 - tx.fract())
                            + sample(tx as u32 + 1, ty as u32 + 1, channel) * tx.fract();
                        a * (1.0 - ty.fract()) + b * ty.fract()
                    });
                    rgb.sort_by(f64::total_cmp);
                    // Exclude the sub-texel edge band: bilinear reconstruction
                    // of sharp corners is approximate at this atlas resolution.
                    if distance > 0.6 && (rgb[1] >= 127.5) != (winding != 0) {
                        wrong.push((x, y));
                    }
                }
            }
            assert!(wrong.is_empty(), "{character}: incorrect ink at {wrong:?}");
        }
    }

    #[test]
    fn coloring_teardrop_does_not_add_empty_edges() {
        let mut builder = ShapeBuilder {
            pixel_scale: 1.0,
            ..Default::default()
        };
        builder.move_to(0.0, 0.0);
        builder.quad_to(4.0, 4.0, 0.0, 8.0);
        builder.quad_to(-4.0, 12.0, 0.0, 0.0);
        builder.close();
        let mut shape = builder.build();
        msdf::edge_coloring::simple(&mut shape, 3.0, 0);
        assert_eq!(shape.contours[0].edges.len(), 6);
        for edge in &shape.contours[0].edges {
            assert_ne!(edge.point(0.0), edge.point(1.0));
        }
    }
}
