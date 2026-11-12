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

@group(2) @binding(0) var<uniform> material: MemoryUniform;
@group(2) @binding(1) var memory_texture: texture_2d<f32>;
@group(2) @binding(2) var memory_sampler: sampler;
@group(2) @binding(3) var<uniform> bounds: vec4<f32>;
@group(2) @binding(4) var cell_texture: texture_2d<f32>;
@group(2) @binding(5) var cell_sampler: sampler;
@group(2) @binding(6) var<uniform> crab_bounds: vec4<f32>;
// The painted ground's tiles (painted_world.rs).
@group(2) @binding(7) var tiles: texture_2d_array<f32>;
@group(2) @binding(8) var tiles_sampler: sampler;

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

// How lit the ground is here, 0 (dark water's own dark) to 1 (the lantern or the sun on it).
fn light_amount(p: vec2<f32>) -> f32 {
    var brightness = 0.0;
    for (var i = 0; i < 8; i += 1) {
        let light = material.lights[i].geometry;
        if light.w > light.z {
            let amount = 1.0 - smoothstep(light.z, light.w, distance(p, light.xy));
            brightness = max(brightness, amount * material.lights[i].luminosity);
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
// Cheap on purpose (software rendering and phones pay per pixel): ONE slow noise per pixel, passed in as `patch`,
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

// A material's painted colour at a world position, in the light there. `patch`: a slow noise (0 to 1) at the
// position. `lod`: how many mips to drop (the pixel's footprint on the tile), so a zoomed-out view sees the tile's
// average, never sparkle.
fn painted_ground(p: vec2<f32>, layer: i32, lit_amount: f32, lod: f32, patch: f32) -> vec3<f32> {
    let uv = vec2<f32>(p.x, -p.y) / TILE_UNITS + (patch - 0.5) * vec2<f32>(0.3, 0.22);
    return tile_read(uv, layer, lit_amount, lod) * (0.9 + 0.2 * patch);
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

fn is_mud(texel: vec2<i32>, dims: vec2<i32>) -> bool {
    if any(texel < vec2<i32>(0)) || any(texel >= dims) { return false; }
    return textureLoad(cell_texture, texel, 0).r > 2.5 / 255.0;
}

// Is this water cell in a gap so narrow between mud cells that it reads as mud? (Presentation only: drawn, not
// simulated.)
fn in_mud_gap(at: vec2<i32>, dims: vec2<i32>) -> bool {
    let x = u32(at.x);
    let y = u32(dims.y - 1 - at.y);
    let local = vec2<f32>(f32(x), f32(y));
    if all(local >= crab_bounds.xy) && all(local <= crab_bounds.zw) { return false; }
    let dx = vec2<i32>(1, 0);
    let dy = vec2<i32>(0, 1);
    let l = is_mud(at - dx, dims);
    let r = is_mud(at + dx, dims);
    let a = is_mud(at - dy, dims);
    let b = is_mud(at + dy, dims);
    if !(l || r || a || b) { return false; }
    let left = l || is_mud(at - 2 * dx, dims);
    let right = r || is_mud(at + 2 * dx, dims);
    let above = a || is_mud(at - 2 * dy, dims);
    let below = b || is_mud(at + 2 * dy, dims);
    return (left && right && (above || below)) || (above && below && (left || right));
}

// Which painted material a cell is: 0 sand, 1 silt, 2 clay, 3 pebble, 4 rubble, 5 rock. The simulation gives sand,
// mud, rock and each sand grain's identity (a pebble when high). Mud is silt or clay, and rock bare or strewn with
// pale rubble, in broad patches of the world (`patch`, the pixel's slow noise): presentation only.
fn material_layer(data: vec2<u32>, patch: f32) -> i32 {
    switch data.x {
        case 1u: { return select(5, 4, patch > 0.7); }
        case 2u: { return select(0, 3, data.y >= 248u); }
        case 3u: { return select(1, 2, patch > 0.52); }
        default: { return 1; }
    }
}

// The ground's own light at the edge of its cell, from the two cells that decide it (the one above and the one to
// the left, read only in the half of the cell that touches them): a bright band under a top or left edge of water,
// with a dark warm ink line right at it. `f`: where in the cell, 0 to 1 from its top left. No bottom or right shade.
fn cell_rim(c: vec3<f32>, p: vec2<f32>, at: vec2<i32>, dims: vec2<i32>, f: vec2<f32>) -> vec3<f32> {
    var edge = 1.0;
    if f.y < 0.5 && at.y > 0 && textureLoad(cell_texture, at - vec2<i32>(0, 1), 0).r < 0.5 / 255.0 {
        edge = min(edge, f.y * 2.0);
    }
    if f.x < 0.5 && at.x > 0 && textureLoad(cell_texture, at - vec2<i32>(1, 0), 0).r < 0.5 / 255.0 {
        edge = min(edge, f.x * 2.0);
    }
    if edge >= 1.0 { return c; }
    let d = edge + (noise(p / 5.0) - 0.5) * 0.3;
    return mix(c * (1.0 + 0.4 * (1.0 - smoothstep(0.3, 1.0, d))), INK, 0.92 * (1.0 - smoothstep(0.1, 0.4, d)));
}

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
    // The detailed ocean field is water, not a blue sheet over the royal region.
    if p.y >= 5000.0 { discard; }
    let cell_uv = vec2<f32>((p.x - bounds.x) / bounds.z, 1.0 - (p.y - bounds.y) / bounds.w);
    let dims = vec2<i32>(textureDimensions(cell_texture));
    let scaled = cell_uv * vec2<f32>(dims);
    let at = clamp(vec2<i32>(floor(scaled)), vec2<i32>(0), dims - 1);
    let data = vec2<u32>(round(textureLoad(cell_texture, at, 0).rg * 255.0));
    // Its own cell's material; a water cell in a narrow mud gap reads as silt.
    let solid = data.x != 0u;
    if !solid && !in_mud_gap(at, dims) { discard; }
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
    if m < 0.004 {
        return vec4<f32>(water_colour(p), 1.0);
    }
    let patch = noise(p / 90.0);
    var c = painted_ground(p, material_layer(data, patch), light_amount(p), lod, patch);
    if solid {
        c = cell_rim(c, p, at, dims, scaled - floor(scaled));
    }
    if m >= 0.995 {
        return vec4<f32>(c, 1.0);
    }
    // Noise only where there is memory, most at its fading edge.
    let n = noise(p / 30.0) * 0.6 + noise(p / 9.0) * 0.4;
    let k = clamp(m + (n - 0.5) * 0.6 * min(m * 4.0, 1.0) * (1.0 - m), 0.0, 1.0);
    let grey = vec3<f32>(dot(c, vec3<f32>(0.3, 0.59, 0.11)));
    let faded = mix(grey, c, k * k);
    let shown = mix(water_colour(p), faded, smoothstep(0.04, 0.95, k));
    return vec4<f32>(shown, 1.0);
}
