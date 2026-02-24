#version 300 es
precision highp float;

// Per-vertex attributes (pre-computed on CPU)
layout(location = 0) in vec3 a_position;   // world-space position
layout(location = 1) in vec2 a_uv;         // atlas UV (pre-computed)
layout(location = 2) in vec4 a_tint;       // per-sprite tint

uniform mat4 u_vp;  // view-projection matrix

out vec2 v_uv;
out vec3 v_world_pos;
out vec4 v_tint;

void main() {
    v_uv = a_uv;
    v_world_pos = a_position;
    v_tint = a_tint;
    gl_Position = u_vp * vec4(a_position, 1.0);
}
