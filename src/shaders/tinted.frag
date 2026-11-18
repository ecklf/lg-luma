uniform sampler2D u_texture;
varying PAINT_PRECISION vec2 v_uv;
varying PAINT_PRECISION vec4 v_color;
void main() {
    gl_FragColor = texture2D(u_texture, v_uv) * v_color;
}
