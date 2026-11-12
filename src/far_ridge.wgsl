// The far painted ridge (see painted_world.rs) as a flat, opaque silhouette: it is 78% receded into the water, so
// its texture barely reads. One colour (the paint's mean, lit and unlit blended and sunk into the water on the CPU
// each frame), eased into the water's colour over the foot so no edge shows. Nothing is sampled, dropped or blended.

struct FarRidgeUniform {
    colour: vec4<f32>,
    water: vec4<f32>,
};

@group(2) @binding(0) var<uniform> paint: FarRidgeUniform;

struct FragmentInput {
    @location(0) world_position: vec4<f32>,
    @location(1) world_normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
};

@fragment
fn fragment(in: FragmentInput) -> @location(0) vec4<f32> {
    return vec4<f32>(mix(paint.colour.rgb, paint.water.rgb, smoothstep(0.5, 0.88, in.uv.y)), 1.0);
}
