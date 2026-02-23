#version 300 es
precision highp float;

in vec2 v_uv;
in vec3 v_world_pos;

uniform float u_time;
uniform vec4 u_color;       // fog color + alpha
uniform float u_intensity;  // fog density

out vec4 frag_color;

// Simple value noise
float hash(vec2 p) {
    return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453);
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

void main() {
    // Multi-layer scrolling fog
    float wx = v_world_pos.x;

    // Two layers at different speeds and scales
    float fog1 = noise(vec2(wx * 0.5 + u_time * 0.3, v_uv.y * 2.0 + u_time * 0.1));
    float fog2 = noise(vec2(wx * 0.8 - u_time * 0.2, v_uv.y * 3.0 - u_time * 0.15));
    float fog = (fog1 + fog2) * 0.5;

    // Height-based density falloff (thicker at bottom)
    float height_fade = 1.0 - smoothstep(0.0, 1.0, v_uv.y);

    float alpha = fog * height_fade * u_intensity * u_color.a;
    if (alpha < 0.01) {
        discard;
    }

    frag_color = vec4(u_color.rgb, alpha);
}
