struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) v_color: vec4<f32>,
    @location(1) v_normal: vec2<f32>,
    @location(2) v_width2: vec2<f32>,
    @location(3) v_gamma_scale: f32,
    @location(4) v_blur: f32,
    @location(5) v_distance: f32,
    @location(6) v_dash: vec3<f32>,
};

// A 4096-unit tile spans 512 style pixels on its own level. `zoom_factor` scales that to the view.
const TILE_UNITS_PER_PIXEL: f32 = 8.0;

@vertex
fn main(
    @location(0) position: vec2<f32>,
    @location(1) normal: vec2<f32>,
    @location(2) line_distance: f32,
    @location(4) translate1: vec4<f32>,
    @location(5) translate2: vec4<f32>,
    @location(6) translate3: vec4<f32>,
    @location(7) translate4: vec4<f32>,
    @location(8) color: vec4<f32>,
    @location(9) zoom_factor: f32,
    @location(10) layer: vec4<f32>, // z_index, line_width, line_blur, line_gap_width
    @location(11) viewport_width: f32,
    @location(12) viewport_height: f32,
    @location(13) translate: vec2<f32>,
    @location(14) dash: vec3<f32>, // on, off, cap_radius
) -> VertexOutput {
    let z_index = layer.x;
    let line_width_px = layer.y;
    let line_blur = layer.z;
    let gapwidth = layer.w * 0.5;

    let halfwidth = line_width_px * 0.5;
    let pixel_ratio = 1.0;
    let antialiasing = (1.0 / pixel_ratio) * 0.5;

    let inset = gapwidth + select(0.0, antialiasing, gapwidth > 0.0);
    let outset = gapwidth + halfwidth * select(1.0, 2.0, gapwidth > 0.0) +
                 select(0.0, antialiasing, halfwidth != 0.0);

    let transform = mat4x4<f32>(translate1, translate2, translate3, translate4);

    // Transform center position to clip space
    let translated = position + translate * TILE_UNITS_PER_PIXEL * zoom_factor;
    var center = transform * vec4<f32>(translated, 0.0, 1.0);

    // Transform the normal direction to clip space (w=0 for direction vectors)
    let normal_clip = transform * vec4<f32>(normal, 0.0, 0.0);
    // A miter's normal is longer than 1, which reaches its tip. The fragment shader measures width
    // across the line, so it gets the normal at unit length.
    let miter = length(normal);
    let dir = normalize(normal_clip.xy) * miter;

    // Apply pixel-width offset in clip space.
    // NDC spans 2 units across the viewport, so 1 pixel = 2/viewport_px in NDC.
    // Multiply by center.w to compensate for the perspective divide.
    // Use per-axis conversion to handle non-square viewports correctly.
    let px_to_clip_x = (2.0 / viewport_width) * center.w;
    let px_to_clip_y = (2.0 / viewport_height) * center.w;
    let clip_offset = vec2<f32>(dir.x * outset * px_to_clip_x, dir.y * outset * px_to_clip_y);
    center = vec4<f32>(center.x + clip_offset.x, center.y + clip_offset.y, z_index, center.w);

    return VertexOutput(
        center,
        color,
        normal / miter,
        vec2<f32>(outset, inset),
        1.0,
        line_blur,
        line_distance / (TILE_UNITS_PER_PIXEL * zoom_factor),
        dash
    );
}
