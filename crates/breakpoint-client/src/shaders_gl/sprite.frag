#version 300 es
precision highp float;

in vec2 v_uv;
in vec3 v_world_pos;

uniform sampler2D u_texture;
uniform vec4 u_sprite_rect; // atlas sub-region: (u0, v0, u1, v1)
uniform vec4 u_tint;
uniform float u_flip_x;

// Dynamic lighting uniforms
uniform vec4 u_lights[16];  // xy=position, z=intensity, w=radius
uniform int u_light_count;
uniform float u_ambient;     // 0.0 = pitch black, 1.0 = fully lit
uniform float u_fog_density; // ground fog density

out vec4 frag_color;

void main() {
    // Map quad UV [0,1] to atlas sub-rect
    float u = mix(u_sprite_rect.x, u_sprite_rect.z, v_uv.x);
    // Flip horizontally if u_flip_x > 0.5
    if (u_flip_x > 0.5) {
        u = u_sprite_rect.x + u_sprite_rect.z - u;
    }
    float v = mix(u_sprite_rect.y, u_sprite_rect.w, v_uv.y);
    vec4 texel = texture(u_texture, vec2(u, v));
    vec4 color = texel * u_tint;
    if (color.a < 0.01) {
        discard;
    }

    // Apply dynamic lighting if lights are present
    if (u_light_count > 0) {
        float light = u_ambient;
        for (int i = 0; i < 16; i++) {
            if (i >= u_light_count) break;
            vec2 light_pos = u_lights[i].xy;
            float intensity = u_lights[i].z;
            float radius = u_lights[i].w;
            float dist = distance(v_world_pos.xy, light_pos);
            float attenuation = 1.0 - smoothstep(0.0, radius, dist);
            light += attenuation * intensity;
        }
        color.rgb *= clamp(light, 0.0, 1.5);
    }

    // Ground fog effect
    if (u_fog_density > 0.01) {
        vec3 fog_color = vec3(0.15, 0.12, 0.2);
        float fog = smoothstep(0.0, 3.0, v_world_pos.y);
        color.rgb = mix(fog_color, color.rgb, mix(1.0 - u_fog_density * 0.5, 1.0, fog));
    }

    frag_color = color;
}
