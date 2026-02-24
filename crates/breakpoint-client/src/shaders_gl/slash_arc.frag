#version 300 es
precision highp float;

in vec2 v_uv;

uniform float u_arc_progress; // 0.0 = start, 1.0 = full swing
uniform float u_arc_angle;    // starting angle in radians
uniform vec4 u_color;
uniform float u_time;

out vec4 frag_color;

void main() {
    // Center UV to [-1, 1]
    vec2 uv = v_uv * 2.0 - 1.0;

    // Polar coordinates
    float angle = atan(uv.y, uv.x);
    float r = length(uv);

    // Normalize angle to [0, 2pi]
    float a = mod(angle - u_arc_angle + 6.28318, 6.28318);

    // Arc sweep range (half circle)
    float sweep = u_arc_progress * 3.14159;
    float arc_dist = a / max(sweep, 0.01);

    // Only render within the swept arc
    if (a > sweep || r < 0.2 || r > 1.0) {
        discard;
    }

    // Radial intensity: bright at the arc radius ~0.7, falloff to edges
    float ring = 1.0 - abs(r - 0.6) * 3.0;
    ring = clamp(ring, 0.0, 1.0);

    // Leading edge brightness
    float edge = smoothstep(0.3, 0.0, abs(arc_dist - 1.0));

    // Speed lines: radial hash noise for anime-style streaks
    float hash = fract(sin(dot(vec2(angle * 10.0, r * 5.0), vec2(12.9898, 78.233))) * 43758.5453);
    float speed_line = step(0.7, hash) * ring;

    // Trail fade behind leading edge
    float trail = (1.0 - arc_dist) * 0.6;

    // Composite
    vec3 hot = vec3(1.0, 1.0, 0.95);
    vec3 warm = u_color.rgb;
    vec3 color = mix(warm, hot, edge * 0.8);

    float alpha = (edge * 0.9 + trail * 0.5 + speed_line * 0.3) * ring;
    alpha *= u_color.a;

    if (alpha < 0.01) {
        discard;
    }

    frag_color = vec4(color, alpha);
}
