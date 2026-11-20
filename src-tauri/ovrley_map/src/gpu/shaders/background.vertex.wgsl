struct VertexOutput {
    @location(0) color: vec4<f32>,
    @builtin(position) position: vec4<f32>,
};

@vertex
fn main(
    @builtin(vertex_index) vertex_idx: u32,
    @location(0) color: vec4<f32>,
) -> VertexOutput {
    // Generate a fullscreen quad using standard 6-vertex triangle list layout
    var positions = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 1.0, -1.0),
        vec2<f32>(-1.0,  1.0),
        vec2<f32>(-1.0,  1.0),
        vec2<f32>( 1.0, -1.0),
        vec2<f32>( 1.0,  1.0)
    );

    let pos = positions[vertex_idx % 6u];

    var out: VertexOutput;
    out.position = vec4<f32>(pos, 1.0e-5, 1.0);
    out.color = color;

    return out;
}
