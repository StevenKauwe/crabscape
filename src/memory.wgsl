// The level's shapes as the crab remembers them (see memory.rs).
//
// Each shape keeps its own colour where the crab perceives it now. Elsewhere
// the colour comes from memory: greyer, softer at the edges and sinking into
// the water the less clearly the place is remembered; never seen, it is only
// water. The memory grid is a small texture (one texel per 5 cm cell), read
// with linear filtering so its edges are soft, and broken up a little with
// noise so the edges look remembered rather than cut.

struct MemoryUniform {
    color: vec4<f32>,
    water: vec4<f32>,
    // The grid in world units: left, bottom, width, height.
    grid: vec4<f32>,
    // Each light: position, inner radius, outer radius. Zero radius is unused.
    lights: array<vec4<f32>, 8>,
};

@group(2) @binding(0) var<uniform> material: MemoryUniform;
@group(2) @binding(1) var memory_texture: texture_2d<f32>;
@group(2) @binding(2) var memory_sampler: sampler;

struct FragmentInput {
    @location(0) world_position: vec4<f32>,
};

// A cheap repeatable hash (Dave Hoskins' hash12).
fn hash(p: vec2<f32>) -> f32 {
    var p3 = fract(vec3<f32>(p.x, p.y, p.x) * 0.1031);
    p3 += dot(p3, p3.yzx + 33.33);
    return fract((p3.x + p3.y) * p3.z);
}

fn noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    return mix(
        mix(hash(i), hash(i + vec2<f32>(1.0, 0.0)), u.x),
        mix(hash(i + vec2<f32>(0.0, 1.0)), hash(i + vec2<f32>(1.0, 1.0)), u.x),
        u.y
    );
}

fn lit(c: vec3<f32>, p: vec2<f32>) -> vec3<f32> {
    var brightness = 0.0;
    for (var i = 0; i < 8; i += 1) {
        let light = material.lights[i];
        if light.w > light.z {
            let amount = 1.0 - smoothstep(light.z, light.w, distance(p, light.xy));
            brightness = max(brightness, amount);
        }
    }
    // Match lantern.rs `daylight`: the canyon represents the twilight
    // zone although the game's vertical distances are compressed.
    let vertical = clamp(-p.y / 9000.0, 0.0, 1.0);
    let canyon = smoothstep(5350.0, 6150.0, p.x)
        * (1.0 - smoothstep(-1500.0, 1000.0, p.y));
    let depth = max(vertical, 0.75 * canyon);
    let daylight = mix(0.12, 6.0, (1.0 - depth) * (1.0 - depth));
    // The terrain palette was painted for a dark seabed; its unlit colours
    // alone are too dim to read when the yellow wash is removed. Lift the
    // shape's own colour in the lantern, without tinting or softening it.
    let exposure = max(daylight, mix(daylight, 3.0, brightness));
    let strongest_channel = max(max(c.r, c.g), max(c.b, 0.0001));
    // Preserve the shape's colour even for pale shells: never clip one
    // channel before the others when lifting its brightness.
    return c * min(exposure, 1.0 / strongest_channel);
}

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
    let p = in.world_position.xy;
    let uv = vec2<f32>(
        (p.x - material.grid.x) / material.grid.z,
        1.0 - (p.y - material.grid.y) / material.grid.w
    );
    // Four taps half a cell apart: softer, rounder edges than the grid.
    let texel = 0.5 / vec2<f32>(textureDimensions(memory_texture));
    let m = 0.25 * (
        textureSample(memory_texture, memory_sampler, uv + vec2<f32>(texel.x, texel.y)).r
        + textureSample(memory_texture, memory_sampler, uv + vec2<f32>(-texel.x, texel.y)).r
        + textureSample(memory_texture, memory_sampler, uv + vec2<f32>(texel.x, -texel.y)).r
        + textureSample(memory_texture, memory_sampler, uv + vec2<f32>(-texel.x, -texel.y)).r
    );
    let c = material.color.rgb;
    if m >= 0.995 {
        return vec4<f32>(lit(c, p), 1.0);
    }
    if m < 0.004 {
        return vec4<f32>(water_colour(p), 1.0);
    }
    // Noise only where there is memory, most at its fading edge.
    let n = noise(p / 30.0) * 0.6 + noise(p / 9.0) * 0.4;
    let k = clamp(m + (n - 0.5) * 0.6 * min(m * 4.0, 1.0) * (1.0 - m), 0.0, 1.0);
    let grey = vec3<f32>(dot(c, vec3<f32>(0.3, 0.59, 0.11)));
    let faded = mix(grey, c, k * k);
    let shown = mix(water_colour(p), lit(faded, p), smoothstep(0.04, 0.95, k));
    return vec4<f32>(shown, 1.0);
}
