// The frame's coverage by its rounded shape, which the blend multiplies into everything drawn so far.
// The shape is symmetric, so it doesn't matter which way the target's y axis runs.
@fragment
fn main(
    @location(0) half_size: vec2<f32>,
    @location(1) radius: f32,
    @builtin(position) position: vec4<f32>,
) -> @location(0) vec4<f32> {
    let corner = abs(position.xy - half_size) - (half_size - vec2<f32>(radius));
    let distance = length(max(corner, vec2<f32>(0.0))) + min(max(corner.x, corner.y), 0.0) - radius;
    return vec4<f32>(0.0, 0.0, 0.0, clamp(0.5 - distance, 0.0, 1.0));
}
