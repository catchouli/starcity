// Terrain and water shader.
//
// Vertices are 4 x u16: block-grid position plus a data word
// (bits 0-3 material, bit 4 = top edge of a side face).
// Normals come from screen-space derivatives (flat shading), so nothing
// below may branch on varyings before sampling.

struct Globals {
    view_proj: mat4x4<f32>,
    camera_pos: vec4<f32>,
    sun_dir: vec4<f32>,
    // x = block height, y = grid strength, z = texture repeats per tile, w = wireframe (0/1)
    params: vec4<f32>,
    // x = wireframe only (hide shading), y = highlight kind (0 none, 1 cursor, 2 bulldoze),
    // z = drag preview shows track (1) or road (0) pieces
    options: vec4<f32>,
    // Highlighted tile rectangle: min x, min z, max x, max z (world units).
    highlight: vec4<f32>,
    // Clouds: altitude (world units), field size (tiles), opacity.
    clouds: vec4<f32>,
    // Cursor in render-target pixels (xy), fade radius in pixels, 1 if over the view.
    cursor: vec4<f32>,
    // Endless ocean around the map: map size (tiles), sea surface y, seabed y,
    // how far the ocean extends past the map (tiles).
    ocean: vec4<f32>,
};

@group(0) @binding(0) var<uniform> g: Globals;
@group(0) @binding(1) var textures: texture_2d_array<f32>;
@group(0) @binding(2) var tex_sampler: sampler;
// Per-tile infrastructure bits: r = roads, g = rails, b = preview pieces that
// fit, a = preview pieces that are blocked.
@group(0) @binding(3) var infra: texture_2d<u32>;

const WATER: u32 = 15u;

// Texture array layers
const L_GRASS: i32 = 0;
const L_DIRT: i32 = 1;
const L_ROCK: i32 = 2;
const L_GRAVEL: i32 = 3;
const L_SAND: i32 = 4;

// Must match the renderer's clear colour, so distant ocean fades into the sky.
const SKY: vec3<f32> = vec3<f32>(0.55, 0.70, 0.85);

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) edge: f32,
    // Quad-local coordinates for the wireframe overlay.
    @location(2) quad_uv: vec2<f32>,
    @location(3) @interpolate(flat) material: u32,
    // Position in the object's own grid (clouds use it for per-block shading).
    @location(4) local: vec3<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32, @location(0) v: vec4<u32>) -> VsOut {
    var out: VsOut;
    let material = v.w & 15u;
    var world = vec3<f32>(f32(v.x), f32(v.y) * g.params.x, f32(v.z));
    if material == WATER {
        world.y -= 0.12 * g.params.x;
    }
    out.world = world;
    out.local = world;
    out.clip = g.view_proj * vec4<f32>(world, 1.0);
    out.edge = f32((v.w >> 4u) & 1u);
    let corner = vi & 3u;
    out.quad_uv = vec2<f32>(f32(corner == 1u || corner == 2u), f32(corner >= 2u));
    out.material = material;
    return out;
}

fn flat_normal(world: vec3<f32>) -> vec3<f32> {
    var n = normalize(cross(dpdx(world), dpdy(world)));
    // Front faces always face the camera; fix up the sign whatever the
    // framebuffer's y convention is.
    let to_eye = g.camera_pos.xyz - world * g.camera_pos.w;
    if dot(n, to_eye) < 0.0 {
        n = -n;
    }
    return n;
}

// 1 on quad edges, ~0.35 on the triangle diagonal, 0 elsewhere.
fn wire(quad_uv: vec2<f32>) -> f32 {
    let w = fwidth(quad_uv);
    let e = min(quad_uv, 1.0 - quad_uv) / max(w, vec2<f32>(1e-5));
    let edge = 1.0 - clamp(min(e.x, e.y) - 0.5, 0.0, 1.0);
    let d = abs(quad_uv.x - quad_uv.y) / max(fwidth(quad_uv.x - quad_uv.y), 1e-5);
    let diag = 1.0 - clamp(d - 0.5, 0.0, 1.0);
    return max(edge, diag * 0.35);
}

