#![cfg(feature = "viewer")]

use project_graph_bevy_canvas::{GraphEdge, document::Document};

#[test]
#[ignore = "requires a desktop display, Vulkan and the configured CJK font"]
fn nested_contents_are_clipped_to_the_live_group_outline() {
    for light in [false, true] {
        for (corner, flip) in [[false, false], [true, false], [false, true], [true, true]]
            .into_iter()
            .enumerate()
        {
            check_nested_corner(light, flip, ["1", "1.5", "2", "1.5"][corner]);
        }
    }
}

fn check_nested_corner(light: bool, flip: [bool; 2], ui_scale: &str) {
    let directory = tempfile::tempdir().unwrap();
    let mut doc = Document::blank().unwrap();
    let a = doc.create_text_node([1960.0, 200.0]).unwrap();
    let b = doc.create_text_node([2830.0, 340.0]).unwrap();
    let inner = doc
        .create_group(&std::collections::HashSet::from([a, b]))
        .unwrap();
    doc.nodes[inner].text.clear();
    doc.nodes[inner].color = [1.0, 0.0, 0.0, 1.0];
    let other = doc.create_text_node([300.0, 2100.0]).unwrap();
    let outer = doc
        .create_group(&std::collections::HashSet::from([inner, other]))
        .unwrap();
    doc.nodes[outer].text.clear();
    doc.nodes[outer].color = [0.0, 1.0, 0.0, 1.0];
    doc.create_text_node([30000.0, 0.0]).unwrap();
    for node in &mut doc.nodes {
        if flip[0] {
            node.rect[0] = -node.rect[0] - node.rect[2];
        }
        if flip[1] {
            node.rect[1] = -node.rect[1] - node.rect[3];
        }
    }
    let name = format!(
        "nested-clip-{}-{}{}-{ui_scale}",
        if light { "light" } else { "dark" },
        u8::from(flip[0]),
        u8::from(flip[1])
    );
    let input = directory.path().join("nested-clip.prg");
    let output = directory.path().join("nested-clip.png");
    doc.save(&input).unwrap();
    let binary = std::env::var_os("GRAPLU_RENDER_BINARY")
        .unwrap_or_else(|| env!("CARGO_BIN_EXE_project-graph-bevy-canvas").into());
    let mut command = std::process::Command::new(binary);
    command
        .arg(&input)
        .args([
            "--preview",
            "--native-scale",
            "--ui-scale",
            ui_scale,
            "--screenshot",
        ])
        .arg(&output);
    if light {
        command.arg("--light");
    }
    let result = command.output().unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    if let Some(path) = std::env::var_os("GRAPLU_CLIP_ARTIFACTS") {
        let path = std::path::PathBuf::from(path);
        std::fs::create_dir_all(&path).unwrap();
        std::fs::copy(&input, path.join(format!("{name}.prg"))).unwrap();
        std::fs::copy(&output, path.join(format!("{name}.png"))).unwrap();
    }
    let image = image::open(output).unwrap().into_rgba8();
    let green = |p: &image::Rgba<u8>| p[1] > 230 && p[0] < 30 && p[2] < 30;
    let red = |p: &image::Rgba<u8>| p[0] > 230 && p[1] < 30 && p[2] < 30;
    let mut bounds = [u32::MAX, u32::MAX, 0, 0];
    for (x, y, _) in image.enumerate_pixels().filter(|(_, _, p)| green(p)) {
        bounds = [
            bounds[0].min(x),
            bounds[1].min(y),
            bounds[2].max(x),
            bounds[3].max(y),
        ];
    }
    let [left, top, right, bottom] = bounds;
    assert!(left < right && top < bottom, "missing parent group");
    // At this distance the parent radius is limited to a quarter of its short
    // side. The square nearest the corner is entirely outside its cubic curve.
    let radius = (right - left).min(bottom - top) as f32 * 0.25;
    // Fully saturated fill pixels start inside the antialiased boundary; leave
    // two physical pixels for locating the outline from that color threshold.
    let outside = (radius * 0.19 - 2.0).max(0.0).floor() as u32;
    assert!(
        outside >= 3,
        "fixture too small for a meaningful pixel check"
    );
    let mut inside = 0;
    for (x, y, _) in image.enumerate_pixels().filter(|(_, _, p)| red(p)) {
        let dx = if flip[0] {
            x.saturating_sub(left)
        } else {
            right.saturating_sub(x)
        };
        let dy = if flip[1] {
            bottom.saturating_sub(y)
        } else {
            y.saturating_sub(top)
        };
        assert!(
            dx > outside || dy > outside,
            "{name}: nested fill leaks beyond parent corner at {x},{y}; bounds={bounds:?}, radius={radius}"
        );
        inside += 1;
    }
    assert!(
        inside > 30,
        "clipping must preserve the visible child interior"
    );
}

