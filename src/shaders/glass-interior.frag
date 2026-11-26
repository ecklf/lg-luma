uniform sampler2D u_texture;
uniform vec2 u_drawable;
varying PAINT_PRECISION vec4 v_color;

// Resting glass deep inside its shape: the baked interior (glass-bake.frag) at this pixel,
// tinted when focused exactly as in paint.frag. Rims keep the general shader.
void main() {
    vec3 color = texture2D(u_texture, gl_FragCoord.xy / u_drawable).rgb;
    float focused = step(0.5, v_color.r);
    color = mix(color, v_color.rgb, focused * 0.88);
    gl_FragColor = vec4(color, v_color.a);
}