fn grid(world: vec3<f32>, n: vec3<f32>) -> f32 {
    let f = fract(world.xz);
    let d = min(f, 1.0 - f) / max(fwidth(world.xz), vec2<f32>(1e-5));
    let line = 1.0 - clamp(min(d.x, d.y) - 0.5, 0.0, 1.0);
    // Fade out when tiles get too small on screen, and on vertical faces.
    let fade = 1.0 - smoothstep(0.05, 0.15, max(fwidth(world.x), fwidth(world.z)));
    return line * fade * step(0.5, n.y);
}

// Size of a screen pixel in world units on the ground.
fn pixel_size(world: vec3<f32>) -> f32 {
    return max(max(fwidth(world.x), fwidth(world.z)), 1e-5);
}

// Tints the highlighted tile rectangle and outlines its border.
fn highlight(color: vec3<f32>, world: vec3<f32>, px: f32) -> vec3<f32> {
    let h = g.highlight;
    let inside = step(h.x, world.x) * step(world.x, h.z) * step(h.y, world.z) * step(world.z, h.w);
    let d = min(min(world.x - h.x, h.z - world.x), min(world.z - h.y, h.w - world.z));
    let border = inside * (1.0 - smoothstep(1.0, 2.5, d / px));
    let tint = select(vec3<f32>(1.0, 1.0, 1.0), vec3<f32>(1.0, 0.2, 0.15), g.options.y > 1.5);
    let enabled = step(0.5, g.options.y);
    let c = mix(color, tint, inside * 0.3 * enabled);
    return mix(c, tint, border * enabled);
}

// --- Roads -------------------------------------------------------------------
//
// Each tile's road bits are pieces between its centre and edge midpoints
// (bits 0-3: centre to N, E, S, W) or between adjacent edge midpoints
// (bits 4-7: N-E, E-S, S-W, W-N). A fragment is on a road if it is within
// half a road width of any piece in its own or a neighbouring tile.

const ROAD_HALF_WIDTH: f32 = 0.34;

// Distance from p to segment ab, and how far along the segment the nearest point is.
fn segment(p: vec2<f32>, a: vec2<f32>, b: vec2<f32>) -> vec2<f32> {
    let ab = b - a;
    let t = clamp(dot(p - a, ab) / dot(ab, ab), 0.0, 1.0);
    return vec2<f32>(length(p - (a + ab * t)), t * length(ab));
}

fn nearest(best: vec2<f32>, candidate: vec2<f32>) -> vec2<f32> {
    return select(best, candidate, candidate.x < best.x);
}

// Nearest piece among a tile's road bits: (distance, distance along the piece).
fn pieces(p: vec2<f32>, o: vec2<f32>, bits: u32) -> vec2<f32> {
    var best = vec2<f32>(1e6, 0.0);
    if bits == 0u {
        return best;
    }
    let c = o + vec2<f32>(0.5, 0.5);
    let n = o + vec2<f32>(0.5, 0.0);
    let e = o + vec2<f32>(1.0, 0.5);
    let s = o + vec2<f32>(0.5, 1.0);
    let w = o + vec2<f32>(0.0, 0.5);
    if (bits & 1u) != 0u { best = nearest(best, segment(p, c, n)); }
    if (bits & 2u) != 0u { best = nearest(best, segment(p, c, e)); }
    if (bits & 4u) != 0u { best = nearest(best, segment(p, c, s)); }
    if (bits & 8u) != 0u { best = nearest(best, segment(p, c, w)); }
    if (bits & 16u) != 0u { best = nearest(best, segment(p, n, e)); }
    if (bits & 32u) != 0u { best = nearest(best, segment(p, e, s)); }
    if (bits & 64u) != 0u { best = nearest(best, segment(p, s, w)); }
    if (bits & 128u) != 0u { best = nearest(best, segment(p, w, n)); }
    return best;
}

