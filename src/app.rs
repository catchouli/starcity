use crate::camera::Camera;
use crate::clouds;
use crate::picking;
use crate::render::{CloudState, Highlight, HighlightKind, RenderSettings, SceneRenderer};
use crate::roads::{self, Roads};
use crate::sim::{self, Changes, Clear, Sim};
use crate::terrain::generation::{self, GenParams};
use crate::terrain::{CHUNK_SIZE, World, mesh};
use crate::ui::{self, Tool, UiState, win95};
use eframe::egui;
use eframe::egui_wgpu::RenderState;
use glam::{Mat4, Vec2, Vec3};
use std::collections::BTreeSet;
use web_time::Instant;

/// Wind speed and direction, in tiles per second.
const WIND: Vec2 = Vec2::new(0.6, 0.25);
/// Redraw interval while the only thing moving is the clouds.
const CLOUD_FRAME_SECONDS: f64 = 1.0 / 15.0;

pub struct StarcityApp {
    params: GenParams,
    world: World,
    camera: Camera,
    renderer: SceneRenderer,
    settings: RenderSettings,
    /// The 3D view is only re-rendered when something changed.
    scene_dirty: bool,
    last_size: [u32; 2],
    gen_ms: f32,
    mesh_ms: f32,
    frame_ms: f32,
    scene_renders: u64,
    ui: UiState,
    sim: Sim,
    last_time: f64,
    /// Tile under the cursor, and the last one seen (kept while dragging off the map).
    hover_tile: Option<(i32, i32)>,
    last_hover_tile: Option<(i32, i32)>,
    drag_start: Option<(i32, i32)>,
    highlight: Option<Highlight>,
    roads: Roads,
    /// The road being dragged out, shown as a preview.
    road_plan: roads::Plan,
    /// Tiles (min x, min z, max x, max z) whose infrastructure texels are stale.
    infra_dirty: Option<[i32; 4]>,
    /// How far the wind has carried the clouds, in tiles.
    wind: Vec2,
    last_cloud_render: f64,
    last_cursor_px: Option<Vec2>,
    clouds_shown: bool,
    #[cfg(not(target_arch = "wasm32"))]
    devtools: crate::devtools::DevTools,
}

