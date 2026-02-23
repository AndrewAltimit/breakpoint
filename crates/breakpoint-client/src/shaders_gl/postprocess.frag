#version 300 es
precision highp float;

in vec2 v_uv;

uniform sampler2D u_scene;
uniform vec2 u_resolution;
uniform float u_time;
uniform float u_scanline_intensity;  // 0.0-1.0
uniform float u_bloom_intensity;     // 0.0-1.0
uniform float u_vignette_intensity;  // 0.0-1.0
uniform float u_crt_curvature;       // 0.0-1.0

out vec4 frag_color;

void main() {
    vec2 uv = v_uv;

    // CRT barrel distortion
    if (u_crt_curvature > 0.01) {
        vec2 centered = uv - 0.5;
        float dist2 = dot(centered, centered);
        uv = 0.5 + centered * (1.0 + dist2 * u_crt_curvature * 0.5);
        // Discard if outside screen bounds
        if (uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0) {
            frag_color = vec4(0.0, 0.0, 0.0, 1.0);
            return;
        }
    }

    vec3 color = texture(u_scene, uv).rgb;

    // Bloom: 8-tap cross-pattern sampling for bright areas
    if (u_bloom_intensity > 0.01) {
        vec2 texel = 1.0 / u_resolution;
        vec3 bloom = vec3(0.0);
        float offsets[4] = float[](1.0, 2.0, 3.0, 4.0);
        for (int i = 0; i < 4; i++) {
            float o = offsets[i];
            bloom += texture(u_scene, uv + vec2(o * texel.x, 0.0)).rgb;
            bloom += texture(u_scene, uv - vec2(o * texel.x, 0.0)).rgb;
            bloom += texture(u_scene, uv + vec2(0.0, o * texel.y)).rgb;
            bloom += texture(u_scene, uv - vec2(0.0, o * texel.y)).rgb;
        }
        bloom /= 16.0;
        // Only add bright parts (luminance threshold)
        float lum = dot(bloom, vec3(0.299, 0.587, 0.114));
        float threshold = 0.5;
        bloom *= smoothstep(threshold, threshold + 0.3, lum);
        color += bloom * u_bloom_intensity;
    }

    // CRT scanlines
    if (u_scanline_intensity > 0.01) {
        float scanline = 0.8 + 0.2 * sin(gl_FragCoord.y * 3.14159);
        color *= mix(1.0, scanline, u_scanline_intensity);
        // RGB sub-pixel shift
        float shift = 0.5 / u_resolution.x * u_scanline_intensity;
        color.r = texture(u_scene, uv + vec2(shift, 0.0)).r;
        color.b = texture(u_scene, uv - vec2(shift, 0.0)).b;
    }

    // Vignette: darken edges
    if (u_vignette_intensity > 0.01) {
        vec2 centered = uv - 0.5;
        float vignette = 1.0 - dot(centered, centered) * 2.0;
        vignette = clamp(vignette, 0.0, 1.0);
        color *= mix(1.0, vignette, u_vignette_intensity);
    }

    frag_color = vec4(color, 1.0);
}
