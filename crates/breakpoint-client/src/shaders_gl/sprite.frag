#version 300 es
precision highp float;

in vec2 v_uv;

uniform sampler2D u_texture;
uniform vec4 u_sprite_rect; // atlas sub-region: (u0, v0, u1, v1)
uniform vec4 u_tint;
uniform float u_flip_x;

out vec4 frag_color;

void main() {
    // Map quad UV [0,1] to atlas sub-rect
    float u = mix(u_sprite_rect.x, u_sprite_rect.z, v_uv.x);
    // Flip horizontally if u_flip_x > 0.5
    if (u_flip_x > 0.5) {
        u = u_sprite_rect.x + u_sprite_rect.z - u;
    }
    float v = mix(u_sprite_rect.y, u_sprite_rect.w, v_uv.y);
    vec4 texel = texture(u_texture, vec2(u, v));
    vec4 color = texel * u_tint;
    if (color.a < 0.01) {
        discard;
    }
    frag_color = color;
}
