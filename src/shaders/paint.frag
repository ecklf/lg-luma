uniform sampler2D u_texture;
uniform float u_aa;
uniform vec4 u_edge_fade;
uniform vec4 u_edge_fade_x;
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
    float blur = max(-v_params.y, 0.0);
    vec2 half_size = v_shape.zw - vec2(blur);
    float radius = min(v_params.x, min(half_size.x, half_size.y));
    vec2 q = abs(v_shape.xy) - half_size + vec2(radius);
    float d = length(max(q, vec2(0.0))) + min(max(q.x, q.y), 0.0) - radius;
    // Box-filtered coverage of one drawable pixel (u_aa) centred on the edge: an edge on a
    // pixel boundary stays one crisp step, and a one-pixel border is fully opaque.
    float alpha = clamp(0.5 - d / u_aa, 0.0, 1.0);
    if (blur > 0.0) {
        alpha = 1.0 - smoothstep(-blur, blur, d);
    }
    if (v_params.y > 0.0) {
        alpha *= clamp(0.5 + (d + v_params.y) / u_aa, 0.0, 1.0);
    }
    gl_FragColor = texture2D(u_texture, v_uv) * v_color * vec4(1.0, 1.0, 1.0, alpha * edge_coverage());
}
