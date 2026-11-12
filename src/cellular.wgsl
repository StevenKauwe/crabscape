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
    // Match lantern.rs `daylight`: the canyon represents the twilight zone.
    let vertical = clamp(-p.y / 9000.0, 0.0, 1.0);
    let canyon = smoothstep(5350.0, 6150.0, p.x)
        * (1.0 - smoothstep(-1500.0, 1000.0, p.y));
    let depth = max(vertical, 0.75 * canyon);
    let day = (1.0 - depth) * (1.0 - depth);
    return clamp(day + 0.9 * brightness, 0.0, 1.0);
}


// ---- painted ground: the same text in cellular.wgsl and memory.wgsl (a test keeps them equal) ----
// Six materials painted by our oil painter (tools/paint-world.sh), each a seamless tile, lit by the lantern in
// layers 0..5 and unlit in 6..11. They are read in world space (so a material looks the same wherever it is cut),
// bent a little by a slow warp and laid over a second, larger and turned reading of itself so no repeat shows.
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

// A material's painted colour at a world position, in the light there. `lod`: how many mips to drop (the pixel's
// footprint on the tile), so a zoomed-out view sees the tile's average, never sparkle.
fn painted_ground(p: vec2<f32>, layer: i32, lit_amount: f32, lod: f32) -> vec3<f32> {
    let warp = (vec2<f32>(noise(p / 83.0), noise(p / 83.0 + 31.7)) - 0.5) * 0.22;
    let uv1 = vec2<f32>(p.x, -p.y) / TILE_UNITS + warp;
    let uv2 = vec2<f32>(-p.y, p.x) / (TILE_UNITS * 1.63) + vec2<f32>(0.37, 0.61);
    return mix(
        tile_read(uv1, layer, lit_amount, lod),
        tile_read(uv2, layer, lit_amount, max(lod - 0.7, 0.0)),
        0.3
    );
}

