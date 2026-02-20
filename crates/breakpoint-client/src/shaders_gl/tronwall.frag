#version 300 es
precision highp float;

uniform vec4 u_color;
uniform float u_intensity;
uniform float u_time;
uniform vec2 u_resolution;

in vec2 v_uv;
in vec3 v_world_pos;
in float v_fog_factor;

out vec4 frag_color;

// Procedural hash noise (replaces texture-based noise)
float hash(vec2 p) {
    vec3 p3 = fract(vec3(p.xyx) * 0.1031);
    p3 += dot(p3, p3.yzx + 33.33);
    return fract((p3.x + p3.y) * p3.z);
}

float noise(vec2 p) {
    vec2 i = floor(p);
    vec2 f = fract(p);
    f = f * f * (3.0 - 2.0 * f);
    float a = hash(i);
    float b = hash(i + vec2(1.0, 0.0));
    float c = hash(i + vec2(0.0, 1.0));
    float d = hash(i + vec2(1.0, 1.0));
    return mix(mix(a, b, f.x), mix(c, d, f.x), f.y);
}

float fbm(vec2 p) {
    float v = 0.0;
    float a = 0.5;
    for (int i = 0; i < 4; i++) {
        v += a * noise(p);
        p *= 2.0;
        a *= 0.5;
    }
    return v;
}

void main() {
    // UV-based coordinates, tiled by u_resolution
    vec2 uv = v_uv;
    vec2 p = uv * u_resolution;

    float t = u_time * 0.8;

    // Flowing distortion
    float distort = fbm(p * 0.5 + t * 0.3) * 0.4;

    // Central line along v-axis (perpendicular to strip length)
    float center = abs(uv.y - 0.5) * 2.0; // 0 at center, 1 at edges

    // Animated energy line — sharp bright core with soft falloff
    float line_pos = uv.y + distort * 0.15;
    float core = exp(-pow((line_pos - 0.5) * 6.0, 2.0));

    // Secondary flowing lines
    float wave1 = sin(p.x * 0.8 + t * 1.5 + distort * 3.0) * 0.5 + 0.5;
    float wave2 = sin(p.x * 1.3 - t * 1.1 + distort * 2.0) * 0.5 + 0.5;
    float secondary = exp(-pow((uv.y - 0.3 - wave1 * 0.15) * 8.0, 2.0)) * 0.4;
    secondary += exp(-pow((uv.y - 0.7 + wave2 * 0.15) * 8.0, 2.0)) * 0.3;

    // Edge glow — soft fade at strip edges
    float edge_fade = smoothstep(0.0, 0.15, uv.y) * smoothstep(1.0, 0.85, uv.y);

    // Combine
    float brightness = (core + secondary) * edge_fade * u_intensity;

    // Subtle pulse
    brightness *= 0.85 + 0.15 * sin(t * 2.0);

    vec3 col = u_color.rgb * brightness;

    frag_color = vec4(col, u_color.a * clamp(brightness * 0.8, 0.0, 1.0));

    // Apply fog
    frag_color.rgb = mix(frag_color.rgb, vec3(0.0), v_fog_factor);
}
