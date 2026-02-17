#version 300 es
precision highp float;

layout(location = 0) in vec3 a_position;
layout(location = 1) in vec3 a_normal;
layout(location = 2) in vec2 a_uv;

uniform mat4 u_mvp;
uniform mat4 u_model;
uniform vec3 u_camera_pos;
uniform float u_fog_density;

out vec3 v_normal;
out vec2 v_uv;
out vec3 v_world_pos;
out float v_fog_factor;

void main() {
    gl_Position = u_mvp * vec4(a_position, 1.0);
    v_normal = mat3(u_model) * a_normal;
    v_uv = a_uv;
    v_world_pos = (u_model * vec4(a_position, 1.0)).xyz;
    float dist = distance(v_world_pos, u_camera_pos);
    v_fog_factor = clamp((dist - 50.0) / 350.0, 0.0, 1.0) * u_fog_density;
}
