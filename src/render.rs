//! 3D scene rendering with plain wgpu into an offscreen texture that egui displays.

use crate::daylight::Light;
use crate::models::{ModelId, ModelVertex, Models};
use crate::terrain::BLOCK_HEIGHT;
use crate::terrain::mesh::{ChunkMesh, Vertex};
use eframe::egui;
use eframe::egui_wgpu::RenderState;
use eframe::wgpu;
use glam::{Mat4, Vec2, Vec3, Vec4};
use wgpu::util::DeviceExt;

const COLOR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// Texture array layers, matching the `L_*` constants in the shader.
const TEXTURES: [&[u8]; 5] = [
    include_bytes!("../assets/textures/grass.png"),
    include_bytes!("../assets/textures/dirt.png"),
    include_bytes!("../assets/textures/rock.png"),
    include_bytes!("../assets/textures/gravel.png"),
    include_bytes!("../assets/textures/sand.png"),
];

/// How far (tiles) the see-through bubble reaches round what it reveals, and how far in front of
/// the centre things have to be to fade.
const REVEAL_RADIUS: f32 = 5.0;
const REVEAL_MARGIN: f32 = 0.8;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Globals {
    pub view_proj: [[f32; 4]; 4],
    /// Eye position with w = 1 (perspective), or direction towards the eye with w = 0 (ortho).
    pub camera_pos: [f32; 4],
    pub sun_dir: [f32; 4],
    /// Block height, grid strength, texture repeats per tile, wireframe.
    pub params: [f32; 4],
    /// Wireframe only, highlight kind (0 none, 1 cursor, 2 bulldoze).
    pub options: [f32; 4],
    /// Highlighted tile rectangle in world x/z: min x, min z, max x, max z.
    pub highlight: [f32; 4],
    /// Cloud altitude (world units), cloud field size, opacity.
    pub clouds: [f32; 4],
    /// Cursor position in render-target pixels, fade radius in pixels, and whether it's over the view.
    pub cursor: [f32; 4],
    /// Endless ocean: map size, sea surface y, seabed y, extent past the map.
    pub ocean: [f32; 4],
    /// Sky colour (rgb) and daylight (w).
    pub sky: [f32; 4],
    /// Direct light colour, strength included.
    pub sun_color: [f32; 4],
    /// Ambient light tint.
    pub ambient: [f32; 4],
    /// Era: how chrome roads and track look (x).
    pub era: [f32; 4],
    /// See-through bubble: centre pixels, radius pixels, margin; and its world centre with an on flag.
    pub reveal: [f32; 4],
    pub reveal_world: [f32; 4],
}

/// One placed model: where it stands, which way it faces, how big it is, and
/// its tint. Drawn with the mesh of the model it's paired with in `set_models`.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ModelInstance {
    /// Origin of the model in the world (x, height, z).
    pub pos: [f32; 3],
    /// Rotation about the vertical axis, in radians: `atan2(-dz, dx)` for a
    /// model whose front is +X to face along (dx, dz).
    pub yaw: f32,
    pub scale: f32,
    /// sRGB tint, applied to the model's "Paint" materials. Alpha 255 darkens
    /// the whole model (a wreck).
    pub tint: [u8; 4],
}

impl ModelInstance {
    pub fn at(pos: [f32; 3], yaw: f32) -> ModelInstance {
        ModelInstance {
            pos,
            yaw,
            scale: 1.0,
            tint: [255, 255, 255, 0],
        }
    }

    pub fn tinted(mut self, rgb: [u8; 3]) -> ModelInstance {
        self.tint = [rgb[0], rgb[1], rgb[2], self.tint[3]];
        self
    }

    /// Marks the model as the thing the see-through bubble reveals: it is never faded itself.
    pub fn revealed(mut self) -> ModelInstance {
        self.tint[3] = 253;
        self
    }

    pub fn wrecked(mut self) -> ModelInstance {
        self.tint[3] = 255;
        self
    }
}

