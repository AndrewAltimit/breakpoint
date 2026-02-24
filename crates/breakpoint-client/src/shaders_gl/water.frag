#version 300 es
precision highp float;

in vec2 v_uv;
in vec3 v_world_pos;

uniform float u_time;
uniform vec4 u_color;       // water base color (RGBA)
uniform float u_depth;      // visual depth: 0.0=surface tile, 1.0=deep tile
uniform float u_wave_speed; // wave animation speed

out vec4 frag_color;

// ── Noise functions ──────────────────────────────────────────────

float hash(vec2 p) {
    return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453);
}

float value_noise(vec2 p) {
    vec2 i = floor(p);
    vec2 f = fract(p);
    f = f * f * (3.0 - 2.0 * f); // smoothstep interpolation
    float a = hash(i);
    float b = hash(i + vec2(1.0, 0.0));
    float c = hash(i + vec2(0.0, 1.0));
    float d = hash(i + vec2(1.0, 1.0));
    return mix(mix(a, b, f.x), mix(c, d, f.x), f.y);
}

// ── Main ─────────────────────────────────────────────────────────

void main() {
    float wx = v_world_pos.x;
    float wy = v_world_pos.y;
    float t = u_time * u_wave_speed;

    // Normalized vertical position: 0 = bottom of tile, 1 = top of tile
    float surface_line = 1.0 - v_uv.y;

    // ── Multi-frequency surface waves ──
    float wave = sin(wx * 3.0 + t * 1.0) * 0.04
               + sin(wx * 7.0 - t * 1.3) * 0.02
               + sin(wx * 13.0 + t * 2.1) * 0.01;

    // Discard above surface wave (only for surface tiles)
    if (u_depth < 0.5 && surface_line > 0.92 + wave) {
        discard;
    }

    // ── Depth-dependent color absorption ──
    // Surface tiles: depth factor based on v_uv.y
    // Deep tiles: everything is fully deep
    float pixel_depth = u_depth < 0.5 ? v_uv.y : 1.0;

    // Blue-shift absorption: reds/greens absorbed faster than blues
    vec3 base = u_color.rgb;
    base.r *= mix(1.0, 0.5, pixel_depth);
    base.g *= mix(1.0, 0.65, pixel_depth);
    base.b *= mix(1.0, 0.9, pixel_depth);
    // Overall darkening with depth
    base *= mix(1.0, 0.6, pixel_depth);

    // ── Dual-layer scrolling caustics ──
    vec2 caustic_uv1 = vec2(wx * 2.5 + t * 0.2, wy * 2.5 + t * 0.15);
    vec2 caustic_uv2 = vec2(wx * 3.2 - t * 0.18, wy * 2.8 + t * 0.25);
    float c1 = value_noise(caustic_uv1);
    float c2 = value_noise(caustic_uv2);
    // Combine and sharpen with smoothstep
    float caustic = smoothstep(0.35, 0.7, c1 * c2 * 2.0);
    // Caustics are brighter near surface, dimmer deep
    float caustic_strength = mix(0.35, 0.1, pixel_depth);
    base += vec3(caustic * caustic_strength * 0.4,
                 caustic * caustic_strength * 0.6,
                 caustic * caustic_strength);

    // ── Surface foam line (only on surface tiles) ──
    if (u_depth < 0.5) {
        float foam_noise = value_noise(vec2(wx * 6.0 + t * 0.5, 0.0)) * 0.05;
        float foam_line = smoothstep(0.05, 0.0, abs(surface_line - 0.88 - wave - foam_noise));
        // Secondary thin foam line
        float foam2 = smoothstep(0.03, 0.0, abs(surface_line - 0.82 - wave * 0.5)) * 0.4;
        base += vec3(0.7, 0.8, 0.9) * (foam_line + foam2);
    }

    // ── Surface highlight with Fresnel approximation ──
    if (u_depth < 0.5) {
        // Bright specular-like highlight along wave peaks
        float highlight = smoothstep(0.02, 0.0, abs(surface_line - 0.90 - wave));
        // Fresnel: brighter when viewed at grazing angle (approximated by surface proximity)
        float fresnel = pow(1.0 - pixel_depth, 3.0) * 0.5;
        base += vec3(0.4, 0.5, 0.6) * (highlight + fresnel * 0.3);
    }

    // ── Subtle animated shimmer ──
    float shimmer = sin(wx * 20.0 + wy * 15.0 + t * 3.0) * 0.02;
    base += vec3(shimmer) * (1.0 - pixel_depth);

    // ── Depth-based opacity ──
    // Surface: more transparent (0.45), deep: more opaque (0.8)
    float alpha = mix(0.45, 0.8, pixel_depth) * u_color.a;

    frag_color = vec4(base, alpha);
}
