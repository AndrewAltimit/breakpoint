#version 300 es
precision highp float;

in vec2 v_uv;
in vec3 v_world_pos;

uniform sampler2D u_texture;
uniform sampler2D u_palette;      // 256x1 palette texture (indexed mode)
uniform float u_use_palette;      // >0.5 enables indexed palette lookup
uniform vec4 u_sprite_rect; // atlas sub-region: (u0, v0, u1, v1)
uniform vec4 u_tint;
uniform float u_flip_x;

// Dynamic lighting uniforms (32 colored lights)
uniform vec4 u_lights[32];       // xy=position, z=intensity, w=radius
uniform vec4 u_light_color[32];  // rgb=color, a=type (0=point)
uniform int u_light_count;
uniform float u_ambient;          // 0.0 = pitch black, 1.0 = fully lit
uniform vec3 u_ambient_color;     // per-room ambient RGB tint
uniform float u_fog_density;      // ground fog density
uniform float u_outline_width;    // >0 enables dark pixel outline on characters
uniform float u_dissolve;         // 0.0=solid, 1.0=fully dissolved (death effect)

// GBA-style color ramp (all zero = disabled)
uniform vec3 u_ramp_shadow;      // dark palette color
uniform vec3 u_ramp_mid;         // midtone palette color
uniform vec3 u_ramp_highlight;   // bright palette color
uniform float u_posterize;       // 0.0=off, 31.0=GBA 5-bit depth

out vec4 frag_color;

void main() {
    // Map quad UV [0,1] to atlas sub-rect
    float u = mix(u_sprite_rect.x, u_sprite_rect.z, v_uv.x);
    // Flip horizontally if u_flip_x > 0.5
    if (u_flip_x > 0.5) {
        u = u_sprite_rect.x + u_sprite_rect.z - u;
    }
    float v = mix(u_sprite_rect.w, u_sprite_rect.y, v_uv.y);
    vec4 texel = texture(u_texture, vec2(u, v));
    vec4 color;
    // Indexed palette mode (deferred activation: u_use_palette always 0.0 for now)
    if (u_use_palette > 0.5) {
        float index = texel.r;
        color = texture(u_palette, vec2(index, 0.5));
        color.a = texel.a;
    } else {
        color = texel * u_tint;
    }
    // MBAACC-style binary alpha: snap to 0 or 1 for crisp pixel edges
    color.a = step(0.5, color.a) * u_tint.a;

    // Pixel outline: if this pixel is transparent but a neighbor has alpha, draw dark outline
    if (u_outline_width > 0.0 && color.a < 0.01) {
        vec2 texel_size = 1.0 / vec2(textureSize(u_texture, 0));
        vec2 uv_pos = vec2(u, v);
        float max_a = 0.0;
        max_a = max(max_a, texture(u_texture, uv_pos + vec2(texel_size.x, 0.0)).a);
        max_a = max(max_a, texture(u_texture, uv_pos - vec2(texel_size.x, 0.0)).a);
        max_a = max(max_a, texture(u_texture, uv_pos + vec2(0.0, texel_size.y)).a);
        max_a = max(max_a, texture(u_texture, uv_pos - vec2(0.0, texel_size.y)).a);
        if (max_a > 0.5) {
            frag_color = vec4(0.08, 0.04, 0.12, 0.9);
            return;
        }
        discard;
    }

    if (color.a < 0.01) {
        discard;
    }

    // Pixel-dissolve death effect: hash-based noise discard
    if (u_dissolve > 0.0) {
        float noise = fract(sin(dot(floor(v_uv * 40.0), vec2(12.9898, 78.233))) * 43758.5453);
        if (noise < u_dissolve) discard;
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
        // Blend ramp with original tinted color to preserve some texture detail
        color.rgb = mix(color.rgb, ramped, 0.7);
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
        color.rgb *= clamp(light, vec3(0.0), vec3(1.5));
    }

    // GBA 5-bit posterization: reduces color depth for authentic banding
    if (u_posterize > 0.5) {
        color.rgb = floor(color.rgb * u_posterize) / u_posterize;
    }

    // Ground fog effect
    if (u_fog_density > 0.01) {
        vec3 fog_color = vec3(0.08, 0.06, 0.12);
        float fog = smoothstep(0.0, 3.0, v_world_pos.y);
        color.rgb = mix(fog_color, color.rgb, mix(1.0 - u_fog_density * 0.5, 1.0, fog));
    }

    frag_color = color;
}
