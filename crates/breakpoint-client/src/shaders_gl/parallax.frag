#version 300 es
precision highp float;

in vec2 v_uv;

uniform sampler2D u_texture;
uniform vec2 u_uv_offset;  // horizontal scroll offset (x) and vertical base (y)
uniform vec2 u_uv_scale;   // UV scale for the layer sub-rect
uniform vec4 u_tint;
uniform float u_time;
uniform float u_intensity;  // sway amplitude: 0.0 = static, <1.0 = gentle sway, >1.0 = water mode
uniform float u_speed;      // crossfade alpha (reused uniform, 1.0 = fully visible)

out vec4 frag_color;

void main() {
    float u_coord;
    float v_coord;

    if (u_intensity > 1.0) {
        // Water wave animation mode — animated surface with caustics
        float wave_amp = (u_intensity - 1.0) * 0.015;
        float wave1 = sin(v_uv.x * 8.0 + u_time * 1.8) * wave_amp;
        float wave2 = sin(v_uv.x * 13.0 - u_time * 2.4 + 1.5) * wave_amp * 0.6;
        float wave3 = sin(v_uv.y * 6.0 + u_time * 1.2) * wave_amp * 0.4;
        float wave_offset = wave1 + wave2 + wave3;

        u_coord = fract(v_uv.x * u_uv_scale.x + u_uv_offset.x + wave_offset);
        v_coord = (1.0 - v_uv.y) * u_uv_scale.y + u_uv_offset.y;
        // Vertical ripple
        v_coord += sin(v_uv.x * 10.0 + u_time * 1.5) * 0.003;

        vec4 texel = texture(u_texture, vec2(u_coord, v_coord));

        // Caustic shimmer — bright dancing highlights
        float caustic = sin(v_uv.x * 20.0 + u_time * 3.0) *
                        sin(v_uv.y * 15.0 - u_time * 2.0) * 0.5 + 0.5;
        caustic = pow(caustic, 4.0) * 0.25;

        // Reflection sparkle at wave crests
        float sparkle = pow(max(0.0, sin(v_uv.x * 30.0 + u_time * 4.0)), 16.0) * 0.15;

        vec4 color = texel * u_tint;
        // Add caustic and sparkle as additive light
        color.rgb += vec3(caustic * 0.3, caustic * 0.5, caustic * 0.7);
        color.rgb += vec3(sparkle * 0.8, sparkle * 0.9, sparkle * 1.0);
        // Gentle alpha pulse for surface shimmer
        color.a *= u_speed * (0.85 + 0.15 * sin(u_time * 0.8 + v_uv.x * 5.0));

        if (color.a < 0.01) {
            discard;
        }
        frag_color = color;
    } else {
        // Standard parallax layer with optional sway
        float sway = sin(v_uv.y * 3.14159 + u_time * 1.5) * u_intensity * 0.01;

        u_coord = fract(v_uv.x * u_uv_scale.x + u_uv_offset.x + sway);
        v_coord = (1.0 - v_uv.y) * u_uv_scale.y + u_uv_offset.y;

        vec4 texel = texture(u_texture, vec2(u_coord, v_coord));

        float crossfade = u_speed;
        vec4 color = texel * u_tint;
        color.a *= crossfade;

        if (color.a < 0.01) {
            discard;
        }
        frag_color = color;
    }
}
