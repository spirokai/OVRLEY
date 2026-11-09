struct VertexOutput {
    @location(0) v_opacity: f32,
    @location(1) v_pattern_position: vec2<f32>,
    @builtin(position) position: vec4<f32>,
};

// A 4096-unit tile spans 512 style pixels on its own level. `zoom_factor` scales that to the view.
const TILE_UNITS_PER_PIXEL: f32 = 8.0;

@vertex
fn main(
    @location(0) position: vec2<f32>,
    @location(4) translate1: vec4<f32>,
    @location(5) translate2: vec4<f32>,
    @location(6) translate3: vec4<f32>,
    @location(7) translate4: vec4<f32>,
    @location(8) color: vec4<f32>,
    @location(9) zoom_factor: f32,
    @location(10) z_index: f32,
    @location(11) translate: vec2<f32>,
    @location(12) pattern_origin: vec2<f32>,
) -> VertexOutput {
    let units_per_pixel = TILE_UNITS_PER_PIXEL * zoom_factor;
    let translated = position + translate * units_per_pixel;

    // Fills lie on the ground: the layer number orders draws and is never a world height.
    var final_position = mat4x4<f32>(translate1, translate2, translate3, translate4) * vec4<f32>(translated, 0.0, 1.0);
    final_position.z = z_index;

    // Style pixels from a shared origin, so the pattern runs unbroken across tiles.
    return VertexOutput(color.a, pattern_origin + position / units_per_pixel, final_position);
}
