attribute vec2 a_position;
attribute vec2 a_offset;
attribute vec4 a_color;
attribute float a_coverage;
uniform vec4 u_transform;
uniform vec2 u_pixel_scale;
uniform vec4 u_tint;
varying PAINT_PRECISION vec4 v_color;
varying PAINT_PRECISION float v_coverage;
varying PAINT_PRECISION float v_logical_y;
void main() {
    vec2 position = a_position * u_transform.xy + u_transform.zw;
    // Offsets are centered on the path boundary and stay half a physical pixel
    // wide at every drawable density. Interior triangle edges have no fringe.
    vec2 direction = a_offset / max(abs(u_transform.xy), vec2(0.000001));
    vec2 offset = direction / max(length(direction), 0.000001) * length(a_offset);
    position += offset / u_pixel_scale;
    gl_Position = vec4(position.x / 960.0 - 1.0, 1.0 - position.y / 540.0, 0.0, 1.0);
    v_color = a_color * u_tint;
    v_coverage = a_coverage;
    v_logical_y = position.y;
}
