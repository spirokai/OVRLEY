struct Output {
    @location(0) out_color: vec4<f32>,
};

@group(0) @binding(0) var pattern: texture_2d<f32>;
@group(0) @binding(1) var pattern_sampler: sampler;

// Every bundled image is @2x, so its texels are half a style pixel.
const TEXELS_PER_PIXEL: f32 = 2.0;

@fragment
fn main(
    @location(0) v_opacity: f32,
    @location(1) v_pattern_position: vec2<f32>,
) -> Output {
    let size = vec2<f32>(textureDimensions(pattern));
    let texel = textureSample(pattern, pattern_sampler, v_pattern_position * TEXELS_PER_PIXEL / size);

    // The texture is premultiplied for filtering, and the layer blend wants straight alpha.
    return Output(vec4<f32>(texel.rgb / max(texel.a, 1e-4), texel.a * v_opacity));
}