/// Per-frame cloud state.
#[derive(Clone, Copy, Debug)]
pub struct CloudState {
    /// How far the wind has carried the clouds, in tiles.
    pub wind: Vec2,
    pub altitude: f32,
    /// 0 hides the clouds entirely.
    pub opacity: f32,
    /// Cursor position in render-target pixels, if it's over the 3D view.
    pub cursor: Option<Vec2>,
    pub fade_radius: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HighlightKind {
    Cursor = 1,
    Bulldoze = 2,
}

/// A highlighted, inclusive rectangle of tiles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Highlight {
    pub a: (i32, i32),
    pub b: (i32, i32),
    pub kind: HighlightKind,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RenderSettings {
    pub msaa: u32,
    /// Fraction of native resolution to render the 3D view at.
    pub render_scale: f32,
    pub grid: bool,
    pub wireframe: bool,
    pub wireframe_only: bool,
    pub texture_repeat: f32,
}

impl Default for RenderSettings {
    fn default() -> Self {
        RenderSettings {
            // MSAA is cheap on desktop GPUs but not on phones; keep the web lean by default.
            msaa: if cfg!(target_arch = "wasm32") { 1 } else { 4 },
            render_scale: 1.0,
            grid: true,
            wireframe: false,
            wireframe_only: false,
            texture_repeat: 0.25,
        }
    }
}

#[derive(Default, Clone, Copy)]
pub struct RenderStats {
    pub chunks_drawn: u32,
    pub chunks_total: u32,
    pub quads_drawn: u32,
    pub quads_total: u32,
}

struct GpuChunk {
    terrain: Option<(wgpu::Buffer, u32)>,
    water: Option<(wgpu::Buffer, u32)>,
    min: Vec3,
    max: Vec3,
}

struct Target {
    size: [u32; 2],
    samples: u32,
    color: wgpu::TextureView,
    msaa: Option<wgpu::TextureView>,
    depth: wgpu::TextureView,
    egui_id: egui::TextureId,
}

pub struct SceneRenderer {
    shader: wgpu::ShaderModule,
    layout: wgpu::PipelineLayout,
    pipelines: Pipelines,
    pipeline_samples: u32,
    globals: wgpu::Buffer,
    bind_group_layout: wgpu::BindGroupLayout,
    terrain_textures: wgpu::TextureView,
    sampler: wgpu::Sampler,
    /// Per-tile infrastructure bits (Rgba8Uint): r = roads, g = rails (later),
    /// b = preview pieces that fit, a = preview pieces that are blocked.
    infra: wgpu::Texture,
    bind_group: wgpu::BindGroup,
    quad_indices: wgpu::Buffer,
    quad_capacity: u32,
    chunks: Vec<GpuChunk>,
    /// Cloud chunks: vertex buffer, quad count and origin in tiles.
    clouds: Vec<(wgpu::Buffer, u32, [f32; 2])>,
    cloud_offsets: wgpu::Buffer,
    /// Size of the (square, wrapping) cloud field in tiles.
    cloud_field: f32,
    target: Option<Target>,
    pub stats: RenderStats,
    /// Whether the drag preview shows track (true) or road pieces.
    pub preview_is_rail: bool,
    /// Endless ocean parameters (see `Globals::ocean`); size 0 hides it.
    pub ocean: [f32; 4],
    /// How chrome roads and track look, 0 to 1 (see `era.rs`).
    pub era_chrome: f32,
    /// A spot to keep visible through whatever stands in front of it (world position), such as the
    /// headquarters: buildings between it and the camera fade to a dither inside a bubble.
    pub reveal: Option<Vec3>,
    /// Every model's mesh, in one vertex and one index buffer.
    model_vertices: Option<wgpu::Buffer>,
    model_indices: Option<wgpu::Buffer>,
    model_ranges: Vec<ModelRange>,
    /// Placed models, one instance buffer and draw list per `ModelLayer`.
    placed: [Option<wgpu::Buffer>; 2],
    draws: [Vec<(ModelId, u32, u32)>; 2],
}

/// Where a model's triangles are in the shared buffers.
#[derive(Clone, Copy, Debug)]
struct ModelRange {
    first_index: u32,
    index_count: u32,
    base_vertex: i32,
}

/// Models that rarely change (buildings, stations) are kept apart from ones
/// that move every frame (trains, vehicles, signals), so moving things don't
/// re-upload the whole town.
#[derive(Clone, Copy, Debug)]
pub enum ModelLayer {
    Static = 0,
    Dynamic = 1,
}

impl SceneRenderer {
    pub fn new(rs: &RenderState) -> SceneRenderer {
        let device = &rs.device;
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("terrain"),
            source: wgpu::ShaderSource::Wgsl(include_str!("terrain.wgsl").into()),
        });

        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("globals"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Uint,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("terrain"),
            bind_group_layouts: &[Some(&bgl)],
            immediate_size: 0,
        });

        let globals = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("globals"),
            size: std::mem::size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let textures = load_texture_array(device, &rs.queue);
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("terrain"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        let infra = create_infra_texture(device, 1, 1);
        let bind_group = create_bind_group(device, &bgl, &globals, &textures, &sampler, &infra);

        let pipelines = create_pipelines(device, &layout, &shader, 1);
        let (quad_indices, quad_capacity) = create_quad_indices(device, 1024);

        SceneRenderer {
            shader,
            layout,
            pipelines,
            clouds: Vec::new(),
            cloud_offsets: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("cloud offsets"),
                size: 8 * 256,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            cloud_field: 1.0,
            pipeline_samples: 1,
            globals,
            bind_group_layout: bgl,
            terrain_textures: textures,
            sampler,
            infra,
            bind_group,
            quad_indices,
            quad_capacity,
            chunks: Vec::new(),
            target: None,
            stats: RenderStats::default(),
            preview_is_rail: false,
            ocean: [0.0; 4],
            era_chrome: 0.0,
            reveal: None,
            model_vertices: None,
            model_indices: None,
            model_ranges: Vec::new(),
            placed: [None, None],
            draws: [Vec::new(), Vec::new()],
        }
    }

    /// Recreates the infrastructure texture for a new map size (all zero).
    pub fn resize_infra(&mut self, device: &wgpu::Device, size_x: u32, size_z: u32) {
        self.infra = create_infra_texture(device, size_x, size_z);
        self.bind_group = create_bind_group(
            device,
            &self.bind_group_layout,
            &self.globals,
            &self.terrain_textures,
            &self.sampler,
            &self.infra,
        );
    }

    /// Writes RGBA infrastructure bits for a rectangle of tiles, row-major.
    pub fn write_infra(&self, queue: &wgpu::Queue, x: u32, z: u32, w: u32, h: u32, rgba: &[u8]) {
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.infra,
                mip_level: 0,
                origin: wgpu::Origin3d { x, y: z, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(w * 4),
                rows_per_image: Some(h),
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
    }

    /// Uploads every model's mesh. Done once at startup.
    pub fn upload_models(&mut self, device: &wgpu::Device, models: &Models) {
        let mut vertices: Vec<ModelVertex> = Vec::new();
        let mut indices: Vec<u32> = Vec::new();
        self.model_ranges.clear();
        for mesh in &models.meshes {
            self.model_ranges.push(ModelRange {
                first_index: indices.len() as u32,
                index_count: mesh.indices.len() as u32,
                base_vertex: vertices.len() as i32,
            });
            vertices.extend_from_slice(&mesh.vertices);
            indices.extend_from_slice(&mesh.indices);
        }
        let make = |label, data: &[u8], usage| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(label),
                contents: data,
                usage,
            })
        };
        self.model_vertices = Some(make(
            "model vertices",
            bytemuck::cast_slice(&vertices),
            wgpu::BufferUsages::VERTEX,
        ));
        self.model_indices = Some(make(
            "model indices",
            bytemuck::cast_slice(&indices),
            wgpu::BufferUsages::INDEX,
        ));
    }

    /// Replaces the set of placed models in a layer. Reuses the buffer when
    /// it's big enough, since trains update this every frame.
    pub fn set_models(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        layer: ModelLayer,
        items: &mut [(ModelId, ModelInstance)],
    ) {
        // One draw per model: group equal models together.
        items.sort_by_key(|(id, _)| id.0);
        let instances: Vec<ModelInstance> = items.iter().map(|(_, i)| *i).collect();
        let draws = &mut self.draws[layer as usize];
        draws.clear();
        let mut start = 0;
        while start < items.len() {
            let id = items[start].0;
            let count = items[start..].iter().take_while(|(m, _)| *m == id).count();
            draws.push((id, start as u32, count as u32));
            start += count;
        }
        let slot = &mut self.placed[layer as usize];
        let bytes: &[u8] = bytemuck::cast_slice(&instances);
        let fits = slot.as_ref().is_some_and(|buf| buf.size() >= bytes.len() as u64);
        if !fits && !instances.is_empty() {
            *slot = Some(device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("placed models"),
                size: (bytes.len() as u64).next_power_of_two().max(256),
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
        }
        if let (Some(buf), false) = (slot.as_ref(), bytes.is_empty()) {
            queue.write_buffer(buf, 0, bytes);
        }
    }

    /// Replaces the cloud field's chunk meshes.
    pub fn upload_clouds(&mut self, device: &wgpu::Device, field_size: usize, meshes: &[(ChunkMesh, (u32, u32))]) {
        self.cloud_field = field_size as f32;
        self.clouds = meshes
            .iter()
            .map(|(m, (x, z))| {
                let buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("cloud chunk"),
                    contents: bytemuck::cast_slice(&m.terrain),
                    usage: wgpu::BufferUsages::VERTEX,
                });
                let quads = (m.terrain.len() / 4) as u32;
                if quads > self.quad_capacity {
                    (self.quad_indices, self.quad_capacity) = create_quad_indices(device, quads.next_power_of_two());
                }
                (buf, quads, [*x as f32, *z as f32])
            })
            .collect();
    }

    /// Which cloud chunks to draw and at what xz offset. The field wraps, so a
    /// chunk is drawn wherever a copy of it overlaps the map (plus a margin the
    /// clouds fade out over), possibly more than once near the seams.
    fn cloud_draws(&self, wind: Vec2) -> Vec<(usize, [f32; 2])> {
        let size = self.cloud_field;
        let chunk = crate::terrain::CHUNK_SIZE as f32;
        let margin = 24.0;
        let shift = Vec2::new(wind.x.rem_euclid(size), wind.y.rem_euclid(size));
        let mut draws = Vec::new();
        for (i, (_, _, origin)) in self.clouds.iter().enumerate() {
            for kz in -1..=1 {
                for kx in -1..=1 {
                    let offset = shift + Vec2::new(kx as f32, kz as f32) * size;
                    let min = Vec2::from(*origin) + offset;
                    let visible = |lo: f32| lo + chunk > -margin && lo < size + margin;
                    if visible(min.x) && visible(min.y) {
                        draws.push((i, offset.into()));
                    }
                }
            }
        }
        draws
    }

    /// Replaces all chunk meshes.
    pub fn upload_chunks(&mut self, device: &wgpu::Device, meshes: &[ChunkMesh]) {
        self.chunks = meshes.iter().map(|m| self.gpu_chunk(device, m)).collect();
        self.update_stats();
    }

    /// Replaces one chunk's mesh (index = cz * chunks_x + cx).
    pub fn update_chunk(&mut self, device: &wgpu::Device, index: usize, mesh: &ChunkMesh) {
        self.chunks[index] = self.gpu_chunk(device, mesh);
        self.update_stats();
    }

    fn gpu_chunk(&mut self, device: &wgpu::Device, m: &ChunkMesh) -> GpuChunk {
        let quads = (m.terrain.len().max(m.water.len()) / 4) as u32;
        if quads > self.quad_capacity {
            (self.quad_indices, self.quad_capacity) = create_quad_indices(device, quads.next_power_of_two());
        }
        let upload = |verts: &[Vertex]| {
            (!verts.is_empty()).then(|| {
                let buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("chunk"),
                    contents: bytemuck::cast_slice(verts),
                    usage: wgpu::BufferUsages::VERTEX,
                });
                (buf, (verts.len() / 4) as u32)
            })
        };
        GpuChunk {
            terrain: upload(&m.terrain),
            water: upload(&m.water),
            min: Vec3::new(m.min[0] as f32, m.min[1] as f32 * BLOCK_HEIGHT - 0.1, m.min[2] as f32),
            max: Vec3::new(m.max[0] as f32, m.max[1] as f32 * BLOCK_HEIGHT, m.max[2] as f32),
        }
    }

    fn update_stats(&mut self) {
        self.stats.chunks_total = self.chunks.len() as u32;
        self.stats.quads_total = self.chunks.iter().map(|c| quads(&c.terrain) + quads(&c.water)).sum();
    }

    /// Renders the scene at `size` pixels into the offscreen target.
    pub fn render(
        &mut self,
        rs: &RenderState,
        size: [u32; 2],
        settings: &RenderSettings,
        view_proj: Mat4,
        camera_pos: Vec4,
        highlight: Option<Highlight>,
        clouds: CloudState,
        light: &Light,
    ) {
        let device = &rs.device;
        let samples = supported_samples(rs, settings.msaa);
        if self.pipeline_samples != samples {
            self.pipelines = create_pipelines(device, &self.layout, &self.shader, samples);
            self.pipeline_samples = samples;
        }
        self.ensure_target(rs, size, samples);
        let target = self.target.as_ref().unwrap();

        // The bubble's centre and size on screen: project the centre and a point a few tiles to its side.
        let (reveal, reveal_world) = match self.reveal {
            Some(c) => {
                let project = |p: Vec3| {
                    let clip = view_proj * p.extend(1.0);
                    (clip.w > 0.0).then(|| {
                        Vec2::new(
                            (clip.x / clip.w * 0.5 + 0.5) * size[0] as f32,
                            (0.5 - clip.y / clip.w * 0.5) * size[1] as f32,
                        )
                    })
                };
                let right = Vec3::new(view_proj.x_axis.x, view_proj.y_axis.x, view_proj.z_axis.x).normalize_or_zero();
                match (project(c), project(c + right * REVEAL_RADIUS)) {
                    (Some(p0), Some(p1)) if (p1 - p0).length() > 8.0 => {
                        ([p0.x, p0.y, (p1 - p0).length(), REVEAL_MARGIN], [c.x, c.y, c.z, 1.0])
                    }
                    _ => ([0.0; 4], [0.0; 4]),
                }
            }
            None => ([0.0; 4], [0.0; 4]),
        };
        let sun = Vec3::from(light.dir);
        let globals = Globals {
            view_proj: view_proj.to_cols_array_2d(),
            camera_pos: camera_pos.into(),
            sun_dir: [sun.x, sun.y, sun.z, 0.0],
            params: [
                BLOCK_HEIGHT,
                if settings.grid { 0.5 } else { 0.0 },
                settings.texture_repeat,
                if settings.wireframe { 1.0 } else { 0.0 },
            ],
            options: [
                if settings.wireframe_only { 1.0 } else { 0.0 },
                highlight.map_or(0.0, |h| h.kind as u8 as f32),
                if self.preview_is_rail { 1.0 } else { 0.0 },
                0.0,
            ],
            clouds: [clouds.altitude, self.cloud_field, clouds.opacity, 0.0],
            ocean: self.ocean,
            cursor: match clouds.cursor {
                Some(c) => [c.x, c.y, clouds.fade_radius, 1.0],
                None => [0.0; 4],
            },
            highlight: highlight.map_or([0.0; 4], |h| {
                [
                    h.a.0.min(h.b.0) as f32,
                    h.a.1.min(h.b.1) as f32,
                    (h.a.0.max(h.b.0) + 1) as f32,
                    (h.a.1.max(h.b.1) + 1) as f32,
                ]
            }),
            sky: [light.sky[0], light.sky[1], light.sky[2], light.day],
            sun_color: [light.sun[0], light.sun[1], light.sun[2], 0.0],
            ambient: [light.ambient[0], light.ambient[1], light.ambient[2], 0.0],
            era: [self.era_chrome, 0.0, 0.0, 0.0],
            reveal,
            reveal_world,
        };
        rs.queue.write_buffer(&self.globals, 0, bytemuck::bytes_of(&globals));

        let planes = frustum_planes(view_proj);
        let visible: Vec<&GpuChunk> = self
            .chunks
            .iter()
            .filter(|c| aabb_visible(&planes, c.min, c.max))
            .collect();

        let cloud_draws = if clouds.opacity > 0.0 {
            self.cloud_draws(clouds.wind)
        } else {
            Vec::new()
        };
        if !cloud_draws.is_empty() {
            let offsets: Vec<[f32; 2]> = cloud_draws.iter().map(|&(_, o)| o).collect();
            let bytes = (offsets.len() * 8) as u64;
            if bytes > self.cloud_offsets.size() {
                self.cloud_offsets = device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("cloud offsets"),
                    size: bytes.next_power_of_two(),
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
            }
            rs.queue
                .write_buffer(&self.cloud_offsets, 0, bytemuck::cast_slice(&offsets));
        }

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("scene") });
        {
            let (view, resolve) = match &target.msaa {
                Some(msaa) => (msaa, Some(&target.color)),
                None => (&target.color, None),
            };
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("scene"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target: resolve,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: light.sky[0] as f64,
                            g: light.sky[1] as f64,
                            b: light.sky[2] as f64,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &target.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.set_index_buffer(self.quad_indices.slice(..), wgpu::IndexFormat::Uint32);

            let mut drawn = 0;
            pass.set_pipeline(&self.pipelines.terrain);
            for c in &visible {
                if let Some((buf, n)) = &c.terrain {
                    pass.set_vertex_buffer(0, buf.slice(..));
                    pass.draw_indexed(0..n * 6, 0, 0..1);
                    drawn += n;
                }
            }
            if self.ocean[0] > 0.0 {
                pass.set_pipeline(&self.pipelines.ocean_floor);
                pass.draw(0..6, 0..4);
            }
            if let (Some(vertices), Some(indices)) = (&self.model_vertices, &self.model_indices) {
                pass.set_pipeline(&self.pipelines.models);
                pass.set_vertex_buffer(0, vertices.slice(..));
                pass.set_index_buffer(indices.slice(..), wgpu::IndexFormat::Uint32);
                for layer in 0..2 {
                    let Some(placed) = &self.placed[layer] else { continue };
                    pass.set_vertex_buffer(1, placed.slice(..));
                    for &(id, first, count) in &self.draws[layer] {
                        let range = self.model_ranges[id.0 as usize];
                        pass.draw_indexed(
                            range.first_index..range.first_index + range.index_count,
                            range.base_vertex,
                            first..first + count,
                        );
                    }
                }
                // The terrain's shared quad indices again, for the clouds and water.
                pass.set_index_buffer(self.quad_indices.slice(..), wgpu::IndexFormat::Uint32);
            }
            if !cloud_draws.is_empty() {
                pass.set_pipeline(&self.pipelines.clouds);
                for (i, &(chunk, _)) in cloud_draws.iter().enumerate() {
                    let (buf, n, _) = &self.clouds[chunk];
                    pass.set_vertex_buffer(0, buf.slice(..));
                    pass.set_vertex_buffer(1, self.cloud_offsets.slice(i as u64 * 8..(i as u64 + 1) * 8));
                    pass.draw_indexed(0..n * 6, 0, 0..1);
                    drawn += n;
                }
            }
            pass.set_pipeline(&self.pipelines.water);
            for c in &visible {
                if let Some((buf, n)) = &c.water {
                    pass.set_vertex_buffer(0, buf.slice(..));
                    pass.draw_indexed(0..n * 6, 0, 0..1);
                    drawn += n;
                }
            }
            if self.ocean[0] > 0.0 {
                pass.set_pipeline(&self.pipelines.ocean_water);
                pass.draw(0..6, 0..4);
            }
            self.stats.quads_drawn = drawn;
            self.stats.chunks_drawn = visible.len() as u32;
        }
        rs.queue.submit([encoder.finish()]);
    }

    /// The egui texture holding the last rendered frame.
    pub fn texture_id(&self) -> Option<egui::TextureId> {
        self.target.as_ref().map(|t| t.egui_id)
    }

    fn ensure_target(&mut self, rs: &RenderState, size: [u32; 2], samples: u32) {
        if let Some(t) = &self.target {
            if t.size == size && t.samples == samples {
                return;
            }
        }
        let device = &rs.device;
        let make = |label, format, samples, usage| {
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: wgpu::Extent3d {
                        width: size[0],
                        height: size[1],
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: samples,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage,
                    view_formats: &[],
                })
                .create_view(&Default::default())
        };
        let color = make(
            "scene color",
            COLOR_FORMAT,
            1,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        );
        let msaa = (samples > 1).then(|| {
            make(
                "scene msaa",
                COLOR_FORMAT,
                samples,
                wgpu::TextureUsages::RENDER_ATTACHMENT,
            )
        });
        let depth = make(
            "scene depth",
            DEPTH_FORMAT,
            samples,
            wgpu::TextureUsages::RENDER_ATTACHMENT,
        );

        let mut egui_renderer = rs.renderer.write();
        let egui_id = match &self.target {
            Some(old) => {
                egui_renderer.update_egui_texture_from_wgpu_texture(
                    device,
                    &color,
                    wgpu::FilterMode::Linear,
                    old.egui_id,
                );
                old.egui_id
            }
            None => egui_renderer.register_native_texture(device, &color, wgpu::FilterMode::Linear),
        };
        self.target = Some(Target {
            size,
            samples,
            color,
            msaa,
            depth,
            egui_id,
        });
    }
}

