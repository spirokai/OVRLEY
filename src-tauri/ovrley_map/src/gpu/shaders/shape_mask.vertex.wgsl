struct VertexOutput {
    @location(0) half_size: vec2<f32>,
    @location(1) radius: f32,
    @builtin(position) position: vec4<f32>,
};

@vertex
fn main(
    @builtin(vertex_index) vertex_idx: u32,
    @location(0) half_size: vec2<f32>,
    @location(1) radius: f32,
) -> VertexOutput {
    var positions = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 1.0, -1.0),
        vec2<f32>(-1.0,  1.0),
        vec2<f32>(-1.0,  1.0),
        vec2<f32>( 1.0, -1.0),
        vec2<f32>( 1.0,  1.0)
    );

    var out: VertexOutput;
    out.position = vec4<f32>(positions[vertex_idx % 6u], 1.0e-5, 1.0);
    out.half_size = half_size;
    out.radius = radius;
    return out;
}
