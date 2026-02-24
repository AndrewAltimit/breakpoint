#version 300 es
precision highp float;

in vec2 v_uv;

uniform sampler2D u_texture;
uniform vec2 u_uv_offset;  // horizontal scroll offset (x) and vertical base (y)
uniform vec2 u_uv_scale;   // UV scale for the layer sub-rect
uniform vec4 u_tint;
uniform float u_time;
uniform float u_intensity;  // sway amplitude (reused uniform, 0.0 = no sway)
uniform float u_speed;      // crossfade alpha (reused uniform, 1.0 = fully visible)

out vec4 frag_color;

void main() {
    // Apply sway (horizontal oscillation for banners/trees)
    float sway = sin(v_uv.y * 3.14159 + u_time * 1.5) * u_intensity * 0.01;

    // Apply scroll offset and wrap horizontally via fract()
    float u = fract(v_uv.x * u_uv_scale.x + u_uv_offset.x + sway);
    float v = (1.0 - v_uv.y) * u_uv_scale.y + u_uv_offset.y;

    vec4 texel = texture(u_texture, vec2(u, v));

    // Apply crossfade alpha (for background transitions)
    float crossfade = u_speed; // 0.0 = invisible, 1.0 = fully visible
    vec4 color = texel * u_tint;
    color.a *= crossfade;

    if (color.a < 0.01) {
        discard;
    }
    frag_color = color;
}
