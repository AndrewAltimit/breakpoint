#version 300 es
precision highp float;

// Per-vertex attributes (pre-computed on CPU)
layout(location = 0) in vec3 a_position;   // world-space position
layout(location = 1) in vec2 a_uv;         // atlas UV (pre-computed)
layout(location = 2) in vec4 a_tint;       // per-sprite tint
layout(location = 3) in float a_outline;   // outline width (0.0 = off)

uniform mat4 u_mvp;  // view-projection matrix

out vec2 v_uv;
out vec3 v_world_pos;
out vec4 v_tint;
out float v_outline;

void main() {
    v_uv = a_uv;
    v_world_pos = a_position;
    v_tint = a_tint;
    v_outline = a_outline;
    gl_Position = u_mvp * vec4(a_position, 1.0);
}