// Track pieces join edge midpoints: bit 0 west-east, bit 1 north-south,
// bits 2-5 the corner diagonals N-E, E-S, S-W, W-N.
const RAIL_HALF_WIDTH: f32 = 0.3;

// Like `segment`, plus the distance with flat ends (z): rails and sleepers
// stop square at the end of the track, while the ballast keeps a round end.
fn rail_segment(p: vec2<f32>, a: vec2<f32>, b: vec2<f32>) -> vec3<f32> {
    let ab = b - a;
    let t_raw = dot(p - a, ab) / dot(ab, ab);
    let t = clamp(t_raw, 0.0, 1.0);
    let d = length(p - (a + ab * t));
    let flat_d = select(1e6, d, t_raw == t);
    return vec3<f32>(d, t * length(ab), flat_d);
}

fn nearest_rail(best: vec3<f32>, candidate: vec3<f32>) -> vec3<f32> {
    let pick = select(best, candidate, candidate.x < best.x);
    return vec3<f32>(pick.xy, min(best.z, candidate.z));
}

// Nearest track piece: (distance, distance along, flat-ended distance).
fn rail_pieces(p: vec2<f32>, o: vec2<f32>, bits: u32) -> vec3<f32> {
    var best = vec3<f32>(1e6, 0.0, 1e6);
    if bits == 0u {
        return best;
    }
    let n = o + vec2<f32>(0.5, 0.0);
    let e = o + vec2<f32>(1.0, 0.5);
    let s = o + vec2<f32>(0.5, 1.0);
    let w = o + vec2<f32>(0.0, 0.5);
    if (bits & 1u) != 0u { best = nearest_rail(best, rail_segment(p, w, e)); }
    if (bits & 2u) != 0u { best = nearest_rail(best, rail_segment(p, n, s)); }
    if (bits & 4u) != 0u { best = nearest_rail(best, rail_segment(p, n, e)); }
    if (bits & 8u) != 0u { best = nearest_rail(best, rail_segment(p, e, s)); }
    if (bits & 16u) != 0u { best = nearest_rail(best, rail_segment(p, s, w)); }
    if (bits & 32u) != 0u { best = nearest_rail(best, rail_segment(p, w, n)); }
    return best;
}

// Preview pieces use the road or track layout depending on the tool.
fn preview_pieces(p: vec2<f32>, o: vec2<f32>, bits: u32) -> vec2<f32> {
    if g.options.z > 0.5 {
        return rail_pieces(p, o, bits).xy;
    }
    return pieces(p, o, bits);
}

struct Infra {
    road: vec2<f32>,
    rail: vec3<f32>,
    preview: f32,
    blocked: f32,
};

// Note: branches on texture data, so call it after anything that needs
// derivatives or implicit-LOD sampling.
fn infra_at(p: vec2<f32>) -> Infra {
    var out = Infra(vec2<f32>(1e6, 0.0), vec3<f32>(1e6, 0.0, 1e6), 1e6, 1e6);
    let tile = vec2<i32>(floor(p));
    let dims = vec2<i32>(textureDimensions(infra));
    for (var dz = -1; dz <= 1; dz++) {
        for (var dx = -1; dx <= 1; dx++) {
            let t = tile + vec2<i32>(dx, dz);
            if t.x < 0 || t.y < 0 || t.x >= dims.x || t.y >= dims.y {
                continue;
            }
            let bits = textureLoad(infra, t, 0);
            if (bits.r | bits.g | bits.b | bits.a) == 0u {
                continue;
            }
            let o = vec2<f32>(t);
            out.road = nearest(out.road, pieces(p, o, bits.r));
            out.rail = nearest_rail(out.rail, rail_pieces(p, o, bits.g));
            out.preview = min(out.preview, preview_pieces(p, o, bits.b).x);
            out.blocked = min(out.blocked, preview_pieces(p, o, bits.a).x);
        }
    }
    return out;
}

