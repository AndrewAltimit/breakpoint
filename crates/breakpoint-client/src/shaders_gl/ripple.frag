#version 300 es
precision highp float;

uniform vec4 u_color;
uniform float u_time;
uniform float u_ring_count;
uniform float u_speed;

in vec2 v_uv;

out vec4 frag_color;

void main() {
    vec2 center = v_uv - 0.5;
    float dist = length(center) * 2.0;
    float ring = sin(dist * u_ring_count - u_time * u_speed) * 0.5 + 0.5;
    float edge = 1.0 - smoothstep(0.8, 1.0, dist);
    frag_color = u_color * vec4(vec3(ring), edge);
}
