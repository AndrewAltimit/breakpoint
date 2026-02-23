#version 300 es
precision highp float;

in vec2 v_uv;
in vec3 v_world_pos;

uniform float u_time;
uniform vec4 u_color;       // water base color (RGBA)
uniform float u_depth;      // visual depth (0-1)
uniform float u_wave_speed; // wave animation speed

out vec4 frag_color;

// Simple hash for caustic noise
float hash(vec2 p) {
    return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453);
}

void main() {
    float wx = v_world_pos.x;
    float wy = v_world_pos.y;

    // Surface wave displacement on top edge
    float wave = sin(wx * 4.0 + u_time * u_wave_speed) * 0.08;
    float surface_line = 1.0 - v_uv.y; // 0 at bottom, 1 at top

    // Discard above surface wave
    if (surface_line > 0.95 + wave) {
        discard;
    }

    // Base semi-transparent water color
    float alpha = mix(0.5, 0.7, u_depth);
    vec3 base = u_color.rgb;

    // Darken with depth (lower parts are darker)
    float depth_factor = mix(1.0, 0.7, v_uv.y);
    base *= depth_factor;

    // Caustic light pattern
    vec2 caustic_uv = vec2(wx * 2.0 + u_time * 0.3, wy * 2.0 + u_time * 0.2);
    float caustic = hash(floor(caustic_uv));
    caustic = smoothstep(0.6, 0.9, caustic) * 0.3;
    base += vec3(caustic * 0.5, caustic * 0.7, caustic);

    // Bright surface highlight line
    float highlight = smoothstep(0.02, 0.0, abs(surface_line - 0.9 - wave));
    base += vec3(0.3, 0.4, 0.5) * highlight;

    frag_color = vec4(base, alpha * u_color.a);
}
