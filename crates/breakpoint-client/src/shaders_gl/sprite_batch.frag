#version 300 es
precision highp float;

in vec2 v_uv;
in vec3 v_world_pos;
in vec4 v_tint;

uniform sampler2D u_texture;

// Dynamic lighting uniforms (32 colored lights)
uniform vec4 u_lights[32];       // xy=position, z=intensity, w=radius
uniform vec4 u_light_color[32];  // rgb=color, a=type (0=point)
uniform int u_light_count;
uniform float u_ambient;          // 0.0 = pitch black, 1.0 = fully lit
uniform vec3 u_ambient_color;     // per-room ambient RGB tint
uniform float u_fog_density;      // ground fog density
uniform vec3 u_fog_color;         // per-room fog color

// GBA-style color ramp (all zero = disabled)
uniform vec3 u_ramp_shadow;      // dark palette color
uniform vec3 u_ramp_mid;         // midtone palette color
uniform vec3 u_ramp_highlight;   // bright palette color
uniform float u_posterize;       // 0.0=off, 31.0=GBA 5-bit depth

out vec4 frag_color;

void main() {
    // v_uv is already in atlas space (pre-computed on CPU)
    vec4 texel = texture(u_texture, v_uv);
    vec4 color = texel * v_tint;

    // MBAACC-style binary alpha: snap to 0 or 1 for crisp pixel edges
    color.a = step(0.5, color.a) * v_tint.a;

    if (color.a < 0.01) {
        discard;
    }

    // GBA-style color ramp: map luminance through a 3-point palette
    bool ramp_active = (u_ramp_shadow.r + u_ramp_shadow.g + u_ramp_shadow.b) > 0.01;
    if (ramp_active) {
        float lum = dot(color.rgb, vec3(0.299, 0.587, 0.114));
        vec3 ramped;
        if (lum < 0.5) {
            ramped = mix(u_ramp_shadow, u_ramp_mid, lum * 2.0);
        } else {
            ramped = mix(u_ramp_mid, u_ramp_highlight, (lum - 0.5) * 2.0);
        }
        color.rgb = mix(color.rgb, ramped, 0.35);
    }

    // Apply dynamic lighting if lights are present
    if (u_light_count > 0) {
        vec3 light = u_ambient * u_ambient_color;
        for (int i = 0; i < 32; i++) {
            if (i >= u_light_count) break;
            vec2 light_pos = u_lights[i].xy;
            float intensity = u_lights[i].z;
            float radius = u_lights[i].w;
            vec3 lcolor = u_light_color[i].rgb;
            float dist = distance(v_world_pos.xy, light_pos);
            float attenuation = 1.0 - smoothstep(0.0, radius, dist);
            light += attenuation * intensity * lcolor;
        }
        light = max(light, vec3(u_ambient * 0.2));
        color.rgb *= clamp(light, vec3(0.0), vec3(1.5));
    }

    // GBA 5-bit posterization
    if (u_posterize > 0.5) {
        color.rgb = floor(color.rgb * u_posterize) / u_posterize;
    }

    // Ground fog effect (per-room colored)
    if (u_fog_density > 0.01) {
        float fog = smoothstep(0.0, 3.0, v_world_pos.y);
        color.rgb = mix(u_fog_color, color.rgb, mix(1.0 - u_fog_density * 0.5, 1.0, fog));
    }

    frag_color = color;
}