fn lighting(n: vec3<f32>) -> f32 {
    let sun = max(dot(n, g.sun_dir.xyz), 0.0);
    let sky = mix(0.40, 0.65, n.y * 0.5 + 0.5);
    return sky + 0.8 * sun;
}

@fragment
fn fs_terrain(in: VsOut) -> @location(0) vec4<f32> {
    let n = flat_normal(in.world);
    let an = abs(n);

    // World-space planar mapping: tops use xz, sides use the dominant axis.
    let side_uv = select(vec2<f32>(in.world.x, -in.world.y), vec2<f32>(in.world.z, -in.world.y), an.x > an.z);
    let uv = select(side_uv, in.world.xz, an.y > 0.5) * g.params.z;

    let is_top = an.y > 0.5;
    var layer = L_ROCK;
    switch in.material {
        case 1u: { layer = select(L_DIRT, L_GRASS, is_top); }
        case 2u: { layer = L_DIRT; }
        case 4u: { layer = L_GRAVEL; }
        case 9u: { layer = L_SAND; }
        default: {}
    }
    let base = textureSample(textures, tex_sampler, uv, layer).rgb;
    // Grass fringe on the top edge of grass block sides.
    let grass = textureSample(textures, tex_sampler, uv, L_GRASS).rgb;
    let rock = textureSample(textures, tex_sampler, in.world.xz * 0.6, L_ROCK).rgb;
    let fringe = select(0.0, smoothstep(0.7, 0.85, in.edge), in.material == 1u && !is_top);
    let light = lighting(n);
    var color = mix(base, grass, fringe) * light;
    color = mix(color, color * 0.6, grid(in.world, n) * g.params.y * on_map(in.world));

    // Everything needing derivatives is done above this line.
    let px = pixel_size(in.world);
    let wf = wire(in.quad_uv) * g.params.w;

    // Roads, only on upward-facing surfaces.
    let inf = infra_at(in.world.xz);
    let top = step(0.5, n.y);
    let hw = ROAD_HALF_WIDTH;
    let road = (1.0 - smoothstep(hw - px, hw + px, inf.road.x)) * top;
    // Fine details fade out once they'd be smaller than a pixel or two.
    let detail = 1.0 - smoothstep(0.015, 0.05, px);
    var asphalt = vec3<f32>(0.13, 0.13, 0.14) * (0.75 + dot(rock, vec3<f32>(0.3, 0.5, 0.2)) * 0.8);
    let kerb = smoothstep(hw - 0.07, hw - 0.05, inf.road.x) * detail;
    asphalt = mix(asphalt, vec3<f32>(0.5, 0.5, 0.48), kerb);
    let centre = 1.0 - smoothstep(0.018, 0.018 + px, inf.road.x);
    let dash = centre * step(0.5, fract(inf.road.y * 2.5)) * detail;
    asphalt = mix(asphalt, vec3<f32>(0.9, 0.85, 0.6), dash);
    color = mix(color, asphalt * light, road);

    // Track: a ballast bed with darker borders, sleepers and two steel rails.
    let rhw = RAIL_HALF_WIDTH;
    let rd = inf.rail.x;
    let bed = (1.0 - smoothstep(rhw - px, rhw + px, rd)) * top;
    var ballast = vec3<f32>(0.45, 0.41, 0.36) * (0.7 + dot(rock, vec3<f32>(0.3, 0.5, 0.2)) * 0.7);
    ballast = mix(ballast, ballast * 0.6, smoothstep(rhw - 0.06, rhw - 0.03, rd));
    let fd = inf.rail.z;
    let sleeper = (1.0 - smoothstep(0.2, 0.2 + px, fd)) * step(fract(inf.rail.y * 5.0), 0.4) * detail;
    ballast = mix(ballast, vec3<f32>(0.3, 0.2, 0.13), sleeper);
    let rail_line = (1.0 - smoothstep(0.014, 0.014 + px, abs(fd - 0.1))) * max(detail, 0.5);
    ballast = mix(ballast, vec3<f32>(0.58, 0.59, 0.62), rail_line);
    color = mix(color, ballast * light, bed);

    // Drag preview: a pale ghost where it fits, red where it doesn't.
    let phw = select(hw, rhw, g.options.z > 0.5);
    let preview = (1.0 - smoothstep(phw - px, phw + px, inf.preview)) * top;
    color = mix(color, vec3<f32>(0.85, 0.88, 0.9) * light, preview * 0.65);
    let blocked = (1.0 - smoothstep(phw - px, phw + px, inf.blocked)) * top;
    color = mix(color, vec3<f32>(0.9, 0.15, 0.1), blocked * 0.7);

    color = highlight(color, in.world, px);

    let wire_bg = select(color, vec3<f32>(0.08, 0.09, 0.12), g.options.x > 0.5);
    color = mix(wire_bg, vec3<f32>(1.0, 0.85, 0.2), wf);
    return vec4<f32>(mix(color, SKY, fog(in.world)), 1.0);
}