fn create_infra_texture(device: &wgpu::Device, w: u32, h: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("infrastructure"),
        size: wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Uint,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    })
}

fn create_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    globals: &wgpu::Buffer,
    textures: &wgpu::TextureView,
    sampler: &wgpu::Sampler,
    infra: &wgpu::Texture,
) -> wgpu::BindGroup {
    let infra_view = infra.create_view(&Default::default());
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("globals"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: globals.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(textures),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::TextureView(&infra_view),
            },
        ],
    })
}

fn quads(b: &Option<(wgpu::Buffer, u32)>) -> u32 {
    b.as_ref().map_or(0, |(_, n)| *n)
}

fn supported_samples(rs: &RenderState, wanted: u32) -> u32 {
    if wanted <= 1 {
        return 1;
    }
    let color = rs.adapter.get_texture_format_features(COLOR_FORMAT).flags;
    let depth = rs.adapter.get_texture_format_features(DEPTH_FORMAT).flags;
    if color.sample_count_supported(wanted) && depth.sample_count_supported(wanted) {
        wanted
    } else {
        1
    }
}

struct Pipelines {
    terrain: wgpu::RenderPipeline,
    clouds: wgpu::RenderPipeline,
    water: wgpu::RenderPipeline,
    ocean_floor: wgpu::RenderPipeline,
    ocean_water: wgpu::RenderPipeline,
    models: wgpu::RenderPipeline,
}

