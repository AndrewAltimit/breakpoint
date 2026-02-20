#version 300 es
precision highp float;

uniform vec4 u_color;
uniform float u_intensity;
uniform float u_time;
uniform vec2 u_resolution; // (tiles_x, noise_scale)

in vec2 v_uv;
in float v_fog_factor;

out vec4 frag_color;

// Fast arithmetic hash — no trig
float hash(vec2 p) {
    vec3 p3 = fract(vec3(p.xyx) * 0.1031);
    p3 += dot(p3, p3.yzx + 33.33);
    return fract((p3.x + p3.y) * p3.z);
}

// Scalar bilinear noise
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
    float tiles = u_resolution.x;
    float nscale = u_resolution.y;

    // Square noise cells: tile x by aspect ratio, shift y center to bottom
    vec2 uv = vec2(v_uv.x * tiles, v_uv.y + 0.5) * nscale;
    uv = uv * 2.0 - vec2(tiles * nscale, nscale);

    // Fast scroll for energetic flame
    float scroll = u_time * 1.8;
    uv.y += (uv.y < 0.0) ? scroll : -scroll;

    float dist = length(2.0 * fract(uv) - 1.0);
    float r = hash(floor(uv));

    float a1 = 6.28318 * r;
    float c1 = cos(a1), s1 = sin(a1);
    const float c2 = -0.5403, s2 = 0.8415;

    // Primary noise displacement
    float n1 = noise(vec2(dot(uv + r, vec2(c1, -s1)), dot(uv + r, vec2(s1, c1))));
    float n2 = noise(vec2(dot(uv, vec2(c2, -s2)), dot(uv, vec2(s2, c2))));
    float disp = mix(n1, n2, smoothstep(0.5, 0.93, dist)) * 0.5;

    // Turbulence layer — high-frequency jitter for chaotic edges
    float turb = noise(uv * 3.0 + u_time * 4.0) * 0.15;
    disp = disp * 0.8 + turb;

    // Flame line: noise scales distance from bottom edge
    float distorted_y = v_uv.y * disp;
    float line = smoothstep(0.1, 0.0, abs(distorted_y));

    // Fade toward top of strip
    float fade = 1.0 - smoothstep(0.0, 0.8, v_uv.y);

    float brightness = line * fade * u_intensity;

    // Early discard transparent fragments — saves blend work on ~80% of strip area
    if (brightness < 0.01) discard;

    vec3 col = u_color.rgb * brightness;
    frag_color = vec4(col, u_color.a * clamp(brightness, 0.0, 1.0));

    // Fog
    frag_color.rgb = mix(frag_color.rgb, vec3(0.0), v_fog_factor);
}
