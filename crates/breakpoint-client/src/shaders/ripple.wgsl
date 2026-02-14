#import bevy_pbr::forward_io::VertexOutput

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> material_color: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var<uniform> material_params: vec4<f32>;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let color = material_color;
    let time = material_params.x;
    let ring_count = material_params.y;
    let speed = material_params.z;

    // Concentric animated rings from UV center
#ifdef VERTEX_UVS_A
    let uv = in.uv;
#else
    let uv = vec2<f32>(0.5, 0.5);
#endif

    let centered = (uv - vec2<f32>(0.5, 0.5)) * 2.0;
    let dist = length(centered);

    // Circular edge mask
    let edge_mask = 1.0 - smoothstep(0.7, 1.0, dist);

    // Animated concentric rings
    let rings = sin(dist * ring_count - time * speed) * 0.5 + 0.5;

    // Subtle opacity pulse
    let pulse = 0.6 + 0.4 * sin(time * 1.5);

    let final_alpha = color.a * edge_mask * (0.3 + rings * 0.7) * pulse;
    let final_color = color.rgb * (0.8 + rings * 0.2);

    return vec4<f32>(final_color, final_alpha);
}
