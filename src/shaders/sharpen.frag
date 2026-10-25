uniform sampler2D u_image;
uniform vec2 u_texel;
uniform float u_strength;
varying PAINT_PRECISION vec2 v_uv;
void main() {
    vec4 center = texture2D(u_image, v_uv);
    vec3 north = texture2D(u_image, v_uv + vec2(0.0, u_texel.y)).rgb;
    vec3 south = texture2D(u_image, v_uv - vec2(0.0, u_texel.y)).rgb;
    vec3 east = texture2D(u_image, v_uv + vec2(u_texel.x, 0.0)).rgb;
    vec3 west = texture2D(u_image, v_uv - vec2(u_texel.x, 0.0)).rgb;
    vec3 lo = min(center.rgb, min(min(north, south), min(east, west)));
    vec3 hi = max(center.rgb, max(max(north, south), max(east, west)));
    // Flat/near-flat areas retain their original value. Local extrema bound
    // the result, preventing new bright/dark halos around logos and text.
    vec3 range = hi - lo;
    float contrast = max(range.r, max(range.g, range.b));
    float gate = smoothstep(0.015, 0.08, contrast);
    vec3 detail = center.rgb - (north + south + east + west) * 0.25;
    gl_FragColor = vec4(clamp(center.rgb + u_strength * gate * detail, lo, hi), center.a);
}
