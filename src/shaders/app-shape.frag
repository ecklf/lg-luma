uniform sampler2D u_texture;
uniform float u_aa;
uniform vec3 u_app_top;
uniform vec3 u_app_bottom;
varying PAINT_PRECISION vec2 v_uv;
varying PAINT_PRECISION vec4 v_color;
varying PAINT_PRECISION vec4 v_shape;
varying PAINT_PRECISION vec2 v_params;

// Installed-app artwork on its rounded backdrop, translucent or at the rounded edge: the
// general paint shader's app path without its glass, blur and edge-fade branches.
void main() {
    vec2 half_size = v_shape.zw;
    float radius = min(v_params.x, min(half_size.x, half_size.y));
    vec2 q = abs(v_shape.xy) - half_size + vec2(radius);
    float d = length(max(q, vec2(0.0))) + min(max(q.x, q.y), 0.0) - radius;
    float alpha = clamp(0.5 - d / u_aa, 0.0, 1.0);
    if (v_params.y > 0.0) {
        alpha *= clamp(0.5 + (d + v_params.y) / u_aa, 0.0, 1.0);
    }
    vec4 color = texture2D(u_texture, v_uv);
    float y = v_shape.y / (2.0 * half_size.y) + 0.5;
    vec3 background = mix(u_app_top, u_app_bottom, y);
    float inside = step(0.0, v_uv.x) * step(v_uv.x, 1.0) *
        step(0.0, v_uv.y) * step(v_uv.y, 1.0);
    color = vec4(mix(background, color.rgb, color.a * inside), 1.0);
    gl_FragColor = color * v_color * vec4(1.0, 1.0, 1.0, alpha);
}