fn create_pipelines(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    samples: u32,
) -> Pipelines {
    let vertex_attrs = [wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Uint16x4,
        offset: 0,
        shader_location: 0,
    }];
    let vertices = wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<Vertex>() as u64,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &vertex_attrs,
    };
    // Clouds take a per-draw xz offset as an instance attribute.
    let offset_attrs = [wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x2,
        offset: 0,
        shader_location: 1,
    }];
    let offsets = wgpu::VertexBufferLayout {
        array_stride: 8,
        step_mode: wgpu::VertexStepMode::Instance,
        attributes: &offset_attrs,
    };
    let make = |label, vs, fs, buffers: &[Option<wgpu::VertexBufferLayout>], blend, depth_write| {
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(label),
            layout: Some(layout),
            vertex: wgpu::VertexState {
                module: shader,
                entry_point: Some(vs),
                compilation_options: Default::default(),
                buffers,
            },
            primitive: wgpu::PrimitiveState {
                cull_mode: Some(wgpu::Face::Back),
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(depth_write),
                depth_compare: Some(wgpu::CompareFunction::LessEqual),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: samples,
                ..Default::default()
            },
            fragment: Some(wgpu::FragmentState {
                module: shader,
                entry_point: Some(fs),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: COLOR_FORMAT,
                    blend,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        })
    };
    use wgpu::VertexFormat as F;
    let attr = |format, offset, shader_location| wgpu::VertexAttribute {
        format,
        offset,
        shader_location,
    };
    let mesh_attrs = [
        attr(F::Float32x3, 0, 0),
        attr(F::Float32x3, 12, 1),
        attr(F::Float32x4, 24, 2),
        attr(F::Unorm8x4, 40, 3),
    ];
    let placed_attrs = [
        attr(F::Float32x3, 0, 4),
        attr(F::Float32, 12, 5),
        attr(F::Float32, 16, 6),
        attr(F::Unorm8x4, 20, 7),
    ];
    let model_buffers = [
        Some(wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<ModelVertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &mesh_attrs,
        }),
        Some(wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<ModelInstance>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &placed_attrs,
        }),
    ];
    let terrain_buffers = [Some(vertices.clone())];
    let cloud_buffers = [Some(vertices), Some(offsets)];
    Pipelines {
        terrain: make("terrain", "vs_main", "fs_terrain", &terrain_buffers, None, true),
        clouds: make("clouds", "vs_cloud", "fs_cloud", &cloud_buffers, None, true),
        water: make(
            "water",
            "vs_main",
            "fs_water",
            &terrain_buffers,
            Some(wgpu::BlendState::ALPHA_BLENDING),
            false,
        ),
        ocean_floor: make("ocean floor", "vs_ocean_floor", "fs_terrain", &[], None, true),
        models: make("models", "vs_model", "fs_model", &model_buffers, None, true),
        ocean_water: make(
            "ocean water",
            "vs_ocean_water",
            "fs_water",
            &[],
            Some(wgpu::BlendState::ALPHA_BLENDING),
            false,
        ),
    }
}

