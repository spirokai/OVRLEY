struct FragmentInput {
    @location(0) v_color: vec4<f32>,
    @location(1) v_normal: vec2<f32>,
    @location(2) v_width2: vec2<f32>,
    @location(3) v_gamma_scale: f32,
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
    // Calculate the distance of the pixel from the line in pixels
    let dist = length(in.v_normal) * in.v_width2.x;

    let pixel_ratio = 1.0;

    // Calculate the antialiasing fade factor
    let blur2 = (in.v_blur + (1.0 / pixel_ratio)) * in.v_gamma_scale;
    let denom = max(blur2, 1e-6);
    let alpha = clamp(min(dist - (in.v_width2.y - blur2), in.v_width2.x - dist) / denom, 0.0, 1.0);

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