// Fog for the endless ocean: nothing on the map is fogged, but the sea fades
// into the sky with distance past the map's edge, so it has no visible end.
fn fog(world: vec3<f32>) -> f32 {
    // Measured from a circle around the island so the horizon is a smooth arc.
    let size = g.ocean.x;
    let from_centre = length(world.xz - vec2<f32>(size * 0.5));
    return smoothstep(size * 0.85, size * 2.2, from_centre);
}

// 1 on the map, 0 outside it (e.g. on the endless ocean floor).
fn on_map(world: vec3<f32>) -> f32 {
    let size = g.ocean.x;
    let inside = world.x >= 0.0 && world.z >= 0.0 && world.x <= size && world.z <= size;
    return select(0.0, 1.0, inside || size == 0.0);
}

@fragment
fn fs_water(in: VsOut) -> @location(0) vec4<f32> {
    let n = flat_normal(in.world);
    var color = highlight(vec3<f32>(0.12, 0.32, 0.45) * lighting(n), in.world, pixel_size(in.world));
    var alpha = 0.78;
    let wf = wire(in.quad_uv) * g.params.w;
    color = mix(color, vec3<f32>(0.3, 0.8, 1.0), wf);
    alpha = mix(alpha, 1.0, wf);
    let f = fog(in.world);
    return vec4<f32>(mix(color, SKY, f), mix(alpha, 1.0, f));
}

// --- Endless ocean -------------------------------------------------------------
//
// Four big rectangles framing the map, generated from the vertex and instance
// index (no vertex buffers): a seabed under the sea surface. Instance i is the
// north, south, west or east band.

fn ocean_vertex(vi: u32, band: u32, y: f32, material: u32) -> VsOut {
    let size = g.ocean.x;
    let r = g.ocean.w;
    var lo = vec2<f32>(-r, -r);
    var hi = vec2<f32>(size + r, 0.0);
    if band == 1u {
        lo = vec2<f32>(-r, size);
        hi = vec2<f32>(size + r, size + r);
    } else if band == 2u {
        lo = vec2<f32>(-r, 0.0);
        hi = vec2<f32>(0.0, size);
    } else if band == 3u {
        lo = vec2<f32>(size, 0.0);
        hi = vec2<f32>(size + r, size);
    }
    // Two triangles, anticlockwise from above: (lo), (lo.x, hi.z), (hi), (lo), (hi), (hi.x, lo.z).
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0), vec2<f32>(0.0, 1.0), vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 1.0), vec2<f32>(1.0, 0.0),
    );
    let c = corners[vi];
    let xz = mix(lo, hi, c);
    var out: VsOut;
    out.world = vec3<f32>(xz.x, y, xz.y);
    out.local = out.world;
    out.clip = g.view_proj * vec4<f32>(out.world, 1.0);
    out.edge = 0.0;
    out.quad_uv = c;
    out.material = material;
    return out;
}

@vertex
fn vs_ocean_floor(@builtin(vertex_index) vi: u32, @builtin(instance_index) band: u32) -> VsOut {
    return ocean_vertex(vi, band, g.ocean.z, 9u);
}

