#version 300 es
precision highp float;

uniform vec4 u_color;

in float v_fog_factor;

out vec4 frag_color;

void main() {
    frag_color = u_color;
    frag_color.rgb = mix(frag_color.rgb, vec3(0.0), v_fog_factor);
}
