uniform sampler2D u_glass_a;
varying PAINT_PRECISION float v_logical_y;
varying PAINT_PRECISION float v_logical_x;

// The interior of resting glass depends only on the backdrop and the position: computed once
// per backdrop and drawable size with the general paint shader's formula (paint.frag).
vec3 glass_sample(vec2 uv, float scattering) {
    uv = clamp(uv, vec2(0.5 / 960.0, 0.5 / 540.0), vec2(959.5 / 960.0, 539.5 / 540.0));
    vec2 sharp_uv = vec2(uv.x, uv.y * 0.5);
    vec2 soft_uv = sharp_uv + vec2(0.0, 0.5);
    vec3 sharp = texture2D(u_glass_a, sharp_uv).rgb;
    vec3 soft = texture2D(u_glass_a, soft_uv).rgb;
    return mix(sharp, soft, scattering);
}

void main() {
    vec3 backdrop = glass_sample(vec2(v_logical_x, v_logical_y) / vec2(1920.0, 1080.0), 0.88);
    float luminance = dot(backdrop, vec3(0.2126, 0.7152, 0.0722));
    gl_FragColor = vec4(mix(backdrop, vec3(0.075), 0.14 + 0.44 * luminance), 1.0);
}
