#version 300 es
precision highp float;

in vec2 v_uv;

uniform vec4 u_color;       // ray color + alpha
uniform float u_intensity;  // overall ray brightness
uniform float u_time;
uniform float u_speed;      // ray animation speed (reusing uniform)

out vec4 frag_color;

void main() {
    // Center UV to [-1, 1]
    vec2 uv = v_uv * 2.0 - 1.0;

    // Light source at top-center of quad
    vec2 light_pos = vec2(0.0, 1.0);
    vec2 delta = uv - light_pos;

    // Radial blur: sample along ray direction
    float intensity = 0.0;
    const int NUM_SAMPLES = 16;
    vec2 step_dir = delta / float(NUM_SAMPLES);

    vec2 sample_pos = light_pos;
    for (int i = 0; i < NUM_SAMPLES; i++) {
        sample_pos += step_dir;

        // Compute angular ray pattern (8 rays with noise)
        float angle = atan(sample_pos.y, sample_pos.x);
        float r = length(sample_pos - light_pos);

        // Create ray pattern with angular frequency
        float ray = sin(angle * 4.0 + u_time * u_speed) * 0.5 + 0.5;
        ray *= sin(angle * 7.0 - u_time * u_speed * 0.7) * 0.3 + 0.7;

        // Attenuate with distance from source
        float atten = 1.0 - smoothstep(0.0, 2.0, r);

        intensity += ray * atten;
    }
    intensity /= float(NUM_SAMPLES);

    // Distance falloff from light source
    float dist = length(delta);
    float falloff = 1.0 - smoothstep(0.0, 1.8, dist);

    // Downward fade (rays should fade toward bottom)
    float vertical_fade = smoothstep(-1.0, 0.5, uv.y);

    float final_alpha = intensity * falloff * vertical_fade * u_intensity * u_color.a;
    if (final_alpha < 0.01) {
        discard;
    }

    frag_color = vec4(u_color.rgb * (1.0 + intensity * 0.3), final_alpha);
}
