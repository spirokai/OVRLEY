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

    // Four samples per pixel on a rotated grid, stepped by how far the atlas moves per pixel, so a glyph
    // squashed by pitch or shrunk with distance is averaged over its pixel instead of point sampled.
    let uv = texel / vec2<f32>(textureDimensions(atlas));
    let uv_dx = dpdx(uv);
    let uv_dy = dpdy(uv);
    let d0 = textureSampleLevel(atlas, atlas_sampler, uv + uv_dx * 0.125 + uv_dy * 0.375, 0.0).r;
    let d1 = textureSampleLevel(atlas, atlas_sampler, uv - uv_dx * 0.375 + uv_dy * 0.125, 0.0).r;
    let d2 = textureSampleLevel(atlas, atlas_sampler, uv + uv_dx * 0.375 - uv_dy * 0.125, 0.0).r;
    let d3 = textureSampleLevel(atlas, atlas_sampler, uv - uv_dx * 0.125 - uv_dy * 0.375, 0.0).r;
    // How much the field changes across one pixel. Foreshortening makes it change faster across the
    // squashed axis than the font scale, which assumes an upright glyph, accounts for.
    let centre = textureSampleLevel(atlas, atlas_sampler, uv, 0.0).r;
    let field_step = length(vec2<f32>(dpdx(centre), dpdy(centre)));

    var color = fill;
    var gamma = EDGE_GAMMA / font_scale;
    var edge = FILL_BUFFER;
    if is_halo {
        color = halo;
        gamma = (halo_blur * 1.19 / SDF_PX + EDGE_GAMMA) / font_scale;
        edge = (6.0 - halo_width / font_scale) / SDF_PX;
    }
    edge = edge - pass_data.x * BOLD_SHIFT;
    // Each sample's edge ramp spans at least the field's change across a pixel, so the edge never steps.
    gamma = max(gamma, 0.5 * field_step);

    let alpha = 0.25 * (smoothstep(edge - gamma, edge + gamma, d0) + smoothstep(edge - gamma, edge + gamma, d1)
        + smoothstep(edge - gamma, edge + gamma, d2) + smoothstep(edge - gamma, edge + gamma, d3));
    // The layer blend wants straight alpha.
    return vec4<f32>(color.rgb, color.a * alpha * opacity);
}
