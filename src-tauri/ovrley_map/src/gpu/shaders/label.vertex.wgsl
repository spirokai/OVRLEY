// Glyph quads arrive already projected: the CPU places each label's anchor in f64 and offsets the
// corners on screen, so positions pass through with the anchor's real w.
struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) texel: vec2<f32>,
    @location(1) @interpolate(flat) fill: vec4<f32>,
    @location(2) @interpolate(flat) halo: vec4<f32>,
    // font_scale, halo_width, halo_blur, opacity
    @location(3) @interpolate(flat) paint: vec4<f32>,
    // bold, is_halo
    @location(4) @interpolate(flat) pass_data: vec2<f32>,
};

@vertex
fn main(
    @location(0) position: vec4<f32>,
    @location(1) texel: vec2<u32>,
    @location(2) fill: vec4<f32>,
    @location(3) halo: vec4<f32>,
    @location(4) paint: vec4<f32>,
    @location(5) bold: f32,
    @location(6) is_halo: f32,
) -> VertexOutput {
    return VertexOutput(position, vec2<f32>(texel), fill, halo, paint, vec2<f32>(bold, is_halo));
}
