uniform sampler2D u_texture;
varying PAINT_PRECISION vec2 v_uv;
varying PAINT_PRECISION vec4 v_color;
varying PAINT_PRECISION vec4 v_shape;
varying PAINT_PRECISION vec2 v_params;
void main() {
    float blur = -v_params.y;
    vec2 half_size = v_shape.zw - vec2(blur);
    float radius = min(v_params.x, min(half_size.x, half_size.y));
    vec2 q = abs(v_shape.xy) - half_size + vec2(radius);
    float d = length(max(q, vec2(0.0))) + min(max(q.x, q.y), 0.0) - radius;
    float alpha = 1.0 - smoothstep(-blur, blur, d);
    gl_FragColor = texture2D(u_texture, v_uv) * v_color * vec4(1.0, 1.0, 1.0, alpha);
}
