varying PAINT_PRECISION vec4 v_color;
varying PAINT_PRECISION float v_coverage;
varying PAINT_PRECISION float v_logical_y;
varying PAINT_PRECISION float v_logical_x;
uniform vec4 u_edge_fade;
uniform vec4 u_edge_fade_x;
uniform bool u_clock_glass;
uniform bool u_glass_enabled;
uniform sampler2D u_glass_a;
uniform sampler2D u_glass_b;
uniform float u_glass_mix;
uniform vec2 u_text_bounds;

void main() {
    float edge = 1.0;
    if (u_edge_fade.y > u_edge_fade.x)
        edge = smoothstep(u_edge_fade.x, u_edge_fade.y, v_logical_y) *
            (1.0 - smoothstep(u_edge_fade.z, u_edge_fade.w, v_logical_y));
    if (u_edge_fade_x.y > u_edge_fade_x.x)
        edge *= smoothstep(u_edge_fade_x.x, u_edge_fade_x.y, v_logical_x);
    if (u_edge_fade_x.w > u_edge_fade_x.z)
        edge *= 1.0 - smoothstep(u_edge_fade_x.z, u_edge_fade_x.w, v_logical_x);
    float coverage = clamp(v_coverage, 0.0, 1.0);
    vec3 color = v_color.rgb;
    if (u_clock_glass) {
        float y = clamp((v_logical_y - u_text_bounds.x) / max(u_text_bounds.y, 1.0), 0.0, 1.0);
        float reflection = 0.5 + 0.5 * cos(y * 10.0 - 1.0);
        vec3 ice = mix(vec3(0.34, 0.77, 0.96), vec3(0.86, 0.98, 1.0), reflection);
        vec3 backdrop = vec3(0.20, 0.36, 0.46);
        if (u_glass_enabled) {
            vec2 uv = (vec2(v_logical_x, v_logical_y) + vec2(4.0 * sin(y * 7.0), 5.0 * cos(y * 6.0))) / vec2(1920.0, 1080.0);
            uv = clamp(uv, vec2(0.5 / 960.0, 0.5 / 540.0), vec2(959.5 / 960.0, 539.5 / 540.0));
            uv.y *= 0.5;
            backdrop = mix(texture2D(u_glass_a, uv).rgb, texture2D(u_glass_b, uv).rgb, u_glass_mix);
        }
        // Cool translucent fill and a narrow luminous boundary. The vector
        // outlines stay sharp at every height, without a bitmap digit atlas.
        color = mix(backdrop, ice, 0.84);
        color = mix(color, vec3(0.88, 1.0, 1.0), (1.0 - coverage) * 0.85);
        color *= v_color.rgb;
    }
    gl_FragColor = vec4(color, v_color.a * coverage * edge);
}