impl StarcityApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> StarcityApp {
        let rs = cc
            .wgpu_render_state
            .as_ref()
            .expect("starcity requires the wgpu renderer");
        win95::apply(&cc.egui_ctx);
        let params = GenParams::default();
        let mut app = StarcityApp {
            world: World::new(1, 1, 1, 0),
            camera: Camera::new(Vec3::ZERO),
            renderer: SceneRenderer::new(rs),
            settings: RenderSettings::default(),
            scene_dirty: true,
            last_size: [0, 0],
            gen_ms: 0.0,
            mesh_ms: 0.0,
            frame_ms: 0.0,
            scene_renders: 0,
            ui: UiState {
                tool: Tool::Pointer,
                wireframe: false,
                grid: true,
                ortho: false,
                clouds: true,
                #[cfg(not(target_arch = "wasm32"))]
                show_debug: crate::devtools::DevTools::show_debug(),
                #[cfg(target_arch = "wasm32")]
                show_debug: false,
                show_about: false,
            },
            sim: Sim::new(&World::new(1, 1, 1, 0), 0),
            last_time: 0.0,
            hover_tile: None,
            last_hover_tile: None,
            drag_start: None,
            highlight: None,
            roads: Roads::new(1, 1),
            road_plan: roads::Plan::default(),
            infra_dirty: None,
            wind: Vec2::ZERO,
            last_cloud_render: 0.0,
            last_cursor_px: None,
            clouds_shown: true,
            #[cfg(not(target_arch = "wasm32"))]
            devtools: crate::devtools::DevTools::from_env(),
            params,
        };
        app.regenerate(rs);
        let c = app.params.size as f32 / 2.0;
        app.camera.focus = Vec3::new(c, app.world.surface_height(c as i32, c as i32), c);
        #[cfg(not(target_arch = "wasm32"))]
        app.dev_setup(c as i32);
        app
    }

    /// Native-only: applies `STARCITY_*` dev settings (see `devtools`).
    #[cfg(not(target_arch = "wasm32"))]
    fn dev_setup(&mut self, c: i32) {
        use crate::devtools::DevTools;
        if let Some(zoom) = DevTools::zoom() {
            self.camera.screen_width = zoom;
        }
        if DevTools::demo().as_deref() == Some("roads") {
            let mut built = Vec::new();
            for (a, b) in [
                ((c - 12, c), (c + 6, c)),
                ((c - 2, c - 8), (c - 2, c + 6)),
                ((c + 6, c), (c + 6, c - 6)),
                ((c + 2, c + 2), (c + 8, c + 8)),
                ((c - 10, c - 6), (c - 4, c - 6)),
                ((c - 4, c - 6), (c + 1, c - 11)),
                ((c + 1, c - 11), (c + 6, c - 11)),
            ] {
                built.extend(roads::plan(&self.world, &self.roads, a, b).build(&mut self.roads));
            }
            self.mark_infra(&built);
            // A road being dragged, to show the preview.
            self.drag_start = Some((c - 10, c + 4));
            self.last_hover_tile = Some((c - 4, c + 10));
            self.road_plan = roads::plan(&self.world, &self.roads, (c - 10, c + 4), (c - 4, c + 10));
            let preview: Vec<_> = self
                .road_plan
                .valid
                .iter()
                .chain(&self.road_plan.blocked)
                .map(|&(t, _)| t)
                .collect();
            self.mark_infra(&preview);
            self.ui.tool = Tool::Road;
        }
    }

    fn regenerate(&mut self, rs: &RenderState) {
        let t = Instant::now();
        self.world = generation::generate(&self.params);
        self.gen_ms = t.elapsed().as_secs_f32() * 1000.0;

        let t = Instant::now();
        let mut meshes = Vec::new();
        for cz in 0..self.world.chunks_z() {
            for cx in 0..self.world.chunks_x() {
                meshes.push(mesh::mesh_chunk(&self.world, cx, cz));
            }
        }
        self.mesh_ms = t.elapsed().as_secs_f32() * 1000.0;
        self.renderer.upload_chunks(&rs.device, &meshes);
        self.sim = Sim::new(&self.world, self.params.seed as u64);
        self.roads = Roads::new(self.world.size_x, self.world.size_z);
        self.road_plan = roads::Plan::default();
        self.infra_dirty = None;
        self.drag_start = None;
        self.renderer
            .resize_infra(&rs.device, self.world.size_x as u32, self.world.size_z as u32);
        let cloud_field = clouds::generate(self.world.size_x, self.params.seed);
        self.renderer
            .upload_clouds(&rs.device, cloud_field.size_x, &clouds::mesh(&cloud_field));
        self.scene_dirty = true;
    }

    /// Re-meshes the chunks containing the given tiles. Tiles on a chunk edge
    /// also re-mesh the neighbouring chunk, since its side faces may change.
    fn remesh_tiles(&mut self, rs: &RenderState, tiles: &[(i32, i32)]) {
        if tiles.is_empty() {
            return;
        }
        let (cw, ch) = (self.world.chunks_x() as i32, self.world.chunks_z() as i32);
        let mut chunks = BTreeSet::new();
        for &(x, z) in tiles {
            for (dx, dz) in [(0, 0), (-1, 0), (1, 0), (0, -1), (0, 1)] {
                let (cx, cz) = (
                    (x + dx).div_euclid(CHUNK_SIZE as i32),
                    (z + dz).div_euclid(CHUNK_SIZE as i32),
                );
                if (0..cw).contains(&cx) && (0..ch).contains(&cz) {
                    chunks.insert((cx as usize, cz as usize));
                }
            }
        }
        for (cx, cz) in chunks {
            let mesh = mesh::mesh_chunk(&self.world, cx, cz);
            self.renderer.update_chunk(&rs.device, cz * cw as usize + cx, &mesh);
        }
        self.scene_dirty = true;
    }

    /// Clouds fade away as you zoom in, so they never fill the screen.
    fn cloud_opacity(&self) -> f32 {
        let t = ((self.camera.screen_width - 30.0) / 40.0).clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    }

    fn view_proj(&self, size: [u32; 2]) -> Mat4 {
        let aspect = size[0] as f32 / size[1] as f32;
        let radius = self.world.size_x as f32 * 1.5 + self.world.height as f32;
        self.camera.view_proj(aspect, radius)
    }

    /// Picks the hovered tile and runs the active tool. Returns tiles that changed.
    /// Picks the hovered tile and runs the active tool.
    fn handle_tool(&mut self, ctx: &egui::Context, response: &egui::Response, view_proj: Mat4) -> Changes {
        let rect = response.rect;
        let pointer = ctx
            .pointer_latest_pos()
            .filter(|_| response.hovered() || response.dragged());
        self.hover_tile = pointer.and_then(|pos| {
            let ndc = Vec2::new(
                (pos.x - rect.min.x) / rect.width() * 2.0 - 1.0,
                1.0 - (pos.y - rect.min.y) / rect.height() * 2.0,
            );
            picking::pick_tile(&self.world, &picking::screen_ray(view_proj, ndc))
        });
        if self.hover_tile.is_some() {
            self.last_hover_tile = self.hover_tile;
        }

        // Every building tool works by dragging from a start tile to the cursor.
        let mut changes = Changes::default();
        let primary = egui::PointerButton::Primary;
        let tool = self.ui.tool;
        if !matches!(tool, Tool::Bulldoze | Tool::RemoveRoad | Tool::Road) {
            self.drag_start = None;
        } else if response.drag_started_by(primary) {
            self.drag_start = self.hover_tile;
        } else if response.drag_stopped_by(primary) {
            if let (Some(a), Some(b)) = (self.drag_start.take(), self.last_hover_tile) {
                changes = self.apply_tool(a, b);
            }
        } else if response.clicked_by(primary) {
            if let Some(t) = self.hover_tile {
                changes = self.apply_tool(t, t);
            }
        }

        let plan = match (tool, self.drag_start, self.last_hover_tile) {
            (Tool::Road, Some(a), Some(b)) => roads::plan(&self.world, &self.roads, a, b),
            _ => roads::Plan::default(),
        };
        if plan != self.road_plan {
            let old = std::mem::replace(&mut self.road_plan, plan);
            let touched: Vec<_> = old
                .valid
                .iter()
                .chain(&old.blocked)
                .chain(&self.road_plan.valid)
                .chain(&self.road_plan.blocked)
                .map(|&(t, _)| t)
                .collect();
            self.mark_infra(&touched);
        }

        let highlight = match tool {
            Tool::Pointer => None,
            Tool::Bulldoze | Tool::RemoveRoad => {
                let end = if self.drag_start.is_some() {
                    self.last_hover_tile
                } else {
                    self.hover_tile
                };
                end.map(|b| Highlight {
                    a: self.drag_start.unwrap_or(b),
                    b,
                    kind: HighlightKind::Bulldoze,
                })
            }
            // While dragging a road, the preview shows where it goes.
            Tool::Road if self.drag_start.is_some() => None,
            _ => self.hover_tile.map(|t| Highlight {
                a: t,
                b: t,
                kind: HighlightKind::Cursor,
            }),
        };
        if highlight != self.highlight {
            self.highlight = highlight;
            self.scene_dirty = true;
        }
        changes
    }

    /// Applies the active tool to a dragged-out span of tiles.
    fn apply_tool(&mut self, a: (i32, i32), b: (i32, i32)) -> Changes {
        match self.ui.tool {
            Tool::Bulldoze => self
                .sim
                .clear(&mut self.world, &mut self.roads, a, b, Clear::Everything),
            Tool::RemoveRoad => self.sim.clear(&mut self.world, &mut self.roads, a, b, Clear::Roads),
            Tool::Road => {
                let plan = roads::plan(&self.world, &self.roads, a, b);
                Changes {
                    infra: plan.build(&mut self.roads),
                    ..Default::default()
                }
            }
            _ => Changes::default(),
        }
    }

    fn apply_changes(&mut self, rs: &RenderState, changes: Changes) {
        self.remesh_tiles(rs, &changes.terrain);
        self.mark_infra(&changes.infra);
    }

    /// Marks tiles whose infrastructure texels need re-uploading.
    fn mark_infra(&mut self, tiles: &[(i32, i32)]) {
        for &(x, z) in tiles {
            let r = self.infra_dirty.get_or_insert([x, z, x, z]);
            *r = [r[0].min(x), r[1].min(z), r[2].max(x), r[3].max(z)];
        }
    }

    /// Uploads the dirty rectangle of the infrastructure texture: roads plus the drag preview.
    fn flush_infra(&mut self, rs: &RenderState) {
        let Some([x0, z0, x1, z1]) = self.infra_dirty.take() else {
            return;
        };
        let (x0, z0) = (x0.max(0), z0.max(0));
        let (x1, z1) = (
            x1.min(self.roads.size_x as i32 - 1),
            z1.min(self.roads.size_z as i32 - 1),
        );
        if x0 > x1 || z0 > z1 {
            return;
        }
        let mut preview = std::collections::HashMap::new();
        for &(t, bits) in &self.road_plan.valid {
            preview.entry(t).or_insert([0u8; 2])[0] |= bits;
        }
        for &(t, bits) in &self.road_plan.blocked {
            preview.entry(t).or_insert([0u8; 2])[1] |= bits;
        }
        let mut rgba = Vec::with_capacity(((x1 - x0 + 1) * (z1 - z0 + 1) * 4) as usize);
        for z in z0..=z1 {
            for x in x0..=x1 {
                let [valid, blocked] = preview.get(&(x, z)).copied().unwrap_or_default();
                rgba.extend_from_slice(&[self.roads.get(x, z), 0, valid, blocked]);
            }
        }
        let (w, h) = ((x1 - x0 + 1) as u32, (z1 - z0 + 1) as u32);
        self.renderer.write_infra(&rs.queue, x0 as u32, z0 as u32, w, h, &rgba);
        self.scene_dirty = true;
    }

    fn handle_keys(&mut self, ctx: &egui::Context, dt: f32) {
        if ctx.egui_wants_keyboard_input() {
            return;
        }
        use egui::Key;
        ctx.input(|i| {
            if i.key_pressed(Key::F1) {
                self.ui.wireframe = !self.ui.wireframe;
            }
            if i.key_pressed(Key::F3) {
                self.ui.show_debug = !self.ui.show_debug;
            }
            if i.key_pressed(Key::R) {
                self.ui.ortho = !self.ui.ortho;
            }
            if i.key_pressed(Key::C) {
                self.ui.clouds = !self.ui.clouds;
            }
            if i.key_pressed(Key::G) {
                self.ui.grid = !self.ui.grid;
            }
            if i.key_pressed(Key::Escape) {
                self.ui.tool = Tool::Pointer;
            }

            let axis = |neg: &[Key], pos: &[Key]| {
                let down = |keys: &[Key]| keys.iter().any(|&k| i.key_down(k));
                down(pos) as i32 as f32 - down(neg) as i32 as f32
            };
            let right = axis(&[Key::A, Key::ArrowLeft], &[Key::D, Key::ArrowRight]);
            let back = axis(&[Key::W, Key::ArrowUp], &[Key::S, Key::ArrowDown]);
            let rotate = axis(&[Key::Q], &[Key::E]);
            if right != 0.0 || back != 0.0 {
                let speed = self.camera.screen_width * 0.8 * dt;
                self.camera.pan(right * speed, back * speed);
                self.scene_dirty = true;
            }
            if rotate != 0.0 {
                self.camera.orbit(rotate * 1.6 * dt, 0.0);
                self.scene_dirty = true;
            }
        });
    }

    fn handle_mouse(&mut self, ui: &egui::Ui, response: &egui::Response) {
        let delta = response.drag_delta();
        if response.dragged_by(egui::PointerButton::Secondary) {
            self.camera.orbit(-delta.x * 0.006, -delta.y * 0.006);
            self.scene_dirty = true;
        }
        // Left-drag pans only with the pointer tool; other tools will use it to build.
        let pan_button = response.dragged_by(egui::PointerButton::Middle)
            || (self.ui.tool == Tool::Pointer && response.dragged_by(egui::PointerButton::Primary));
        if pan_button {
            let units_per_point = self.camera.screen_width / response.rect.width();
            let foreshorten = (-self.camera.pitch).sin().max(0.2);
            self.camera
                .pan(-delta.x * units_per_point, -delta.y * units_per_point / foreshorten);
            self.scene_dirty = true;
        }
        if response.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0 {
                self.camera
                    .zoom((-scroll * 0.0015).exp(), self.world.size_x as f32 * 2.0);
                self.scene_dirty = true;
            }
        }
    }

    /// Keeps the focus inside the map and resting on the ground.
    fn settle_focus(&mut self, dt: f32) {
        let size = self.world.size_x as f32;
        let f = &mut self.camera.focus;
        f.x = f.x.clamp(0.0, size);
        f.z = f.z.clamp(0.0, size);
        let ground = self.world.surface_height(f.x as i32, f.z as i32);
        let diff = ground - f.y;
        if diff.abs() > 0.01 {
            f.y += diff * (dt * 10.0).min(1.0);
            self.scene_dirty = true;
        }
    }

    fn debug_window(&mut self, ctx: &egui::Context, rs: &RenderState) {
        let mut regen = false;
        let mut open = self.ui.show_debug;
        ui::win95::window(ctx, "Debug", &mut open, egui::pos2(8.0, 90.0), |ui| {
            ui.set_width(270.0);
            let info = rs.adapter.get_info();
            let s = self.renderer.stats;
            ui.label(format!("{:.1} ms/frame  ·  {:?}", self.frame_ms, info.backend));
            ui.label(format!("scene renders: {}", self.scene_renders));
            ui.label(format!(
                "chunks {}/{}  ·  quads {}/{}",
                s.chunks_drawn, s.chunks_total, s.quads_drawn, s.quads_total
            ));
            ui.label(format!(
                "vertex memory: {:.1} MB",
                s.quads_total as f32 * 4.0 * std::mem::size_of::<mesh::Vertex>() as f32 / 1e6
            ));
            ui.label(format!(
                "generate {:.1} ms  ·  mesh {:.1} ms",
                self.gen_ms, self.mesh_ms
            ));

            ui.collapsing("Camera", |ui| {
                let c = &self.camera;
                ui.label(format!("focus ({:.1}, {:.1}, {:.1})", c.focus.x, c.focus.y, c.focus.z));
                ui.label(format!(
                    "yaw {:.2}  pitch {:.2}  width {:.0}",
                    c.yaw, c.pitch, c.screen_width
                ));
                ui.label(if c.is_ortho() { "orthographic" } else { "perspective" });
            });

            ui.collapsing("Rendering", |ui| {
                let before = self.settings.clone();
                ui.add_enabled(
                    self.settings.wireframe,
                    egui::Checkbox::new(&mut self.settings.wireframe_only, "Wireframe only"),
                );
                egui::ComboBox::from_label("MSAA")
                    .selected_text(format!("{}x", self.settings.msaa))
                    .show_ui(ui, |ui| {
                        for n in [1, 4] {
                            ui.selectable_value(&mut self.settings.msaa, n, format!("{n}x"));
                        }
                    });
                ui.add(egui::Slider::new(&mut self.settings.render_scale, 0.25..=1.0).text("render scale"));
                ui.add(egui::Slider::new(&mut self.settings.texture_repeat, 0.05..=1.0).text("texture repeat"));
                if self.settings != before {
                    self.scene_dirty = true;
                }
            });

            egui::CollapsingHeader::new("Terrain generation")
                .default_open(true)
                .show(ui, |ui| {
                    let p = &mut self.params;
                    let before = p.clone();
                    ui.horizontal(|ui| {
                        ui.label("seed");
                        ui.add(egui::DragValue::new(&mut p.seed));
                        if ui.button("🎲").clicked() {
                            p.seed = (ui.input(|i| i.time) * 1e6) as u32 ^ p.seed.wrapping_mul(2654435761);
                        }
                    });
                    egui::ComboBox::from_label("map size")
                        .selected_text(format!("{0}×{0}", p.size))
                        .show_ui(ui, |ui| {
                            for n in [128, 256, 512, 1024] {
                                ui.selectable_value(&mut p.size, n, format!("{n}×{n}"));
                            }
                        });
                    ui.add(egui::Slider::new(&mut p.max_height, 8..=120).text("max height"));
                    ui.add(egui::Slider::new(&mut p.sea_level, 0..=40).text("sea level"));
                    ui.add(egui::Slider::new(&mut p.hill_scale, 30.0..=500.0).text("hill scale"));
                    ui.add(egui::Slider::new(&mut p.octaves, 1..=6).text("octaves"));
                    ui.add(egui::Slider::new(&mut p.persistence, 0.1..=0.7).text("roughness"));
                    ui.add(egui::Slider::new(&mut p.warp, 0.0..=100.0).text("warp"));
                    ui.add(egui::Slider::new(&mut p.valley_exponent, 0.5..=3.0).text("valley flattening"));
                    ui.add(egui::Slider::new(&mut p.flatness, 0.0..=1.0).text("plateaus"));
                    ui.add(egui::Slider::new(&mut p.terrace_step, 2.0..=12.0).text("terrace step"));
                    regen = *p != before;
                    if ui.button("Reset").clicked() {
                        *p = GenParams {
                            seed: p.seed,
                            ..Default::default()
                        };
                        regen = true;
                    }
                });

            ui.separator();
            ui.small("WASD/arrows or left-drag: pan · Q/E or right-drag: rotate\nscroll: zoom · R: ortho/perspective · F1: wireframe · G: grid\nEsc: pointer tool · F3: this window");
        });
        self.ui.show_debug = open;
        if regen {
            self.regenerate(rs);
        }
    }
}

