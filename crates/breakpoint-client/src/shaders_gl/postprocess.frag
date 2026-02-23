#version 300 es
precision highp float;

in vec2 v_uv;

uniform sampler2D u_scene;
uniform vec2 u_resolution;
uniform float u_time;
uniform float u_scanline_intensity;      // 0.0-1.0
uniform float u_bloom_intensity;         // 0.0-1.0
uniform float u_vignette_intensity;      // 0.0-1.0
uniform float u_crt_curvature;           // 0.0-1.0
uniform vec3 u_grade_shadows;            // shadow color tint
uniform vec3 u_grade_highlights;         // highlight color tint
uniform float u_grade_contrast;          // contrast (1.0 = neutral)
uniform float u_saturation;              // saturation (1.0 = neutral)
uniform float u_chromatic_aberration;    // pixel offset (0.0 = off)
uniform float u_film_grain;             // grain intensity (0.0 = off)

out vec4 frag_color;

// Simple hash for film grain
float hash(vec2 p) {
    return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453);
}

void main() {
    vec2 uv = v_uv;

    // CRT barrel distortion
    if (u_crt_curvature > 0.01) {
        vec2 centered = uv - 0.5;
        float dist2 = dot(centered, centered);
        uv = 0.5 + centered * (1.0 + dist2 * u_crt_curvature * 0.5);
        if (uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0) {
            frag_color = vec4(0.0, 0.0, 0.0, 1.0);
            return;
        }
    }

    // ── Chromatic aberration (damage effect) ──
    vec3 color;
    if (u_chromatic_aberration > 0.01) {
        float ca = u_chromatic_aberration / u_resolution.x;
        color.r = texture(u_scene, uv + vec2(ca, 0.0)).r;
        color.g = texture(u_scene, uv).g;
        color.b = texture(u_scene, uv - vec2(ca, 0.0)).b;
    } else {
        color = texture(u_scene, uv).rgb;
    }

    // ── Bloom: 9-tap Gaussian-approximation separable sampling ──
    if (u_bloom_intensity > 0.01) {
        vec2 texel = 1.0 / u_resolution;
        vec3 bloom = vec3(0.0);
        // Gaussian weights approximation for 9 taps
        float weights[5] = float[](0.227, 0.194, 0.122, 0.054, 0.016);
        // Horizontal + vertical combined (cross pattern)
        bloom += texture(u_scene, uv).rgb * weights[0];
        for (int i = 1; i < 5; i++) {
            float o = float(i) * 1.5; // spread samples wider
            bloom += texture(u_scene, uv + vec2(o * texel.x, 0.0)).rgb * weights[i];
            bloom += texture(u_scene, uv - vec2(o * texel.x, 0.0)).rgb * weights[i];
            bloom += texture(u_scene, uv + vec2(0.0, o * texel.y)).rgb * weights[i];
            bloom += texture(u_scene, uv - vec2(0.0, o * texel.y)).rgb * weights[i];
        }
        bloom /= 2.0; // normalize (center counted once, each pair counted twice)
        // Luminance threshold: only bloom bright areas
        float lum = dot(bloom, vec3(0.299, 0.587, 0.114));
        bloom *= smoothstep(0.4, 0.8, lum);
        color += bloom * u_bloom_intensity;
    }

    // ── Color grading ──
    // Split toning: tint shadows and highlights separately
    float luminance = dot(color, vec3(0.299, 0.587, 0.114));
    vec3 shadow_blend = mix(vec3(1.0), u_grade_shadows, 1.0 - luminance);
    vec3 highlight_blend = mix(vec3(1.0), u_grade_highlights, luminance);
    color *= shadow_blend * highlight_blend;

    // Contrast adjustment (pivot at 0.5)
    if (abs(u_grade_contrast - 1.0) > 0.01) {
        color = (color - 0.5) * u_grade_contrast + 0.5;
    }

    // Saturation adjustment
    if (abs(u_saturation - 1.0) > 0.01) {
        float gray = dot(color, vec3(0.299, 0.587, 0.114));
        color = mix(vec3(gray), color, u_saturation);
    }

    // ── CRT scanlines ──
    if (u_scanline_intensity > 0.01) {
        float scanline = 0.8 + 0.2 * sin(gl_FragCoord.y * 3.14159);
        color *= mix(1.0, scanline, u_scanline_intensity);
        // RGB sub-pixel shift
        float shift = 0.5 / u_resolution.x * u_scanline_intensity;
        color.r = texture(u_scene, uv + vec2(shift, 0.0)).r;
        color.b = texture(u_scene, uv - vec2(shift, 0.0)).b;
    }

    // ── Film grain ──
    if (u_film_grain > 0.01) {
        float grain = hash(gl_FragCoord.xy + fract(u_time * 100.0)) * 2.0 - 1.0;
        color += vec3(grain * u_film_grain * 0.1);
    }

    // ── Vignette ──
    if (u_vignette_intensity > 0.01) {
        vec2 centered = uv - 0.5;
        float vignette = 1.0 - dot(centered, centered) * 2.0;
        vignette = clamp(vignette, 0.0, 1.0);
        color *= mix(1.0, vignette, u_vignette_intensity);
    }

    // Clamp to valid range
    color = clamp(color, vec3(0.0), vec3(1.0));

    frag_color = vec4(color, 1.0);
}
