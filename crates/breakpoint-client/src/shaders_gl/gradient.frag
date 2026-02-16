#version 300 es
precision highp float;

uniform vec4 u_color_start;
uniform vec4 u_color_end;

in vec2 v_uv;

out vec4 frag_color;

void main() {
    frag_color = mix(u_color_start, u_color_end, v_uv.y);
}