#[test]
#[ignore = "requires a desktop display, Vulkan and the configured CJK font"]
fn expanded_group_corners_stay_visible_in_near_and_distant_desktop_views() {
    let directory = tempfile::tempdir().unwrap();
    let artifacts = std::env::var_os("GRAPLU_GROUP_CORNER_ARTIFACTS").map(std::path::PathBuf::from);
    if let Some(path) = &artifacts {
        std::fs::create_dir_all(path).unwrap();
    }
    for distant in [false, true] {
        let mut doc = Document::blank().unwrap();
        doc.create_text_node([-300.0, -200.0]).unwrap();
        doc.create_text_node([300.0, 200.0]).unwrap();
        let group = doc
            .create_group(&std::collections::HashSet::from([0, 1]))
            .unwrap();
        doc.nodes[group].text.clear(); // Inspect the actual outline, without overview title masking.
        doc.nodes[group].color = [1.0, 0.0, 0.0, 1.0];
        if distant {
            doc.create_text_node([6000.0, 0.0]).unwrap();
        }
        let name = if distant { "far" } else { "near" };
        let input = directory.path().join(format!("group-{name}.prg"));
        doc.save(&input).unwrap();
        if let Some(path) = &artifacts {
            std::fs::copy(&input, path.join(input.file_name().unwrap())).unwrap();
        }
        for light in [false, true] {
            for scale in ["1", "1.5", "2"] {
                let name = format!(
                    "group-{name}-{}-{scale}",
                    if light { "light" } else { "dark" }
                );
                let output = directory.path().join(format!("{name}.png"));
                let binary = std::env::var_os("GRAPLU_RENDER_BINARY")
                    .unwrap_or_else(|| env!("CARGO_BIN_EXE_project-graph-bevy-canvas").into());
                let mut command = std::process::Command::new(binary);
                command
                    .arg(&input)
                    .args([
                        "--preview",
                        "--native-scale",
                        "--ui-scale",
                        scale,
                        "--screenshot",
                    ])
                    .arg(&output);
                if light {
                    command.arg("--light");
                }
                let result = command.output().unwrap();
                assert!(
                    result.status.success(),
                    "{}",
                    String::from_utf8_lossy(&result.stderr)
                );
                if let Some(path) = &artifacts {
                    std::fs::write(path.join(format!("{name}.log")), &result.stderr).unwrap();
                }
                assert!(
                    output.is_file(),
                    "{name}: preview exited without saving its screenshot: {}",
                    String::from_utf8_lossy(&result.stderr)
                );
                if let Some(path) = &artifacts {
                    std::fs::copy(&output, path.join(output.file_name().unwrap())).unwrap();
                }
                let image = image::open(&output).unwrap().into_rgba8();
                let red =
                    |pixel: &image::Rgba<u8>| pixel[0] > 230 && pixel[1] < 30 && pixel[2] < 30;
                let mut bounds = [u32::MAX, u32::MAX, 0, 0];
                let mut count = 0;
                for (x, y, pixel) in image.enumerate_pixels() {
                    if red(pixel) {
                        bounds = [
                            bounds[0].min(x),
                            bounds[1].min(y),
                            bounds[2].max(x),
                            bounds[3].max(y),
                        ];
                        count += 1;
                    }
                }
                assert!(count > 500, "{name}: missing red group fill");
                let [left, top, right, bottom] = bounds;
                for (x, y) in [
                    (left + 2, top + 2),
                    (right - 2, top + 2),
                    (left + 2, bottom - 2),
                    (right - 2, bottom - 2),
                ] {
                    assert!(
                        !red(image.get_pixel(x, y)),
                        "{name}: corner disappeared at {x},{y}"
                    );
                }
                assert!(
                    red(image.get_pixel((left + right) / 2, (top + bottom) / 2)),
                    "{name}: group interior must remain filled"
                );
            }
        }
    }
}

