struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) v_color: vec4<f32>,
    @location(1) v_along_metres: f32,
    @location(2) @interpolate(flat) v_travelled_metres: f32,
    @location(3) @interpolate(flat) v_keep_side: f32,
};

// The road line shader's offset in clip space, at the route's own width. Output pixels throughout.
@vertex
fn main(
    @location(0) position: vec2<f32>,
    @location(1) normal: vec2<f32>,
    @location(2) along_metres: f32,
    @location(4) translate1: vec4<f32>,
    @location(5) translate2: vec4<f32>,
    @location(6) translate3: vec4<f32>,
    @location(7) translate4: vec4<f32>,
    @location(8) color: vec4<f32>,
    @location(9) viewport_width: f32,
    @location(10) viewport_height: f32,
    @location(11) line_width: f32,
    @location(12) travelled_metres: f32,
    @location(13) keep_side: f32,
) -> VertexOutput {
    let transform = mat4x4<f32>(translate1, translate2, translate3, translate4);
    let center = transform * vec4<f32>(position, 0.0, 1.0);
    let normal_clip = transform * vec4<f32>(normal, 0.0, 0.0);
    let dir = normalize(normal_clip.xy);

    let halfwidth = line_width * 0.5;
    let offset = vec2<f32>(
        dir.x * halfwidth * (2.0 / viewport_width) * center.w,
        dir.y * halfwidth * (2.0 / viewport_height) * center.w,
    );

    // Ground layers order by draw order, so depth only has to stay inside the clip volume.
    return VertexOutput(
        vec4<f32>(center.x + offset.x, center.y + offset.y, 0.0, center.w),
        color,
        along_metres,
        travelled_metres,
        keep_side,
    );
}
