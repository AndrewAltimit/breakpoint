#version 300 es
precision highp float;

uniform vec4 u_color;
uniform float u_intensity;

in vec2 v_uv;
in float v_fog_factor;

out vec4 frag_color;

void main() {
    // Body at ~55% brightness — visible on black, top edge still pops
    float body = 0.55;
    // Top 10% transitions to full brightness (bright top-edge highlight)
    float top_edge = smoothstep(0.90, 0.95, v_uv.y);
    float brightness = mix(body, 1.0, top_edge) * u_intensity;

    frag_color = vec4(u_color.rgb * brightness, u_color.a);
    // Slight fade at very bottom
    frag_color.a *= smoothstep(0.0, 0.05, v_uv.y);
    // Apply fog
    frag_color.rgb = mix(frag_color.rgb, vec3(0.0), v_fog_factor);
}