#[test]
#[ignore = "requires a desktop display, Vulkan and the configured CJK font"]
fn nested_group_fill_stays_above_parent_at_different_scales() {
    let directory = tempfile::tempdir().unwrap();
    let mut doc = Document::blank().unwrap();
    doc.create_text_node([-200.0, -200.0]).unwrap();
    doc.create_text_node([200.0, 200.0]).unwrap();
    let inner = doc
        .create_group(&std::collections::HashSet::from([0, 1]))
        .unwrap();
    let outer = doc
        .create_group(&std::collections::HashSet::from([inner]))
        .unwrap();
    doc.nodes[inner].color = [1.0, 0.0, 0.0, 1.0];
    doc.nodes[outer].color = [0.0, 0.0, 1.0, 1.0];
    doc.nodes[inner].id = "a-inner".into();
    doc.nodes[outer].id = "z-outer".into();
    doc.nodes[outer].children = vec!["a-inner".into()];
    let input = directory.path().join("nested.prg");
    doc.save(&input).unwrap();
    for width in [256, 320, 512, 640] {
        let output = directory.path().join(format!("nested-{width}.png"));
        let binary = std::env::var_os("GRAPLU_RENDER_BINARY")
            .unwrap_or_else(|| env!("CARGO_BIN_EXE_project-graph-bevy-canvas").into());
        let result = std::process::Command::new(binary)
            .arg(&input)
            .args(["--render-png"])
            .arg(&output)
            .args(["--export-rect", "-400,-400,800,800"])
            .args(["--export-width", &width.to_string()])
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let image = image::open(&output).unwrap().into_rgba8();
        let pixel = image.get_pixel(image.width() / 2, image.height() / 2);
        assert!(
            pixel[0] > 240 && pixel[2] < 10,
            "inner group was covered at width {width}: {pixel:?}"
        );
    }
}

#[test]
#[ignore = "requires a desktop display, Vulkan and the configured CJK font"]
fn offscreen_export_contains_arc_outside_node_bounds_without_clipped_pixels() {
    let directory = tempfile::tempdir().unwrap();
    let mut doc = Document::blank().unwrap();
    doc.create_text_node([0.0, 0.0]).unwrap();
    doc.create_text_node([250.0, 0.0]).unwrap();
    doc.edges.push(GraphEdge {
        id: "large-arc".into(),
        kind: "ArcEdge".into(),
        members: vec![0, 1],
        rates: vec![[0.5, 0.5]; 2],
        arc_offset: 600.0,
        text: "越界弧线标签".into(),
        color: [1.0, 0.0, 0.0, 1.0],
        ..Default::default()
    });
    doc.counts.insert("ArcEdge".into(), 1);
    let input = directory.path().join("arc.prg");
    let output = directory.path().join("arc.png");
    doc.save(&input).unwrap();
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_project-graph-bevy-canvas"))
        .arg(&input)
        .args(["--render-png"])
        .arg(&output)
        .args(["--export-width", "600", "--transparent"])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let image = image::open(&output).unwrap().into_rgba8();
    assert_eq!(image.width(), 600);
    assert!(
        image.height() > 500,
        "arc extent must expand beyond the short node strip"
    );
    assert!(
        image
            .pixels()
            .filter(|pixel| pixel[0] > 200 && pixel[1] < 100 && pixel[3] > 20)
            .count()
            > 500
    );
    for x in 0..image.width() {
        assert_eq!(image.get_pixel(x, 0)[3], 0);
        assert_eq!(image.get_pixel(x, image.height() - 1)[3], 0);
    }
    for y in 0..image.height() {
        assert_eq!(image.get_pixel(0, y)[3], 0);
        assert_eq!(image.get_pixel(image.width() - 1, y)[3], 0);
    }
    if let Some(path) = std::env::var_os("GRAPLU_EXPORT_ARTIFACT") {
        std::fs::copy(&output, path).unwrap();
    }
}
