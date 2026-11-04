struct FragmentInput {
    @location(0) v_color: vec4<f32>,
    @location(1) v_normal: vec2<f32>,
    @location(2) v_edges: vec2<f32>, // outer, inner
    @location(3) v_extent: f32,
    @location(4) v_blur: f32,
    @location(5) v_distance: f32,
    @location(6) v_dash: vec3<f32>,
};

struct Output {
    @location(0) out_color: vec4<f32>,
};

// How much of [0, x] dashes cover, with a dash at the start of every period.
fn dashes_up_to(x: f32, on: f32, period: f32) -> f32 {
    let periods = floor(x / period);
    return periods * on + min(x - periods * period, on);
}

@fragment
fn main(in: FragmentInput) -> Output {
    // This pixel's distance from the line's centre, in pixels.
    let dist = length(in.v_normal) * in.v_extent;

    // The share of a box one pixel plus the blur wide, centred here, that the line covers on either
    // side of its centre. Unlike a ramp at each edge, this keeps a line thinner than its box at its
    // true weight wherever it falls on the pixel grid, so it neither shimmers nor breaks up as it moves.
    let outer = in.v_edges.x;
    let inner = in.v_edges.y;
    let half_box = (in.v_blur + 1.0) * 0.5;
    let near = dist - half_box;
    let far = dist + half_box;
    let alpha = (clamp(far, inner, outer) - clamp(near, inner, outer) + clamp(-near, inner, outer) - clamp(-far, inner, outer))
        / (2.0 * half_box);

    // Dashes are averaged over this pixel's length along the line, so they stay smooth as a pitched
    // view compresses them, and fade to their average where they get smaller than a pixel. Outside
    // this branch to keep the derivative in uniform control flow.
    let pixel_length = max(fwidth(in.v_distance), 1e-4);
    let period = in.v_dash.x + in.v_dash.y;
    var coverage = 1.0;
    if period > 0.0 {
        // A round cap lengthens a dash the most along the line's centre.
        let across = min(length(in.v_normal), 1.0);
        let reach = in.v_dash.z * sqrt(1.0 - across * across);
        let on = min(in.v_dash.x + 2.0 * reach, period);
        let x = in.v_distance + reach;
        let half_pixel = pixel_length * 0.5;
        coverage = (dashes_up_to(x + half_pixel, on, period) - dashes_up_to(x - half_pixel, on, period)) / pixel_length;
    }

    // Output non-premultiplied alpha: the blend state (SrcAlpha, OneMinusSrcAlpha)
    // handles the premultiplication. Using v_color * alpha here would double-apply alpha.
    return Output(vec4<f32>(in.v_color.rgb, in.v_color.a * alpha * coverage));
}
