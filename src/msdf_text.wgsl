// Adapted from StarArawn/kayak_ui sample_quad.wgsl at 7c5c6f77.
// MIT OR Apache-2.0; see vendor/kayak_font_msdf/UPSTREAM.md.
#import bevy_sprite::mesh2d_vertex_output::VertexOutput

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var font_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var font_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var<uniform> pixel_range: f32;

fn median_three(v: vec3<f32>) -> f32 {
    return max(min(v.x, v.y), min(max(v.x, v.y), v.z));
}

@fragment
fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {
    let distance = median_three(textureSample(font_texture, font_sampler, mesh.uv).rgb) - 0.5;
    // Derivatives follow the camera and DPI without changing the source font size.
    let unit_range = vec2<f32>(pixel_range) / vec2<f32>(textureDimensions(font_texture));
    let screen_size = 1.0 / max(fwidth(mesh.uv), vec2<f32>(0.000001));
    let screen_range = max(0.5 * dot(unit_range, screen_size), 1.0);
    let coverage = clamp(distance * screen_range + 0.5, 0.0, 1.0);
    return vec4<f32>(mesh.color.rgb, mesh.color.a * coverage);
}

