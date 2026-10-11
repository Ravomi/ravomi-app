#import bevy_sprite::mesh2d_vertex_output::VertexOutput
@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> pattern: u32;
@fragment
fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {
    // Measure the transition in screen pixels so zooming does not turn dash
    // ends or the inside of double lines into hard, stair-stepped cutouts.
    let aa = max(fwidth(mesh.uv) * 0.5, vec2<f32>(0.0001));
    var coverage = 1.0;
    // Negative advancement identifies decorations, which always remain solid.
    if mesh.uv.x >= 0.0 {
        if pattern == 1u {
            let distance = 5.0 - abs(fract((mesh.uv.x + 5.0) / 20.0) * 20.0 - 10.0);
            coverage = smoothstep(-aa.x, aa.x, distance);
        }
        if pattern == 2u {
            coverage = smoothstep(-aa.y, aa.y, abs(mesh.uv.y) - 1.0);
        }
    }
    return vec4<f32>(mesh.color.rgb, mesh.color.a * coverage);
}
