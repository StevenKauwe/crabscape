// The level's shapes as the crab remembers them (see memory.rs).
//
// Each shape keeps its own colour where the crab perceives it now. Elsewhere
// the colour comes from memory: greyer, softer at the edges and sinking into
// the water the less clearly the place is remembered; never seen, it is only
// water. The memory grid is a small texture (one texel per 5 cm cell), read
// with linear filtering so its edges are soft, and broken up a little with
// noise so the edges look remembered rather than cut.

struct Light {
    // Position, inner radius, outer radius. Zero radius is unused.
    geometry: vec4<f32>,
    luminosity: f32,
};

struct MemoryUniform {
    color: vec4<f32>,
    water: vec4<f32>,
    // The grid in world units: left, bottom, width, height.
    grid: vec4<f32>,
    lights: array<Light, 8>,
};

struct MemoryGlobals {
    grid: vec4<f32>,
    lights: array<Light, 8>,
};
@group(2) @binding(3) var<uniform> globals: MemoryGlobals;
// The painted ground's tiles (painted_world.rs).
@group(2) @binding(4) var tiles: texture_2d_array<f32>;
@group(2) @binding(5) var tiles_sampler: sampler;

@group(2) @binding(0) var<uniform> material: MemoryUniform;
@group(2) @binding(1) var memory_texture: texture_2d<f32>;
@group(2) @binding(2) var memory_sampler: sampler;

