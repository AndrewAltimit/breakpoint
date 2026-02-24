#version 300 es
precision highp float;

in vec2 v_uv;

uniform float u_rotation; // current rotation angle
uniform float u_pulse;    // 0.0-1.0 pulse intensity
uniform vec4 u_color;
uniform float u_time;

out vec4 frag_color;

void main() {
    // Center UV to [-1, 1]
    vec2 uv = v_uv * 2.0 - 1.0;

    // Rotate UVs
    float c = cos(u_rotation);
    float s = sin(u_rotation);
    vec2 ruv = vec2(uv.x * c - uv.y * s, uv.x * s + uv.y * c);

    float r = length(ruv);
    float angle = atan(ruv.y, ruv.x);

    // Discard outside circle
    if (r > 1.0) {
        discard;
    }

    // Concentric rings
    float ring1 = smoothstep(0.02, 0.0, abs(r - 0.9));
    float ring2 = smoothstep(0.02, 0.0, abs(r - 0.7));
    float ring3 = smoothstep(0.015, 0.0, abs(r - 0.45));

    // Angular rune modulation (8 segments)
    float runes = step(0.5, fract(angle * 8.0 / 6.28318 + u_time * 0.5));
    float rune_ring = runes * smoothstep(0.04, 0.0, abs(r - 0.57)) * 0.7;

    // Inner pentagram-like pattern (5-pointed star)
    float star_angle = mod(angle + u_rotation * 0.5, 6.28318);
    float star = abs(sin(star_angle * 2.5)) * step(r, 0.35) * step(0.1, r);
    float star_line = smoothstep(0.05, 0.0, abs(star - r * 1.5));

    // Glow toward center
    float center_glow = smoothstep(0.3, 0.0, r) * 0.3;

    // Pulse: brighten everything rhythmically
    float pulse_mod = 1.0 + u_pulse * 0.4 * sin(u_time * 4.0);

    // Composite
    float alpha = (ring1 + ring2 + ring3 + rune_ring + star_line * 0.5 + center_glow) * pulse_mod;
    alpha = clamp(alpha, 0.0, 1.0) * u_color.a;

    if (alpha < 0.01) {
        discard;
    }

    vec3 color = u_color.rgb * (1.0 + center_glow * 2.0);
    frag_color = vec4(color, alpha);
}
