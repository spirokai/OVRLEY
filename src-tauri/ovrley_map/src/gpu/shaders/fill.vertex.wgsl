struct VertexOutput {
    @location(0) v_color: vec4<f32>,
    @builtin(position) position: vec4<f32>,
};

// A 4096-unit tile spans 512 style pixels on its own level. `zoom_factor` scales that to the view.
const TILE_UNITS_PER_PIXEL: f32 = 8.0;

@vertex
fn main(
    @location(0) position: vec2<f32>,
    @location(1) normal: vec2<f32>,
    @location(4) translate1: vec4<f32>,
    @location(5) translate2: vec4<f32>,
    @location(6) translate3: vec4<f32>,
    @location(7) translate4: vec4<f32>,
    @location(8) color: vec4<f32>,
    @location(9) zoom_factor: f32,
    @location(10) z_index: f32,
    @location(11) translate: vec2<f32>,
) -> VertexOutput {
    let width = 3.0 * zoom_factor;
    let translated = position + translate * TILE_UNITS_PER_PIXEL * zoom_factor;

    // Fills lie on the ground: the layer number orders draws and is never a world height.
    var final_position = mat4x4<f32>(translate1, translate2, translate3, translate4) * vec4<f32>(translated + normal * width, 0.0, 1.0);
    final_position.z = z_index;

    return VertexOutput(color, final_position);
}
