struct FragmentInput {
    @location(0) v_color: vec4<f32>,
    @location(1) v_along_metres: f32,
    @location(2) @interpolate(flat) v_travelled_metres: f32,
    @location(3) @interpolate(flat) v_keep_side: f32,
};

// Full coverage with no analytic edge fade: the stencil paints each sample once, so a fade would
// leave rings where sections and crossings overlap. MSAA smooths the edges.
@fragment
fn main(in: FragmentInput) -> @location(0) vec4<f32> {
    if ((in.v_along_metres - in.v_travelled_metres) * in.v_keep_side < 0.0) {
        discard;
    }
    return in.v_color;
}
