// A window filling like a tank: the water comes up from the bottom with a
// wavy line, a bright edge where it meets the air, the window bent a little
// just under it; above it, nothing yet.
//
// s.a.x  how full: 0 empty, 1 full

fn level(s: Shader, x: f32) -> f32 {
    let t = clamp(s.a.x, 0.0, 1.0);
    let amp = 14.0 * (1.0 - t) + 2.0;
    let w = amp * (0.6 * sin(x * 0.016 + t * 7.0) + 0.4 * sin(x * 0.043 - t * 11.0));
    return (s.size.y + 2.0 * amp) * (1.0 - t) - amp + w;
}

fn shade(s: Shader) -> vec4<f32> {
    let t = clamp(s.a.x, 0.0, 1.0);
    let p = s.pos;
    if (t >= 0.999) { return inside(s, p); }
    // How far under the water (> 0) or above it.
    let d = p.y - level(s, p.x);
    if (d < -2.0) { return vec4<f32>(0.0); }
    let calm = 1.0 - t;
    // Just under the line the water is a lens: what is under it, lifted.
    let lift = 10.0 * exp(-max(d, 0.0) / 18.0) * calm;
    let c = inside(s, p + vec2<f32>(0.0, lift));
    var rgb = c.rgb + vec3<f32>(0.62, 0.84, 0.74) * exp(-max(d, 0.0) / 30.0) * 0.16 * calm;
    // The edge, where it meets the air.
    let rim = exp(-(d - 1.0) * (d - 1.0) / 3.0);
    rgb = mix(rgb, vec3<f32>(0.88, 1.0, 0.95), rim * 0.85);
    return vec4<f32>(rgb, max(c.a, rim * 0.9) * smoothstep(-2.0, 0.5, d));
}
