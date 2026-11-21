uniform sampler2D u_texture;
varying PAINT_PRECISION vec2 v_uv;
varying PAINT_PRECISION vec4 v_color;
varying PAINT_PRECISION vec4 v_shape;
varying PAINT_PRECISION vec2 v_params;
void main() {
    vec4 color = texture2D(u_texture, v_uv) * v_color;
    if (v_params.x < -1.5)
        color.rgb += v_shape.rgb;
    gl_FragColor = color;
}