impl StarcityApp {
    /// Menu bar, toolbar and status bar around the 3D view.
    fn game_ui(&mut self, ui: &mut egui::Ui, rs: &RenderState) {
        let panel = egui::Frame::NONE
            .fill(win95::FACE)
            .inner_margin(egui::Margin::symmetric(4, 2));
        let actions = egui::Panel::top("menu_bar")
            .frame(panel)
            .show(ui, |ui| ui::menu_bar(ui, &mut self.ui))
            .inner;
        egui::Panel::top("toolbar")
            .frame(panel.inner_margin(egui::Margin::symmetric(4, 3)))
            .show(ui, |ui| ui::toolbar(ui, &mut self.ui));
        let plan = &self.road_plan;
        let status = if !plan.is_empty() {
            match plan.blocked.len() {
                0 => format!("Road: {} tiles", plan.valid.len()),
                n => format!("Road: {} tiles, {n} blocked", plan.valid.len()),
            }
        } else {
            match self.hover_tile {
                Some((x, z)) => {
                    let h = self.world.surface(x, z).map_or(0, |(base, _)| base);
                    format!("Tile ({x}, {z})  height {h}")
                }
                None => format!("Zoom {:.0}", self.camera.screen_width),
            }
        };
        egui::Panel::bottom("status_bar")
            .frame(panel)
            .show(ui, |ui| ui::status_bar(ui, &self.ui, &status));

        // The menus and keyboard edit `self.ui`; apply those to the renderer and camera.
        if self.settings.wireframe != self.ui.wireframe || self.settings.grid != self.ui.grid {
            self.settings.wireframe = self.ui.wireframe;
            self.settings.grid = self.ui.grid;
            self.scene_dirty = true;
        }
        self.camera.want_ortho = self.ui.ortho;
        if self.ui.clouds != self.clouds_shown {
            self.clouds_shown = self.ui.clouds;
            self.scene_dirty = true;
        }

        if actions.new_map {
            self.params.seed = (ui.input(|i| i.time) * 1e6) as u32 ^ self.params.seed.wrapping_mul(2654435761);
            self.regenerate(rs);
        }
    }
}

