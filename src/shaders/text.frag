// Text has a single tint per draw. Apply it here rather than transporting a
// constant color through a vertex varying, keeping label polarity independent
// of the TV driver's varying interpolation path.
uniform vec4 u_tint;
uniform vec4 u_edge_fade;
varying PAINT_PRECISION float v_coverage;
varying PAINT_PRECISION float v_logical_y;
void main() {
    float edge = 1.0;
    if (u_edge_fade.y > u_edge_fade.x) {
        edge = smoothstep(u_edge_fade.x, u_edge_fade.y, v_logical_y) *
            (1.0 - smoothstep(u_edge_fade.z, u_edge_fade.w, v_logical_y));
    }
    gl_FragColor = vec4(u_tint.rgb, u_tint.a * clamp(v_coverage, 0.0, 1.0) * edge);
}
