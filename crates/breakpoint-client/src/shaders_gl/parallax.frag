#version 300 es
precision highp float;

in vec2 v_uv;

uniform sampler2D u_texture;
uniform vec2 u_uv_offset;  // horizontal scroll offset (x) and vertical base (y)
uniform vec2 u_uv_scale;   // UV scale for the layer sub-rect
uniform vec4 u_tint;

out vec4 frag_color;

void main() {
    // Apply scroll offset and wrap horizontally via fract()
    float u = fract(v_uv.x * u_uv_scale.x + u_uv_offset.x);
    float v = v_uv.y * u_uv_scale.y + u_uv_offset.y;

    vec4 texel = texture(u_texture, vec2(u, v));
    vec4 color = texel * u_tint;
    if (color.a < 0.01) {
        discard;
    }
    frag_color = color;
}
