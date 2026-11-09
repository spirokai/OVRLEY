// Haze over the far ground. On flat ground a pixel's depth follows its row alone, so the map centre's
// share of this row's depth is 1 at the centre and falls to 0 at the horizon. The haze starts where
// that share drops below `haze.x` and reaches strength `haze.z` at `haze.y`.
@fragment
fn main(
    @location(0) color: vec4<f32>,
    @location(1) depth_rows: vec2<f32>,
    @location(2) haze: vec3<f32>,
    @builtin(position) position: vec4<f32>,
) -> @location(0) vec4<f32> {
    let centre_share = 1.0 + (position.y - depth_rows.x) * depth_rows.y;
    let amount = haze.z * (1.0 - smoothstep(haze.y, haze.x, centre_share));
    // Premultiplied: the blend scales it by the frame's alpha, so it lands only where the map is.
    return vec4<f32>(color.rgb * amount, amount);
}