impl eframe::App for StarcityApp {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let frame_start = Instant::now();
        let rs = frame.wgpu_render_state().expect("wgpu").clone();
        let ctx = ui.ctx().clone();
        let dt = ui.input(|i| i.stable_dt).min(0.1);

        self.handle_keys(&ctx, dt);
        if self.camera.update(dt) {
            self.scene_dirty = true;
        }
        self.settle_focus(dt);

        let now = ctx.input(|i| i.time);
        let clouds_visible = self.ui.clouds && self.cloud_opacity() > 0.0;
        if clouds_visible {
            self.wind += WIND * (now - self.last_time) as f32;
            // Drifting clouds don't need full frame rate.
            if now - self.last_cloud_render >= CLOUD_FRAME_SECONDS {
                self.last_cloud_render = now;
                self.scene_dirty = true;
            }
        }
        let changed = self.sim.advance(&mut self.world, (now - self.last_time) as f32);
        self.last_time = now;
        self.remesh_tiles(&rs, &changed);

        self.game_ui(ui, &rs);
        if self.ui.show_debug {
            self.debug_window(&ctx, &rs);
        }
        ui::about_window(&ctx, &mut self.ui.show_about);

        let mut scene_changed = false;
        egui::CentralPanel::default().frame(egui::Frame::NONE).show(ui, |ui| {
            let rect = ui.max_rect();
            let response = ui.allocate_rect(rect, egui::Sense::click_and_drag());
            let scale = ctx.pixels_per_point() * self.settings.render_scale;
            let size = [
                (rect.width() * scale).max(1.0) as u32,
                (rect.height() * scale).max(1.0) as u32,
            ];
            let changes = self.handle_tool(&ctx, &response, self.view_proj(size));
            self.apply_changes(&rs, changes);
            self.flush_infra(&rs);
            self.handle_mouse(ui, &response);
            // Clouds fade around the cursor, so redraw when it moves over them.
            let cursor = ctx.pointer_latest_pos().filter(|_| response.hovered());
            let cursor_px = cursor.map(|p| Vec2::new((p.x - rect.min.x) * scale, (p.y - rect.min.y) * scale));
            if clouds_visible && cursor_px != self.last_cursor_px {
                self.scene_dirty = true;
            }
            self.last_cursor_px = cursor_px;

            if size != self.last_size {
                self.last_size = size;
                self.scene_dirty = true;
            }

            if self.scene_dirty {
                scene_changed = true;
                let view_proj = self.view_proj(size);
                let camera_pos = if self.camera.is_ortho() {
                    (-self.camera.forward()).extend(0.0)
                } else {
                    self.camera.eye().extend(1.0)
                };
                let clouds = CloudState {
                    wind: self.wind,
                    altitude: (self.params.max_height as f32 + 6.0) * crate::terrain::BLOCK_HEIGHT,
                    opacity: if self.ui.clouds { self.cloud_opacity() } else { 0.0 },
                    cursor: cursor_px,
                    fade_radius: 220.0 * scale,
                };
                self.renderer
                    .render(&rs, size, &self.settings, view_proj, camera_pos, self.highlight, clouds);
                self.scene_renders += 1;
                self.scene_dirty = false;
            }
            if let Some(id) = self.renderer.texture_id() {
                let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
                ui.painter().image(id, rect, uv, egui::Color32::WHITE);
            }
        });

        // Keep frames coming only while something is moving (including egui's
        // smoothed scrolling, which spreads one wheel tick over several frames).
        let keys_held = ctx.input(|i| !i.keys_down.is_empty());
        if keys_held || scene_changed || self.scene_dirty {
            ctx.request_repaint();
        } else if clouds_visible {
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(CLOUD_FRAME_SECONDS));
        } else if self.sim.is_active() {
            ctx.request_repaint_after(std::time::Duration::from_secs_f32(sim::TICK_SECONDS));
        }
        #[cfg(not(target_arch = "wasm32"))]
        self.devtools.update(&ctx);
        let ms = frame_start.elapsed().as_secs_f32() * 1000.0;
        self.frame_ms = self.frame_ms * 0.9 + ms * 0.1;
    }
}
