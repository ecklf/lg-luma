varying PAINT_PRECISION vec4 v_color;
varying PAINT_PRECISION float v_coverage;
varying PAINT_PRECISION float v_logical_y;
varying PAINT_PRECISION float v_logical_x;
uniform vec4 u_edge_fade;
uniform vec4 u_edge_fade_x;
void main() {
    float edge = 1.0;
    if (u_edge_fade.y > u_edge_fade.x) {
        edge = smoothstep(u_edge_fade.x, u_edge_fade.y, v_logical_y) *
            (1.0 - smoothstep(u_edge_fade.z, u_edge_fade.w, v_logical_y));
    }
    if (u_edge_fade_x.y > u_edge_fade_x.x)
        edge *= smoothstep(u_edge_fade_x.x, u_edge_fade_x.y, v_logical_x);
    if (u_edge_fade_x.w > u_edge_fade_x.z)
        edge *= 1.0 - smoothstep(u_edge_fade_x.z, u_edge_fade_x.w, v_logical_x);
    gl_FragColor = vec4(v_color.rgb, v_color.a * clamp(v_coverage, 0.0, 1.0) * edge);
}
