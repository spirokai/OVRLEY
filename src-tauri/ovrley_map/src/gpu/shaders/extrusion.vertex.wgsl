struct VertexOutput {
    @location(0) v_color: vec4<f32>,
    @builtin(position) position: vec4<f32>,
};

// MapLibre's default light: `position: [1.15, 210, 30]`, `intensity: 0.5`, anchored to the viewport.
const LIGHT_Z: f32 = 0.996;
const LIGHT_INTENSITY: f32 = 0.5;
const AMBIENT: f32 = 0.03;

// `position.z` is metres above the ground. The tile matrix scales z from metres to world units.
@vertex
fn main(
    @location(0) position: vec3<f32>,
    @location(1) normal: vec2<f32>,
    @location(2) gradient: f32,
    @location(4) translate1: vec4<f32>,
    @location(5) translate2: vec4<f32>,
    @location(6) translate3: vec4<f32>,
    @location(7) translate4: vec4<f32>,
    @location(8) color: vec4<f32>,
    @location(14) light_xy: vec2<f32>,
) -> VertexOutput {
    let final_position = mat4x4<f32>(translate1, translate2, translate3, translate4) * vec4<f32>(position, 1.0);

    // Walls have a unit normal on the ground plane. Roofs have none, and face straight up.
    let surface_normal = vec3<f32>(normal, 1.0 - length(normal));
    let light = clamp(dot(surface_normal, vec3<f32>(light_xy, LIGHT_Z)), 0.0, 1.0);
    let luminance = dot(color.rgb, vec3<f32>(0.2126, 0.7152, 0.0722));
    let highlight = max(1.0 - luminance + LIGHT_INTENSITY, 1.0);
    let directional = mix(1.0 - LIGHT_INTENSITY, highlight, light) * gradient;
    let shaded = clamp((color.rgb + vec3<f32>(AMBIENT)) * directional, vec3<f32>(0.0), vec3<f32>(1.0));

    return VertexOutput(vec4<f32>(shaded, 1.0), final_position);
}
