uniform sampler2D u_texture;
uniform float u_aa;
uniform vec4 u_edge_fade;
uniform vec4 u_edge_fade_x;
uniform vec3 u_app_top;
uniform vec3 u_app_bottom;
uniform bool u_app;
uniform sampler2D u_glass_a;
uniform sampler2D u_glass_b;
uniform float u_glass_mix;
uniform bool u_glass_enabled;
varying PAINT_PRECISION vec2 v_uv;
varying PAINT_PRECISION vec4 v_color;
varying PAINT_PRECISION vec4 v_shape;
varying PAINT_PRECISION vec2 v_params;
varying PAINT_PRECISION float v_logical_y;
varying PAINT_PRECISION float v_logical_x;
float edge_coverage() {
    float edge = 1.0;
    if (u_edge_fade.y > u_edge_fade.x)
        edge = smoothstep(u_edge_fade.x, u_edge_fade.y, v_logical_y) *
            (1.0 - smoothstep(u_edge_fade.z, u_edge_fade.w, v_logical_y));
    if (u_edge_fade_x.y > u_edge_fade_x.x)
        edge *= smoothstep(u_edge_fade_x.x, u_edge_fade_x.y, v_logical_x);
    if (u_edge_fade_x.w > u_edge_fade_x.z)
        edge *= 1.0 - smoothstep(u_edge_fade_x.z, u_edge_fade_x.w, v_logical_x);
    return edge;
}
void main() {
    if (v_params.x < 0.0) {
        vec4 color = texture2D(u_texture, v_uv) * v_color;
        // -2: wallpaper with folded full-screen fills, p * m + k (k in v_shape).
        if (v_params.x < -1.5)
            color.rgb += v_shape.rgb;
        gl_FragColor = color * vec4(1.0, 1.0, 1.0, edge_coverage());
        return;
    }
    bool glass = v_params.y < -999.0;
    float blur = glass ? 0.0 : max(-v_params.y, 0.0);
    vec2 half_size = v_shape.zw - vec2(blur);
    float radius = min(v_params.x, min(half_size.x, half_size.y));
    vec2 q = abs(v_shape.xy) - half_size + vec2(radius);
    float d = length(max(q, vec2(0.0))) + min(max(q.x, q.y), 0.0) - radius;
    // Box-filtered coverage of one drawable pixel (u_aa) centred on the edge: an edge on a
    // pixel boundary stays one crisp step, and a one-pixel border is fully opaque.
    float alpha = clamp(0.5 - d / u_aa, 0.0, 1.0);
    if (glass) {
        vec3 color = v_color.rgb;
        if (u_glass_enabled) {
            // Rounded SDF normal bends the wallpaper most strongly at the curved rim.
            vec2 normal = sign(v_shape.xy) * normalize(max(q, vec2(0.001)));
            float rim = 1.0 - smoothstep(0.0, 22.0, -d);
            vec2 uv = (vec2(v_logical_x, v_logical_y) - normal * rim * 12.0) / vec2(1920.0, 1080.0);
            vec3 backdrop = mix(texture2D(u_glass_a, uv).rgb, texture2D(u_glass_b, uv).rgb, u_glass_mix);
            float tint = mix(0.66, 0.92, step(0.5, v_color.r));
            color = mix(backdrop, v_color.rgb, tint);
            float light = max(dot(normal, normalize(vec2(-0.5, -1.0))), 0.0);
            color += vec3(0.035 + rim * (0.035 + light * 0.16));
        }
        gl_FragColor = vec4(color, alpha * v_color.a * edge_coverage());
        return;
    }
    if (blur > 0.0) {
        alpha = 1.0 - smoothstep(-blur, blur, d);
    }
    if (v_params.y > 0.0) {
        alpha *= clamp(0.5 + (d + v_params.y) / u_aa, 0.0, 1.0);
    }
    vec4 color = texture2D(u_texture, v_uv);
    if (u_app) {
        float y = v_shape.y / (2.0 * half_size.y) + 0.5;
        vec3 background = mix(u_app_top, u_app_bottom, y);
        float inside = step(0.0, v_uv.x) * step(v_uv.x, 1.0) *
            step(0.0, v_uv.y) * step(v_uv.y, 1.0);
        color = vec4(mix(background, color.rgb, color.a * inside), 1.0);
    }
    gl_FragColor = color * v_color * vec4(1.0, 1.0, 1.0, alpha * edge_coverage());
}
