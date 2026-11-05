// Sizes here are output pixels, so the edge gamma is `0.105` over the font scale, with no pixel ratio.
@group(0) @binding(0) var atlas: texture_2d<f32>;
@group(0) @binding(1) var atlas_sampler: sampler;

// The distance the field encodes either side of the glyph's edge, in glyph pixels.
const SDF_PX: f32 = 8.0;
const EDGE_GAMMA: f32 = 0.105;
// Where the edge sits in the field: (256 - 64) / 256.
const FILL_BUFFER: f32 = 0.75;
// A Regular drawn as a Bold sits this much further out, about 0.7 glyph pixels on each side.
const BOLD_SHIFT: f32 = 0.09;

@fragment
fn main(
    @location(0) texel: vec2<f32>,
    @location(1) @interpolate(flat) fill: vec4<f32>,
    @location(2) @interpolate(flat) halo: vec4<f32>,
    @location(3) @interpolate(flat) paint: vec4<f32>,
    @location(4) @interpolate(flat) pass_data: vec2<f32>,
) -> @location(0) vec4<f32> {
    let font_scale = paint.x;
    let halo_width = paint.y;
    let halo_blur = paint.z;
    let opacity = paint.w;
    let is_halo = pass_data.y > 0.5;

    let distance = textureSample(atlas, atlas_sampler, texel / vec2<f32>(textureDimensions(atlas))).r;

    var color = fill;
    var gamma = EDGE_GAMMA / font_scale;
    var edge = FILL_BUFFER;
    if is_halo {
        color = halo;
        gamma = (halo_blur * 1.19 / SDF_PX + EDGE_GAMMA) / font_scale;
        edge = (6.0 - halo_width / font_scale) / SDF_PX;
    }
    edge = edge - pass_data.x * BOLD_SHIFT;

    let alpha = smoothstep(edge - gamma, edge + gamma, distance);
    // The layer blend wants straight alpha.
    return vec4<f32>(color.rgb, color.a * alpha * opacity);
}
