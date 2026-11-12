// A painted far ridge (see painted_world.rs): a repeating strip of rock spires, painted lit and unlit. The two
// blend from the light at the camera's depth, and the result recedes into the water's colour by distance. That
// recession is this shape's own colour, never an overlay: every pixel is the ridge's own paint or nothing.

struct RidgeUniform {
    water: vec4<f32>,
    // x: 0 far / 1 near, y: lit, z: receded into the water.
    settings: vec4<f32>,
};

@group(2) @binding(0) var<uniform> paint: RidgeUniform;
@group(2) @binding(1) var ridges: texture_2d_array<f32>;
@group(2) @binding(2) var ridges_sampler: sampler;

struct FragmentInput {
    @location(0) world_position: vec4<f32>,
    @location(1) world_normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
};

@fragment
fn fragment(in: FragmentInput) -> @location(0) vec4<f32> {
    let layer = i32(round(paint.settings.x)) * 2;
    let lit = textureSampleLevel(ridges, ridges_sampler, in.uv, layer, 0.0);
    if lit.a < 0.01 { discard; }
    let dark = textureSampleLevel(ridges, ridges_sampler, in.uv, layer + 1, 0.0);
    let own = mix(dark.rgb, lit.rgb, paint.settings.y);
    return vec4<f32>(mix(own, paint.water.rgb, paint.settings.z), lit.a);
}
