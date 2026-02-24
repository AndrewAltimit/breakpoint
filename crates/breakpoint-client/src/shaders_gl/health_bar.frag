#version 300 es
precision highp float;

in vec2 v_uv;

uniform vec4 u_color;       // health bar fill color
uniform float u_intensity;  // fill amount (0.0-1.0)
uniform float u_time;

out vec4 frag_color;

void main() {
    // Border (dark outline)
    float border = 0.08;
    if (v_uv.x < border || v_uv.x > 1.0 - border ||
        v_uv.y < border || v_uv.y > 1.0 - border) {
        frag_color = vec4(0.05, 0.02, 0.08, 0.9);
        return;
    }

    // Background (dark)
    float fill = u_intensity;
    if (v_uv.x > fill) {
        frag_color = vec4(0.1, 0.08, 0.12, 0.6);
        return;
    }

    // Fill color with subtle pulse animation
    float pulse = 1.0 + 0.05 * sin(u_time * 3.0);
    vec3 bar_color = u_color.rgb * pulse;

    // Color gradient: green -> yellow -> red based on health
    if (fill < 0.3) {
        bar_color = mix(vec3(0.8, 0.1, 0.1), bar_color, fill / 0.3);
    }

    // Bright edge at fill boundary
    float edge = smoothstep(0.02, 0.0, abs(v_uv.x - fill));
    bar_color += vec3(0.3) * edge;

    frag_color = vec4(bar_color, 0.9);
}