/// One index buffer shared by every chunk: quad q uses vertices 4q..4q+3.
fn create_quad_indices(device: &wgpu::Device, quads: u32) -> (wgpu::Buffer, u32) {
    let indices: Vec<u32> = (0..quads).flat_map(|q| [0, 1, 2, 0, 2, 3].map(|i| q * 4 + i)).collect();
    let buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("quad indices"),
        contents: bytemuck::cast_slice(&indices),
        usage: wgpu::BufferUsages::INDEX,
    });
    (buf, quads)
}

fn load_texture_array(device: &wgpu::Device, queue: &wgpu::Queue) -> wgpu::TextureView {
    let images: Vec<(u32, Vec<u8>)> = TEXTURES.iter().map(|png| decode_png(png)).collect();
    let size = images[0].0;
    let mip_count = size.ilog2() + 1;
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("terrain textures"),
        size: wgpu::Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: images.len() as u32,
        },
        mip_level_count: mip_count,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    for (layer, (s, rgba)) in images.into_iter().enumerate() {
        assert_eq!(s, size, "all terrain textures must be the same size");
        let mut level = rgba;
        let mut dim = size;
        for mip in 0..mip_count {
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: mip,
                    origin: wgpu::Origin3d {
                        x: 0,
                        y: 0,
                        z: layer as u32,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                &level,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(dim * 4),
                    rows_per_image: Some(dim),
                },
                wgpu::Extent3d {
                    width: dim,
                    height: dim,
                    depth_or_array_layers: 1,
                },
            );
            if dim > 1 {
                level = downsample_srgb(&level, dim);
                dim /= 2;
            }
        }
    }
    texture.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::D2Array),
        ..Default::default()
    })
}

