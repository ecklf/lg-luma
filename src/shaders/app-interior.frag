uniform sampler2D u_texture;
uniform vec3 u_app_top;
uniform vec3 u_app_bottom;
varying PAINT_PRECISION vec2 v_uv;
varying PAINT_PRECISION vec4 v_color;
varying PAINT_PRECISION vec4 v_shape;

void main() {
    vec2 half_size = v_shape.zw;
    vec4 color = texture2D(u_texture, v_uv);
    float y = v_shape.y / (2.0 * half_size.y) + 0.5;
    vec3 background = mix(u_app_top, u_app_bottom, y);
    float inside = step(0.0, v_uv.x) * step(v_uv.x, 1.0) *
        step(0.0, v_uv.y) * step(v_uv.y, 1.0);
    color = vec4(mix(background, color.rgb, color.a * inside), 1.0);
    gl_FragColor = color * v_color;
}
