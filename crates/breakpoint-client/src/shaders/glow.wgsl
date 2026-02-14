#import bevy_pbr::forward_io::VertexOutput

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> material_color: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var<uniform> material_params: vec4<f32>;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let color = material_color;
    let intensity = material_params.x;
    let alpha = material_params.y;

    // UV-based soft edge falloff: bright center fading to transparent edges
#ifdef VERTEX_UVS_A
    let uv = in.uv;
#else
    let uv = vec2<f32>(0.5, 0.5);
#endif

    let dist = abs(uv.y - 0.5) * 2.0;
    let falloff = exp(-dist * dist * 4.0);

    let final_color = color.rgb * intensity * falloff;
    let final_alpha = alpha * falloff;

    return vec4<f32>(final_color, final_alpha);
}
