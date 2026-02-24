#version 300 es
precision highp float;

in vec2 v_uv;

uniform vec4 u_color;
uniform float u_arc_progress; // 0.0 = start, 1.0 = full swing
uniform float u_time;

out vec4 frag_color;

void main() {
    // Arc sweep: bright leading edge fading to trail
    float arc_angle = u_arc_progress * 3.14159;

    // UV.x maps along the arc, UV.y maps perpendicular
    float arc_pos = v_uv.x;
    float perp = abs(v_uv.y - 0.5) * 2.0;

    // Leading edge is at arc_progress position
    float dist_from_edge = abs(arc_pos - u_arc_progress);
    float edge_brightness = smoothstep(0.3, 0.0, dist_from_edge);

    // Trail behind the leading edge
    float trail = step(arc_pos, u_arc_progress) * (1.0 - arc_pos / max(u_arc_progress, 0.01));

    // Perpendicular falloff (thin arc line)
    float width_falloff = smoothstep(1.0, 0.3, perp);

    // Color: white-hot at leading edge, fading to warm color
    vec3 hot = vec3(1.0, 1.0, 0.9);
    vec3 warm = u_color.rgb;
    vec3 color = mix(warm, hot, edge_brightness);

    float alpha = (edge_brightness * 0.8 + trail * 0.4) * width_falloff;
    alpha *= u_color.a;

    if (alpha < 0.01) {
        discard;
    }

    frag_color = vec4(color, alpha);
}
