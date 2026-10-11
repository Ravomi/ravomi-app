#import bevy_sprite::mesh2d_vertex_output::VertexOutput

struct Region {
    bounds: vec4<f32>, // center.xy, half size.xy in world space
    radius: f32,
    parent: u32,
    padding: vec2<f32>,
}
struct Settings {
    head: u32,
    pixel_range: f32,
    padding: vec2<f32>,
}
@group(#{MATERIAL_BIND_GROUP}) @binding(0) var content: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var content_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var mask: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(3) var mask_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(4) var<storage, read> regions: array<Region>;
@group(#{MATERIAL_BIND_GROUP}) @binding(5) var<uniform> settings: Settings;

@fragment
fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {
    let sample = textureSample(content, content_sampler, mesh.uv);
    var color = sample * mesh.color;
    if settings.pixel_range > 0.0 {
        let median = max(min(sample.r, sample.g), min(max(sample.r, sample.g), sample.b));
        let unit_range = vec2<f32>(settings.pixel_range) / vec2<f32>(textureDimensions(content));
        let screen_size = 1.0 / max(fwidth(mesh.uv), vec2<f32>(0.000001));
        let screen_range = max(0.5 * dot(unit_range, screen_size), 1.0);
        color = vec4<f32>(mesh.color.rgb, mesh.color.a * clamp((median - 0.5) * screen_range + 0.5, 0.0, 1.0));
    }
    var head = settings.head;
    // Parent indices always decrease: malformed document cycles cannot hang a GPU.
    loop {
        if head == 0u || head >= arrayLength(&regions) { break; }
        let region = regions[head];
        let inset = region.bounds.zw - abs(mesh.world_position.xy - region.bounds.xy);
        if min(inset.x, inset.y) < 0.0 { discard; }
        if region.radius > 0.0 {
            // Sample the very same continuous-corner mask as the parent fill.
            // Its logical extent is 54, with one transparent unit of padding.
            let uv = (vec2<f32>(1.0) + min(inset * (24.0 / region.radius), vec2<f32>(25.0))) / 54.0;
            color.a *= textureSampleLevel(mask, mask_sampler, uv, 0.0).a;
        }
        if region.parent >= head { break; }
        head = region.parent;
    }
    return color;
}
