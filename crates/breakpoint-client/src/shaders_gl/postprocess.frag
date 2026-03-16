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
// Genesis-style enhancements
uniform float u_palette_quantize;       // 0.0 = off, 1.0 = full Genesis 9-bit color
uniform float u_raster_distort;         // 0.0 = off, line scroll raster intensity

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

    // ── Line scroll raster distortion (Genesis HBlank effect) ──
    // Per-scanline horizontal offset — creates wavy water, heat haze
    if (u_raster_distort > 0.01) {
        float scanline_y = gl_FragCoord.y;
        // Multi-frequency waves for organic look (like Sonic water)
        float wave1 = sin(scanline_y * 0.04 + u_time * 2.5) * 2.0;
        float wave2 = sin(scanline_y * 0.09 - u_time * 1.8) * 1.2;
        float wave3 = sin(scanline_y * 0.15 + u_time * 3.2) * 0.6;
        float offset = (wave1 + wave2 + wave3) * u_raster_distort / u_resolution.x;
        // Apply stronger distortion in lower screen half (underwater)
        float screen_y = gl_FragCoord.y / u_resolution.y;
        float intensity = smoothstep(0.7, 0.3, screen_y); // stronger at bottom
        uv.x += offset * intensity;
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

    // ── Per-scanline palette cycling (Genesis DMA color cycling) ──
    // Subtle hue rotation per scanline for animated water/sky
    if (u_palette_quantize > 0.01) {
        float scanline = gl_FragCoord.y;
        // Cycle speed varies by scanline group (mimics per-line palette DMA)
        float cycle = sin(scanline * 0.03 + u_time * 1.2) * 0.02 * u_palette_quantize;
        // Rotate hue slightly: approximate by shifting R/B channels
        float shifted_r = color.r * cos(cycle) - color.b * sin(cycle);
        float shifted_b = color.r * sin(cycle) + color.b * cos(cycle);
        color.r = shifted_r;
        color.b = shifted_b;
    }

    // ── Genesis 9-bit color quantization ──
    // Mega Drive CRAM: 3 bits per channel (8 levels: 0/2/4/6/8/A/C/E mapped to 0-255)
    if (u_palette_quantize > 0.01) {
        float levels = mix(256.0, 8.0, u_palette_quantize); // 8 = full Genesis, 256 = off
        color = floor(color * levels + 0.5) / levels;
    }

    // ── CRT scanlines (Genesis-accurate: every 2nd line, lighter than CRT) ──
    if (u_scanline_intensity > 0.01) {
        // Genesis had 224 active lines; use every-other-line darkening
        float line = mod(gl_FragCoord.y, 2.0);
        float scanline_factor = 1.0 - step(1.0, line) * u_scanline_intensity * 0.15;
        color *= scanline_factor;
        // RGB sub-pixel shift (horizontal offset for CRT phosphor simulation)
        float shift = 0.5 / u_resolution.x * u_scanline_intensity;
        color.r = texture(u_scene, uv + vec2(shift, 0.0)).r * scanline_factor;
        color.b = texture(u_scene, uv - vec2(shift, 0.0)).b * scanline_factor;
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
