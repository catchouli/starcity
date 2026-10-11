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
    // x = wireframe only (hide shading), y = highlight kind (0 none, 1 cursor, 2 bulldoze)
    options: vec4<f32>,
    // Highlighted tile rectangle: min x, min z, max x, max z (world units).
    highlight: vec4<f32>,
};

@group(0) @binding(0) var<uniform> g: Globals;
@group(0) @binding(1) var textures: texture_2d_array<f32>;
@group(0) @binding(2) var tex_sampler: sampler;

const WATER: u32 = 15u;

// Texture array layers
const L_GRASS: i32 = 0;
const L_DIRT: i32 = 1;
const L_ROCK: i32 = 2;
const L_GRAVEL: i32 = 3;

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) edge: f32,
    // Quad-local coordinates for the wireframe overlay.
    @location(2) quad_uv: vec2<f32>,
    @location(3) @interpolate(flat) material: u32,
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

// Tints the highlighted tile rectangle and outlines its border.
fn highlight(color: vec3<f32>, world: vec3<f32>) -> vec3<f32> {
    let h = g.highlight;
    let inside = step(h.x, world.x) * step(world.x, h.z) * step(h.y, world.z) * step(world.z, h.w);
    let d = min(min(world.x - h.x, h.z - world.x), min(world.z - h.y, h.w - world.z));
    let px = max(max(fwidth(world.x), fwidth(world.z)), 1e-5);
    let border = inside * (1.0 - smoothstep(1.0, 2.5, d / px));
    let tint = select(vec3<f32>(1.0, 1.0, 1.0), vec3<f32>(1.0, 0.2, 0.15), g.options.y > 1.5);
    let enabled = step(0.5, g.options.y);
    let c = mix(color, tint, inside * 0.3 * enabled);
    return mix(c, tint, border * enabled);
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
        default: {}
    }
    let base = textureSample(textures, tex_sampler, uv, layer).rgb;
    // Grass fringe on the top edge of grass block sides.
    let grass = textureSample(textures, tex_sampler, uv, L_GRASS).rgb;
    let fringe = select(0.0, smoothstep(0.7, 0.85, in.edge), in.material == 1u && !is_top);
    var color = mix(base, grass, fringe) * lighting(n);

    color = mix(color, color * 0.6, grid(in.world, n) * g.params.y);
    color = highlight(color, in.world);

    let wf = wire(in.quad_uv) * g.params.w;
    let wire_bg = select(color, vec3<f32>(0.08, 0.09, 0.12), g.options.x > 0.5);
    color = mix(wire_bg, vec3<f32>(1.0, 0.85, 0.2), wf);
    return vec4<f32>(color, 1.0);
}

@fragment
fn fs_water(in: VsOut) -> @location(0) vec4<f32> {
    let n = flat_normal(in.world);
    var color = highlight(vec3<f32>(0.12, 0.32, 0.45) * lighting(n), in.world);
    var alpha = 0.78;
    let wf = wire(in.quad_uv) * g.params.w;
    color = mix(color, vec3<f32>(0.3, 0.8, 1.0), wf);
    alpha = mix(alpha, 1.0, wf);
    return vec4<f32>(color, alpha);
}
