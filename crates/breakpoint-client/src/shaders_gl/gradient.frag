#version 300 es
precision highp float;

uniform vec4 u_color_start;
uniform vec4 u_color_end;

in vec2 v_uv;
in float v_fog_factor;

out vec4 frag_color;

void main() {
    frag_color = mix(u_color_start, u_color_end, v_uv.y);
    frag_color.rgb = mix(frag_color.rgb, vec3(0.0), v_fog_factor);
}