@vertex
fn vs_ocean_water(@builtin(vertex_index) vi: u32, @builtin(instance_index) band: u32) -> VsOut {
    // The water vertex shader lowers water slightly below sea level; match it.
    return ocean_vertex(vi, band, g.ocean.y - 0.12 * g.params.x, WATER);
}

// --- Clouds ------------------------------------------------------------------
//
// Cloud chunks are drawn with a per-draw xz offset (wind drift and wrapping).
// They're opaque, and fade with an ordered dither (screen-door transparency):
// near the cursor, beyond the map edge, and when zoomed in close.

@vertex
fn vs_cloud(@builtin(vertex_index) vi: u32, @location(0) v: vec4<u32>, @location(1) offset: vec2<f32>) -> VsOut {
    var out: VsOut;
    // Cloud blocks are cubes: one world unit tall.
    let world = vec3<f32>(f32(v.x) + offset.x, f32(v.y) + g.clouds.x, f32(v.z) + offset.y);
    out.world = world;
    out.local = vec3<f32>(f32(v.x), f32(v.y), f32(v.z));
    out.clip = g.view_proj * vec4<f32>(world, 1.0);
    out.edge = 0.0;
    let corner = vi & 3u;
    out.quad_uv = vec2<f32>(f32(corner == 1u || corner == 2u), f32(corner >= 2u));
    out.material = v.w & 15u;
    return out;
}

// 4x4 Bayer matrix threshold in (0, 1).
fn bayer4(p: vec2<u32>) -> f32 {
    var m = array<f32, 16>(0.0, 8.0, 2.0, 10.0, 12.0, 4.0, 14.0, 6.0, 3.0, 11.0, 1.0, 9.0, 15.0, 7.0, 13.0, 5.0);
    return (m[(p.y & 3u) * 4u + (p.x & 3u)] + 0.5) / 16.0;
}

@fragment
fn fs_cloud(in: VsOut) -> @location(0) vec4<f32> {
    let n = flat_normal(in.world);
    let wf = wire(in.quad_uv) * g.params.w;

    let to_cursor = distance(in.clip.xy, g.cursor.xy);
    let near_cursor = select(1.0, smoothstep(g.cursor.z * 0.4, g.cursor.z, to_cursor), g.cursor.w > 0.5);
    let size = g.clouds.y;
    let outside = max(max(-in.world.x, in.world.x - size), max(-in.world.z, in.world.z - size));
    let past_edge = 1.0 - smoothstep(0.0, 20.0, outside);
    let alpha = g.clouds.z * near_cursor * past_edge;
    if alpha < bayer4(vec2<u32>(in.clip.xy)) {
        discard;
    }

    // Mostly white. Each cloud has a tone (materials 5-8) from white to a light
    // grey; cumulus are brightest on top and greyer towards the base, and each
    // block varies a little so the surface looks fluffy rather than flat.
    let tone = f32(clamp(in.material, 5u, 8u) - 5u);
    let albedo = vec3<f32>(1.0, 1.0, 1.0) - vec3<f32>(0.045, 0.045, 0.035) * tone;
    let height = smoothstep(3.0, 11.0, in.local.y);
    let block = floor(in.local - n * 0.01);
    let jitter = fract(sin(dot(block, vec3<f32>(12.9898, 78.233, 37.719))) * 43758.5453) - 0.5;
    let sun = max(dot(n, g.sun_dir.xyz), 0.0);
    let shade = mix(0.68, 0.9, n.y * 0.5 + 0.5) + 0.18 * sun;
    let under = mix(vec3<f32>(0.84, 0.87, 0.94), vec3<f32>(1.0, 1.0, 1.0), step(-0.5, n.y));
    var color = albedo * under * shade * mix(0.86, 1.04, height) * (1.0 + jitter * 0.08);
    color = mix(color, vec3<f32>(1.0, 0.85, 0.2), wf);
    return vec4<f32>(color, 1.0);
}
