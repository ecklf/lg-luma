attribute vec2 a_position;
attribute vec2 a_offset;
attribute vec4 a_origin;
attribute float a_coverage;
uniform vec4 u_transform;
uniform vec2 u_pixel_scale;
uniform vec4 u_tint;
uniform float u_fringe;
varying PAINT_PRECISION vec4 v_color;
varying PAINT_PRECISION float v_coverage;
varying PAINT_PRECISION float v_logical_y;
varying PAINT_PRECISION float v_logical_x;
void main() {
    // Snap each glyph's insertion origin/baseline, not individual outline
    // vertices. Curves retain their shape while scrolling and changing density.
    vec2 origin = a_origin.xy + u_transform.zw;
    origin = floor(origin * u_pixel_scale + vec2(0.5)) / u_pixel_scale;
    // With MSAA, use the original outline; inward offsets can fold thin stems.
    // Single-sample targets retain a narrower analytic boundary fringe.
    vec2 position = origin + a_position + a_offset * u_fringe / u_pixel_scale;
    gl_Position = vec4(position.x / 960.0 - 1.0, 1.0 - position.y / 540.0, 0.0, 1.0);
    v_color = u_tint;
    v_coverage = mix(1.0, a_coverage, step(0.001, u_fringe));
    v_logical_y = position.y;
    v_logical_x = position.x;
}
