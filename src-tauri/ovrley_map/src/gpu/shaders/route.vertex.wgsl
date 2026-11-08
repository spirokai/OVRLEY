struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) v_color: vec4<f32>,
    @location(1) v_along_metres: f32,
    @location(2) @interpolate(flat) v_travelled_metres: f32,
    @location(3) @interpolate(flat) v_keep_side: f32,
};

// A ground direction as output pixels on screen, up to a scale that is the same for every direction
// at one vertex. The perspective divide's derivative, with its shared division by `w * w` dropped.
fn screen_direction(ground: vec2<f32>, center: vec4<f32>, transform: mat4x4<f32>, viewport: vec2<f32>) -> vec2<f32> {
    let clip = transform * vec4<f32>(ground, 0.0, 0.0);
    return (clip.xy * center.w - center.xy * clip.w) * viewport;
}

// Offsets along the line's normal on screen, so a pitched camera keeps its width in every direction.
// A ground normal does not project to that: normals carry through the projection by its cofactor,
// which is the ground tangent's projection turned a quarter. The projected normal only picks the side.
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
    let viewport = vec2<f32>(viewport_width, viewport_height);
    let tangent = screen_direction(vec2<f32>(-normal.y, normal.x), center, transform, viewport);
    let projected_normal = screen_direction(normal, center, transform, viewport);
    let screen_normal = vec2<f32>(-tangent.y, tangent.x);
    let side = select(1.0, -1.0, dot(screen_normal, projected_normal) < 0.0);
    let dir = normalize(screen_normal) * side;

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
