// Underwater background, shaded at each world position rather than using the
// crab's current depth for the whole camera view. Match memory.rs::water_at.
struct FragmentInput {
    @location(0) world_position: vec4<f32>,
};

fn water_colour(p: vec2<f32>) -> vec3<f32> {
    let surface = clamp(p.y / 5000.0, 0.0, 1.0);
    let vertical = clamp(-p.y / 9000.0, 0.0, 1.0);
    let canyon = smoothstep(5350.0, 6150.0, p.x)
        * (1.0 - smoothstep(-1500.0, 1000.0, p.y));
    let depth = max(vertical, 0.75 * canyon);
    let shallow = mix(vec3<f32>(0.07, 0.22, 0.26), vec3<f32>(0.16, 0.38, 0.44), surface);
    let c = mix(shallow, vec3<f32>(0.005, 0.016, 0.022), depth);
    return select(c / 12.92, pow((c + 0.055) / 1.055, vec3<f32>(2.4)), c > vec3<f32>(0.04045));
}

@fragment
fn fragment(in: FragmentInput) -> @location(0) vec4<f32> {
    return vec4<f32>(water_colour(in.world_position.xy), 1.0);
}
