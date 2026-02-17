#version 300 es
precision highp float;

uniform vec4 u_color;
uniform float u_intensity;

in vec2 v_uv;
in float v_fog_factor;

out vec4 frag_color;

void main() {
    vec2 center = v_uv - 0.5;
    float dist = length(center) * 2.0;
    float glow = exp(-dist * dist * 4.0) * u_intensity;
    frag_color = vec4(u_color.rgb, u_color.a * glow);
    frag_color.rgb = mix(frag_color.rgb, vec3(0.0), v_fog_factor);
}
