//! Canvas profile matching Godot's `continuous_corners.gd` / `text_node.gd`.
//! Rendering uses the existing resvg and Bevy nine-slice implementations.
use bevy::math::{Vec2, cubic_splines::CubicSegment};
use std::fmt::Write;

pub const MAX_RADIUS: usize = 24;

pub fn radius(size: Vec2) -> usize {
    // text_node.gd calls fitted_radius(size, PANEL, 0.0): retain four straight
    // units between opposite corners, independently of the live outline width.
    (size.min_element() * 0.5 - 2.0)
        .clamp(0.0, MAX_RADIUS as f32)
        .floor() as usize
}

/// Keep expanded groups recognisable at a distance without turning small ones
/// into pills. The scale is world units per logical window pixel.
pub fn group_radius(size: Vec2, world_per_pixel: f32) -> f32 {
    (MAX_RADIUS as f32 * world_per_pixel).min(size.min_element().max(0.0) * 0.25)
}

fn corner(size: Vec2, radius: f32, index: usize) -> [Vec2; 7] {
    let origins = [
        Vec2::new(size.x - radius, 0.0),
        Vec2::new(size.x, size.y - radius),
        Vec2::new(radius, size.y),
        Vec2::new(0.0, radius),
    ];
    [
        Vec2::ZERO,
        Vec2::new(0.34, 0.0),
        Vec2::new(0.587_401_03, 0.0),
        Vec2::new(0.793_700_5, 0.206_299_47),
        Vec2::new(1.0, 0.412_598_94),
        Vec2::new(1.0, 0.66),
        Vec2::ONE,
    ]
    .map(|point| {
        let p = point * radius;
        origins[index]
            + match index {
                0 => p,
                1 => Vec2::new(-p.y, p.x),
                2 => -p,
                _ => Vec2::new(p.y, -p.x),
            }
    })
}

pub fn path(size: Vec2, radius: f32, padding: f32) -> String {
    let mut path = String::new();
    for index in 0..4 {
        let p = corner(size, radius, index).map(|point| point + Vec2::splat(padding));
        write!(
            path,
            "{} {} {} ",
            if index == 0 { 'M' } else { 'L' },
            p[0].x,
            p[0].y
        )
        .expect("writing to a String cannot fail");
        for offset in [0, 3] {
            write!(
                path,
                "C {} {} {} {} {} {} ",
                p[offset + 1].x,
                p[offset + 1].y,
                p[offset + 2].x,
                p[offset + 2].y,
                p[offset + 3].x,
                p[offset + 3].y,
            )
            .expect("writing to a String cannot fail");
        }
    }
    path.push('Z');
    path
}

pub fn outline(center: Vec2, size: Vec2) -> [Vec2; 105] {
    outline_with_radius(center, size, radius(size) as f32)
}

pub fn outline_with_radius(center: Vec2, size: Vec2, radius: f32) -> [Vec2; 105] {
    let mut points = [Vec2::ZERO; 105];
    let mut cursor = 0;
    for index in 0..4 {
        let p = corner(size, radius, index);
        for offset in [0, 3] {
            let curve =
                CubicSegment::new_bezier([p[offset], p[offset + 1], p[offset + 2], p[offset + 3]]);
            for step in 0..=12 {
                let point = curve.position(step as f32 / 12.0) - size * 0.5;
                points[cursor] = center + Vec2::new(point.x, -point.y);
                cursor += 1;
            }
        }
    }
    points[104] = points[0];
    points
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn group_corners_keep_screen_radius_and_limit_small_group_roundness() {
        let size = Vec2::new(2400.0, 1200.0);
        for scale in [0.1, 0.5, 1.0, 2.0, 8.0] {
            let radius = group_radius(size, scale);
            assert!((radius / scale - 24.0).abs() < 0.001);
            let points = outline_with_radius(Vec2::ZERO, size, radius);
            assert_eq!(points[0], points[104]);
            assert!((points[0].x - (size.x * 0.5 - radius)).abs() < 0.001);
        }
        for size in [Vec2::new(20.0, 80.0), Vec2::new(300.0, 100.0)] {
            let radius = group_radius(size, 100.0);
            assert_eq!(radius, size.min_element() * 0.25);
            assert!(size.min_element() - 2.0 * radius >= size.min_element() * 0.5);
        }
    }

    #[test]
    fn radius_matches_current_godot_text_node_and_keeps_straight_section() {
        for (size, expected) in [
            (Vec2::new(200.0, 100.0), 24),
            (Vec2::new(200.0, 48.0), 22),
            (Vec2::new(31.0, 20.0), 8),
            (Vec2::splat(7.9), 1),
            (Vec2::ONE, 0),
        ] {
            assert_eq!(radius(size), expected);
            if size.min_element() >= 4.0 {
                assert!(size.min_element() - 2.0 * radius(size) as f32 >= 4.0);
            }
        }
    }

    #[test]
    fn outline_is_closed_and_shares_svg_corner_profile() {
        let size = Vec2::new(120.0, 80.0);
        let center = Vec2::new(7.0, -9.0);
        let points = outline(center, size);
        assert_eq!(points[0], points[104]);
        let midpoint = corner(size, 24.0, 0)[3] - size * 0.5;
        assert!(points[12].distance(center + Vec2::new(midpoint.x, -midpoint.y)) < 0.0001);
        for point in points {
            assert!(
                (point - center)
                    .abs()
                    .cmple(size * 0.5 + Vec2::splat(0.0001))
                    .all()
            );
        }
        let svg = path(size, 24.0, 1.0);
        assert_eq!(svg.matches('C').count(), 8);
        assert!(svg.starts_with("M 97 1 "));
        assert!(svg.ends_with('Z'));
    }
}