struct FragmentInput {
    @location(0) world_position: vec4<f32>,
    @location(1) world_normal: vec3<f32>,
    // A painted shape's y is its distance below its top edge, in world units (level.rs).
    @location(2) uv: vec2<f32>,
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
        let light = globals.lights[i].geometry;
        if light.w > light.z {
            let amount = 1.0 - smoothstep(light.z, light.w, distance(p, light.xy));
            brightness = max(brightness, amount * globals.lights[i].luminosity);
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
    // Scale the lamp contribution before the final colour limit, not a
    // blend already clamped to one. Daylight and unlit water stay unchanged.
    let exposure = daylight + max(3.0 - daylight, 0.0) * brightness;
    let strongest_channel = max(max(c.r, c.g), max(c.b, 0.0001));
    // Preserve the shape's colour even for pale shells: never clip one
    // channel before the others when lifting its brightness.
    return c * min(exposure, 1.0 / strongest_channel);
}

// How lit the ground is here, 0 (dark water's own dark) to 1 (the lantern or the sun on it).
fn light_amount(p: vec2<f32>) -> f32 {
    var brightness = 0.0;
    for (var i = 0; i < 8; i += 1) {
        let light = globals.lights[i].geometry;
        if light.w > light.z {
            let amount = 1.0 - smoothstep(light.z, light.w, distance(p, light.xy));
            brightness = max(brightness, amount * globals.lights[i].luminosity);
        }
    }
    let vertical = clamp(-p.y / 9000.0, 0.0, 1.0);
    let canyon = smoothstep(5350.0, 6150.0, p.x)
        * (1.0 - smoothstep(-1500.0, 1000.0, p.y));
    let depth = max(vertical, 0.75 * canyon);
    let day = (1.0 - depth) * (1.0 - depth);
    return clamp(day + 0.9 * brightness, 0.0, 1.0);
}

// ---- painted ground: the same text in cellular.wgsl and memory.wgsl (a test keeps them equal) ----
// Six materials painted by our oil painter (tools/paint-world.sh), each a seamless tile, lit by the lantern in
// layers 0..5 and unlit in 6..11. They are read in world space, so a material looks the same wherever it is cut.
// Cheap on purpose (software rendering and phones pay per pixel): ONE slow noise per pixel, passed in as `blot`,
// bends the reading, tints the ground in broad patches and (in cellular.wgsl) picks silt or clay; one tile
// fetch where the light is all lit or all dark, two only between.
const TILE_UNITS: f32 = 104.0;
const TILE_TEXELS: f32 = 96.0;
const INK: vec3<f32> = vec3<f32>(0.011, 0.008, 0.007);

fn tile_read(uv: vec2<f32>, layer: i32, lit_amount: f32, lod: f32) -> vec3<f32> {
    var c = vec3<f32>(0.0);
    if lit_amount < 0.999 {
        c += (1.0 - lit_amount) * textureSampleLevel(tiles, tiles_sampler, uv, layer + 6, lod).rgb;
    }
    if lit_amount > 0.001 {
        c += lit_amount * textureSampleLevel(tiles, tiles_sampler, uv, layer, lod).rgb;
    }
    return c;
}

// A material's painted colour at a world position, in the light there. `blot`: a slow noise (0 to 1) at the
// position. `lod`: how many mips to drop (the pixel's footprint on the tile), so a zoomed-out view sees the tile's
// average, never sparkle.
fn painted_ground(p: vec2<f32>, layer: i32, lit_amount: f32, lod: f32, blot: f32) -> vec3<f32> {
    let uv = vec2<f32>(p.x, -p.y) / TILE_UNITS + (blot - 0.5) * vec2<f32>(0.3, 0.22);
    return tile_read(uv, layer, lit_amount, lod) * (0.9 + 0.2 * blot);
}

// The shape's own light on its paint: a bright band under its top edge (lit from the top left), then a dark warm
// ink line along the edge itself. `top`: world units below the shape's top edge. One noise, at the edge only.
fn rim_top(c: vec3<f32>, p: vec2<f32>, top: f32) -> vec3<f32> {
    if top > 9.0 { return c; }
    let d = top + (noise(p / 6.0) - 0.5) * 1.6;
    let lit_band = 1.0 - smoothstep(2.0, 9.0, d);
    let ink = 1.0 - smoothstep(0.7, 2.5, d);
    return mix(c * (1.0 + 0.4 * lit_band), INK, ink * 0.92);
}
// ---- end painted ground ----

fn water_colour(p: vec2<f32>) -> vec3<f32> {
    if p.y >= 5000.0 {
        // Match the existing surface sky so unknown castle shapes never reveal themselves.
        let horizon = vec3<f32>(0.66, 0.82, 0.86);
        let sky = vec3<f32>(0.39, 0.66, 0.80);
        let a = pow((horizon + 0.055) / 1.055, vec3<f32>(2.4));
        let b = pow((sky + 0.055) / 1.055, vec3<f32>(2.4));
        return mix(a, b, clamp((p.y - 5000.0) / 350.0, 0.0, 1.0));
    }
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
    // World units a pixel spans, for the tile's mip (taken before any branch: it needs derivatives).
    let footprint = max(length(dpdx(p)), length(dpdy(p)));
    let lod = clamp(log2(max(footprint / (TILE_UNITS / TILE_TEXELS), 1.0)), 0.0, 6.0);
    let uv = vec2<f32>(
        (p.x - globals.grid.x) / globals.grid.z,
        1.0 - (p.y - globals.grid.y) / globals.grid.w
    );
    // Four taps half a cell apart: softer, rounder edges than the grid.
    let texel = 0.5 / vec2<f32>(textureDimensions(memory_texture));
    let m = 0.25 * (
        textureSample(memory_texture, memory_sampler, uv + vec2<f32>(texel.x, texel.y)).r
        + textureSample(memory_texture, memory_sampler, uv + vec2<f32>(-texel.x, texel.y)).r
        + textureSample(memory_texture, memory_sampler, uv + vec2<f32>(texel.x, -texel.y)).r
        + textureSample(memory_texture, memory_sampler, uv + vec2<f32>(-texel.x, -texel.y)).r
    );
    if m < 0.004 {
        return vec4<f32>(water_colour(p), 1.0);
    }
    // The shape in the light here: a painted one (its colour's alpha names its tile) or a plain colour.
    var c: vec3<f32>;
    var painted = false;
    if material.color.a < 0.99 {
        painted = true;
        let layer = i32(round(material.color.a * 10.0)) - 1;
        c = rim_top(painted_ground(p, layer, light_amount(p), lod, noise(p / 83.0)), p, in.uv.y);
    } else {
        c = material.color.rgb;
    }
    if m >= 0.995 {
        return vec4<f32>(select(lit(c, p), c, painted), 1.0);
    }
    // Noise only where there is memory, most at its fading edge.
    let n = noise(p / 30.0) * 0.6 + noise(p / 9.0) * 0.4;
    let k = clamp(m + (n - 0.5) * 0.6 * min(m * 4.0, 1.0) * (1.0 - m), 0.0, 1.0);
    let grey = vec3<f32>(dot(c, vec3<f32>(0.3, 0.59, 0.11)));
    let faded = mix(grey, c, k * k);
    let shown = mix(water_colour(p), select(lit(faded, p), faded, painted), smoothstep(0.04, 0.95, k));
    return vec4<f32>(shown, 1.0);
}
