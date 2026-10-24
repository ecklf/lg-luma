attribute vec2 a_position;
attribute vec2 a_uv;
attribute vec4 a_color;
attribute vec4 a_shape;
attribute vec2 a_params;
varying PAINT_PRECISION vec2 v_uv;
varying PAINT_PRECISION vec4 v_color;
varying PAINT_PRECISION vec4 v_shape;
varying PAINT_PRECISION vec2 v_params;
varying PAINT_PRECISION float v_logical_y;
varying PAINT_PRECISION float v_logical_x;
void main() {
    gl_Position = vec4(a_position.x / 960.0 - 1.0, 1.0 - a_position.y / 540.0, 0.0, 1.0);
    v_uv = a_uv;
    v_color = a_color;
    v_shape = a_shape;
    v_params = a_params;
    v_logical_y = a_position.y;
    v_logical_x = a_position.x;
}
