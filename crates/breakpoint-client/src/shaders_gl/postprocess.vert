#version 300 es
precision highp float;

layout(location = 0) in vec3 a_position;
layout(location = 1) in vec3 a_normal;
layout(location = 2) in vec2 a_uv;

out vec2 v_uv;

void main() {
    // Fullscreen quad: position [-0.5, 0.5] -> NDC [-1, 1]
    gl_Position = vec4(a_position.xy * 2.0, 0.0, 1.0);
    v_uv = a_uv;
}