// The shape's own light on its paint: a bright band under its top edge (lit from the top left), then a dark warm
// ink line along the edge itself. `top`: world units below the shape's top edge.
fn rim_top(c: vec3<f32>, p: vec2<f32>, top: f32) -> vec3<f32> {
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

// Is this water cell in a gap so narrow between mud cells that it reads as mud? (The simulation's cells are
// presentation here; this soft gap is drawn, not simulated.)
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

// 1 where the cell holds ground, 0 where it is water. Outside the window: ground (the floor mesh draws there).
fn solid_at(texel: vec2<i32>, dims: vec2<i32>) -> f32 {
    if any(texel < vec2<i32>(0)) || any(texel >= dims) { return 1.0; }
    return select(0.0, 1.0, textureLoad(cell_texture, texel, 0).r > 0.5 / 255.0);
}

fn cell_texel(p: vec2<f32>, dims: vec2<i32>) -> vec2<i32> {
    let uv = vec2<f32>((p.x - bounds.x) / bounds.z, 1.0 - (p.y - bounds.y) / bounds.w);
    return vec2<i32>(floor(uv * vec2<f32>(dims)));
}

fn cell_position(p: vec2<f32>, dims: vec2<i32>) -> vec2<f32> {
    return vec2<f32>((p.x - bounds.x) / bounds.z, 1.0 - (p.y - bounds.y) / bounds.w) * vec2<f32>(dims) - 0.5;
}

// How much of the ground is here, rounded between cell centres: 0 water, 1 ground, 0.5 on the painted edge.
fn coverage(p: vec2<f32>, dims: vec2<i32>) -> f32 {
    let q = cell_position(p, dims);
    let i = vec2<i32>(floor(q));
    let f = fract(q);
    return mix(
        mix(solid_at(i, dims), solid_at(i + vec2<i32>(1, 0), dims), f.x),
        mix(solid_at(i + vec2<i32>(0, 1), dims), solid_at(i + vec2<i32>(1, 1), dims), f.x),
        f.y
    );
}

// Which painted material a cell is: 0 sand, 1 silt, 2 clay, 3 pebble, 4 rubble, 5 rock. The simulation gives
// sand, mud and rock, and each sand grain's identity (a pebble when high). Mud is silt or clay, and rock bare or
// strewn with pale rubble, in broad patches of the world, which is presentation only.
fn material_layer(texel: vec2<i32>, dims: vec2<i32>, p: vec2<f32>) -> i32 {
    let data = vec2<u32>(round(textureLoad(cell_texture, texel, 0).rg * 255.0));
    switch data.x {
        // Rock, with broad patches of pale rubble (talus), never specks.
        case 1u: { return select(5, 4, noise(p / 130.0 + 7.0) > 0.7); }
        case 2u: {
            if data.y >= 248u { return 3; }
            return 0;
        }
        case 3u: {
            return select(1, 2, noise(p / 170.0) > 0.52);
        }
        default: { return 1; }
    }
}

// The cell that most holds this point, by the same rounding as the coverage.
fn dominant_layer(p: vec2<f32>, dims: vec2<i32>) -> i32 {
    let q = cell_position(p, dims);
    let i = vec2<i32>(floor(q));
    let f = fract(q);
    let pick = vec2<i32>(select(0, 1, f.x > 0.5), select(0, 1, f.y > 0.5));
    var texel = clamp(i + pick, vec2<i32>(0), dims - 1);
    // A water cell nearest: take the nearest ground cell of the four instead.
    if solid_at(texel, dims) < 0.5 {
        for (var k = 0; k < 4; k += 1) {
            let t = clamp(i + vec2<i32>(k & 1, k >> 1), vec2<i32>(0), dims - 1);
            if solid_at(t, dims) > 0.5 { texel = t; }
        }
    }
    return material_layer(texel, dims, p);
}

// The ground's own light on its paint, from the water beside it: bright where water lies up or to the left (the
// ground's lit top-left), deep where it lies down or to the right, and a dark warm ink line along the edge itself.
fn cell_rim(c: vec3<f32>, p: vec2<f32>, dims: vec2<i32>, cover: f32) -> vec3<f32> {
    let reach = 5.0;
    // Deep inside the ground (the usual case, and the costly one on a phone): four plain reads say so.
    let cu = vec2<f32>(0.0, reach);
    let cr = vec2<f32>(reach, 0.0);
    if cover > 0.999
        && solid_at(cell_texel(p + cu, dims), dims) + solid_at(cell_texel(p - cu, dims), dims)
            + solid_at(cell_texel(p - cr, dims), dims) + solid_at(cell_texel(p + cr, dims), dims) > 3.999 {
        return c;
    }
    let up = coverage(p + vec2<f32>(0.0, reach), dims);
    let down = coverage(p - vec2<f32>(0.0, reach), dims);
    let left = coverage(p - vec2<f32>(reach, 0.0), dims);
    let right = coverage(p + vec2<f32>(reach, 0.0), dims);
    let bright = (1.0 - min(up, left));
    let deep = (1.0 - min(down, right));
    var shaded = c * (1.0 + 0.5 * bright - 0.55 * deep * (1.0 - bright));
    // Ink: a dark line right at the edge, and a softer one wherever water is within reach.
    let near = 1.0 - min(min(up, down), min(left, right));
    let wobble = (noise(p / 5.0) - 0.5) * 0.2;
    let ink = max(smoothstep(0.9, 0.6, cover + wobble), 0.55 * smoothstep(0.2, 0.9, near + wobble));
    return mix(shaded, INK, ink * 0.92);
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
    let dims = vec2<i32>(textureDimensions(cell_texture));
    let cell_uv = vec2<f32>((p.x - bounds.x) / bounds.z, 1.0 - (p.y - bounds.y) / bounds.w);
    let at = clamp(vec2<i32>(floor(cell_uv * vec2<f32>(dims))), vec2<i32>(0), dims - 1);
    // The ground's edge, rounded between cell centres and broken up a little, so it reads as painted.
    let cover = coverage(p, dims);
    let edge = 0.5 + (noise(p / 11.0) - 0.5) * 0.16;
    var layer = -1;
    var solid = cover >= edge;
    if solid {
        layer = dominant_layer(p, dims);
    } else if textureLoad(cell_texture, at, 0).r == 0.0 && in_mud_gap(at, dims) {
        layer = 1;
    }
    if layer < 0 { discard; }
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
    // Each cell of ground a little different from its neighbours, and the ground itself in slow patches, so no
    // repeat shows.
    let cell = floor(p / 4.0);
    let vary = (0.97 + 0.06 * hash(cell)) * (0.9 + 0.2 * noise(p / 41.0));
    var c = painted_ground(p, layer, light_amount(p), lod) * vary;
    if solid {
        c = cell_rim(c, p, dims, cover);
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
