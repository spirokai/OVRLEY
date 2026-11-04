struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) v_color: vec4<f32>,
    @location(1) v_normal: vec2<f32>,
    @location(2) v_edges: vec2<f32>, // outer, inner
    @location(3) v_extent: f32,
    @location(4) v_blur: f32,
    @location(5) v_distance: f32,
    @location(6) v_dash: vec3<f32>,
};

// A 4096-unit tile spans 512 style pixels on its own level. `zoom_factor` scales that to the view.
const TILE_UNITS_PER_PIXEL: f32 = 8.0;
// Nearer than this share of the map centre's depth is at the near plane and beyond. It only bounds
// the width handed to the fragment shader there.
const MIN_DEPTH_SHARE: f32 = 0.02;
// How far past the antialiasing a line's geometry reaches. Under MSAA the fragment shader runs at
// the pixel centre while the samples decide coverage, so samples a pixel has outside the geometry
// would cut its antialiased edge a second time, and unevenly as the line moves across the pixel.
const SAMPLE_MARGIN: f32 = 0.5;

@vertex
fn main(
    @location(0) position: vec2<f32>,
    @location(1) normal: vec2<f32>,
    @location(2) line_distance: f32,
    @location(3) centre_w: f32,
    @location(4) translate1: vec4<f32>,
    @location(5) translate2: vec4<f32>,
    @location(6) translate3: vec4<f32>,
    @location(7) translate4: vec4<f32>,
    @location(8) color: vec4<f32>,
    @location(9) zoom_factor: f32,
    @location(10) layer: vec4<f32>, // z_index, line_width, line_blur, line_gap_width
    @location(11) viewport_width: f32,
    @location(12) viewport_height: f32,
    @location(13) translate_and_narrows: vec3<f32>, // translate, narrows_with_distance
    @location(14) dash: vec3<f32>, // on, off, cap_radius
) -> VertexOutput {
    let z_index = layer.x;
    let line_width_px = layer.y;
    let line_blur = layer.z;
    let gapwidth = layer.w * 0.5;
    let translate = translate_and_narrows.xy;

    let halfwidth = line_width_px * 0.5;

    let transform = mat4x4<f32>(translate1, translate2, translate3, translate4);

    // Transform center position to clip space
    let translated = position + translate * TILE_UNITS_PER_PIXEL * zoom_factor;
    var center = transform * vec4<f32>(translated, 0.0, 1.0);

    // A line's width holds at the map centre and narrows with distance, as on the ground. Widening
    // by a constant in clip space leaves that to the perspective divide, which also stays right for
    // a vertex behind the camera. The fragment shader works in screen pixels, so it gets the width
    // already narrowed, and the antialiasing stays one pixel at any distance.
    let width_w = mix(center.w, centre_w, translate_and_narrows.z);
    let distance_scale = width_w / max(center.w, centre_w * MIN_DEPTH_SHARE);
    let core = gapwidth + halfwidth * select(1.0, 2.0, gapwidth > 0.0);
    let blur = line_blur * distance_scale;
    // The line covers [inner, outer] pixels from its centre on both sides. Blur fades each edge
    // inwards over `blur` pixels, as MapLibre's ramp does, so an edge sits half of it in.
    let inner = select(0.0, max(gapwidth * distance_scale - blur * 0.5, 0.0), gapwidth > 0.0);
    let outer = max(core * distance_scale - blur * 0.5, inner);
    // The fragment shader filters with a box one pixel plus the blur wide.
    let fringe = select(0.0, (blur + 1.0) * 0.5 + SAMPLE_MARGIN, halfwidth != 0.0);
    let extent = outer + fringe;

    // Transform the normal direction to clip space (w=0 for direction vectors)
    let normal_clip = transform * vec4<f32>(normal, 0.0, 0.0);
    // A miter's normal is longer than 1, which reaches its tip. The fragment shader measures width
    // across the line, so it gets the normal at unit length.
    let miter = length(normal);
    let dir = normalize(normal_clip.xy) * miter;

    // Apply pixel-width offset in clip space.
    // NDC spans 2 units across the viewport, so 1 pixel = 2/viewport_px in NDC.
    // Use per-axis conversion to handle non-square viewports correctly.
    let px_to_ndc = vec2<f32>(2.0 / viewport_width, 2.0 / viewport_height);
    let clip_offset = dir * px_to_ndc * (core * width_w + (extent - core * distance_scale) * center.w);
    center = vec4<f32>(center.x + clip_offset.x, center.y + clip_offset.y, z_index, center.w);

    return VertexOutput(
        center,
        color,
        normal / miter,
        vec2<f32>(outer, inner),
        extent,
        blur,
        line_distance / (TILE_UNITS_PER_PIXEL * zoom_factor),
        dash
    );
}