/// Decodes a square PNG to RGBA8. Returns (size, pixels).
fn decode_png(bytes: &[u8]) -> (u32, Vec<u8>) {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().expect("valid png");
    let mut buf = vec![0; reader.output_buffer_size().expect("png size")];
    let info = reader.next_frame(&mut buf).expect("png frame");
    let px = (info.width * info.height) as usize;
    let rgba = match info.color_type {
        png::ColorType::Rgba => buf[..px * 4].to_vec(),
        png::ColorType::Rgb => buf[..px * 3]
            .chunks_exact(3)
            .flat_map(|c| [c[0], c[1], c[2], 255])
            .collect(),
        other => panic!("unsupported png colour type {other:?}"),
    };
    assert_eq!(info.width, info.height);
    (info.width, rgba)
}

/// 2x2 box filter in linear space, so mips don't darken.
fn downsample_srgb(src: &[u8], dim: u32) -> Vec<u8> {
    let to_linear: Vec<f32> = (0..256).map(|i| srgb_to_linear(i as f32 / 255.0)).collect();
    let half = dim / 2;
    let mut out = vec![0u8; (half * half * 4) as usize];
    for y in 0..half {
        for x in 0..half {
            for c in 0..4 {
                let at = |sx: u32, sy: u32| src[((sy * dim + sx) * 4 + c) as usize];
                let samples = [
                    at(2 * x, 2 * y),
                    at(2 * x + 1, 2 * y),
                    at(2 * x, 2 * y + 1),
                    at(2 * x + 1, 2 * y + 1),
                ];
                let v = if c == 3 {
                    samples.iter().map(|&s| s as f32).sum::<f32>() / 4.0
                } else {
                    linear_to_srgb(samples.iter().map(|&s| to_linear[s as usize]).sum::<f32>() / 4.0) * 255.0
                };
                out[((y * half + x) * 4 + c) as usize] = v.round() as u8;
            }
        }
    }
    out
}

fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn linear_to_srgb(c: f32) -> f32 {
    if c <= 0.0031308 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

fn frustum_planes(m: Mat4) -> [Vec4; 5] {
    let (r0, r1, r2, r3) = (m.row(0), m.row(1), m.row(2), m.row(3));
    // Left, right, bottom, top, far. (Near is skipped; nothing sits behind the eye.)
    [r3 + r0, r3 - r0, r3 + r1, r3 - r1, r3 - r2]
}

fn aabb_visible(planes: &[Vec4; 5], min: Vec3, max: Vec3) -> bool {
    planes.iter().all(|p| {
        let v = Vec3::new(
            if p.x >= 0.0 { max.x } else { min.x },
            if p.y >= 0.0 { max.y } else { min.y },
            if p.z >= 0.0 { max.z } else { min.z },
        );
        p.truncate().dot(v) + p.w >= 0.0
    })
}
