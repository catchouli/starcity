mod company_ui;
mod craft;

use crate::boats::Boats;
use crate::camera::Camera;
use crate::clock::{Clock, Speed};
use crate::clouds;
use crate::company::{self, Assets, Company, Facts, Item};
use crate::daylight::{self, Light};
use crate::hq::Hq;
use crate::input::PhysicalKeys;
use crate::models::Models;
use crate::network::{Blocked, Layer, Plan};
use crate::picking;
use crate::planes::Planes;
use crate::props;
use crate::render::{CloudState, Highlight, HighlightKind, ModelLayer, RenderSettings, SceneRenderer};
use crate::sea::Sea;
use crate::sim::{self, Changes, Clear, Sim};
use crate::stations::{StationId, Stations};
use crate::terminals::{TerminalId, Terminals};
use crate::terrain::generation::{self, GenParams};
use crate::terrain::{CHUNK_SIZE, World, mesh};
use crate::towns::{TownId, Towns};
use crate::trains::{SpawnError, Trains};
use crate::ui::{self, Tool, UiState, win95};
use crate::vehicles::Vehicles;
use crate::{rails, roads};
use craft::Craft;
use eframe::egui;
use eframe::egui_wgpu::RenderState;
use glam::{Mat4, Vec2, Vec3};
use std::collections::{BTreeSet, HashMap};
use web_time::Instant;

/// Wind speed and direction, in tiles per second.
const WIND: Vec2 = Vec2::new(0.6, 0.25);
/// Redraw interval while the only thing moving is the clouds.
const CLOUD_FRAME_SECONDS: f64 = 1.0 / 15.0;
/// Longest simulation step, and the most simulated time per frame.
const MAX_SIM_STEP: f32 = 0.1;
const MAX_SIM_PER_FRAME: f32 = 0.4;
/// The light is re-evaluated at most this often.
const LIGHT_FRAME_SECONDS: f64 = 0.1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Network {
    Road,
    Rail,
}

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
    physical_keys: PhysicalKeys,
    sim: Sim,
    last_time: f64,
    /// Tile under the cursor, and the last one seen (kept while dragging off the map).
    hover_tile: Option<(i32, i32)>,
    last_hover_tile: Option<(i32, i32)>,
    drag_start: Option<(i32, i32)>,
    highlight: Option<Highlight>,
    roads: Layer,
    rails: Layer,
    signals: rails::Signals,
    trains: Trains,
    stations: Stations,
    towns: Towns,
    vehicles: Vehicles,
    vehicles_moving: bool,
    /// Every model there is (assets/models/*.glb).
    models: Models,
    /// The calendar year, which decides which models are in use.
    year: i32,
    /// The water grid; built the first time boats or ports need it.
    sea: Option<Sea>,
    sea_ms: f32,
    terminals: Terminals,
    boats: Boats,
    planes: Planes,
    selected_craft: Option<Craft>,
    selected_terminal: Option<TerminalId>,
    craft_moving: bool,
    /// Which way the airport tool lays its runway (O toggles).
    runway_along_x: bool,
    /// Price or reason shown in the status bar for the port, airport, boat and plane tools.
    site_hint: Option<String>,
    /// Terrain tiles levelled before the first frame (demo airports), to re-mesh.
    demo_levelled: Vec<(i32, i32)>,
    /// Buildings or stations changed, so the static prop boxes need rebuilding.
    static_changed: bool,
    selected_town: Option<TownId>,
    /// Seconds since the last town and passenger update.
    town_clock: f32,
    /// Bumped whenever track changes, so trains re-plan their routes.
    track_version: u64,
    selected_train: Option<u32>,
    selected_station: Option<StationId>,
    /// View-projection and 3D view rectangle from the last frame, for picking
    /// trains and placing labels.
    last_view: Option<(Mat4, egui::Rect)>,
    /// Signals or trains changed, so the prop boxes need rebuilding.
    props_changed: bool,
    trains_moving: bool,
    clock: Clock,
    company: Company,
    /// The headquarters, in the company town.
    hq: Option<Hq>,
    /// What the company owns and can do, refreshed once a second.
    assets: Assets,
    facts: Facts,
    /// Road pieces the player has built (towns' own roads don't cost upkeep).
    player_roads: u32,
    assets_version: u64,
    /// The speed to go back to when un-pausing with Space.
    resume_speed: Speed,
    /// The light the scene was last drawn with.
    light: Light,
    /// How far the day/night cycle is allowed to go, eased as speed or the setting changes.
    light_strength: f32,
    last_light_render: f64,
    /// A short-lived status bar message and when it expires.
    message: Option<(String, f64)>,
    /// The road or track being dragged out, shown as a preview.
    plan: Plan,
    /// Which network `plan` is for.
    plan_network: Network,
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
    pub fn new(cc: &eframe::CreationContext<'_>, demo: bool) -> StarcityApp {
        let rs = cc
            .wgpu_render_state
            .as_ref()
            .expect("starcity requires the wgpu renderer");
        win95::apply(&cc.egui_ctx);
        let params = GenParams::default();
        let models = Models::load_embedded();
        let mut renderer = SceneRenderer::new(rs);
        renderer.upload_models(&rs.device, &models);
        let mut app = StarcityApp {
            world: World::new(1, 1, 1, 0),
            camera: Camera::new(Vec3::ZERO),
            renderer,
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
                day_night: true,
                speed: Speed::X1,
                #[cfg(not(target_arch = "wasm32"))]
                show_debug: crate::devtools::DevTools::show_debug(),
                #[cfg(target_arch = "wasm32")]
                show_debug: false,
                show_about: false,
                show_finance: false,
                show_hq: false,
                show_next_step: true,
                go_home: false,
            },
            physical_keys: PhysicalKeys::default(),
            sim: Sim::new(&World::new(1, 1, 1, 0), 0),
            last_time: 0.0,
            hover_tile: None,
            last_hover_tile: None,
            drag_start: None,
            highlight: None,
            roads: Layer::new(1, 1),
            rails: Layer::new(1, 1),
            signals: rails::Signals::default(),
            trains: Trains::new(1),
            stations: Stations::default(),
            towns: Towns::empty(1, 1),
            vehicles: Vehicles::new(1),
            vehicles_moving: false,
            models,
            year: 2000,
            sea: None,
            sea_ms: 0.0,
            terminals: Terminals::default(),
            boats: Boats::default(),
            planes: Planes::default(),
            selected_craft: None,
            selected_terminal: None,
            craft_moving: false,
            runway_along_x: true,
            site_hint: None,
            demo_levelled: Vec::new(),
            static_changed: false,
            selected_town: None,
            town_clock: 0.0,
            track_version: 0,
            selected_train: None,
            selected_station: None,
            last_view: None,
            props_changed: false,
            trains_moving: false,
            clock: Clock::default(),
            company: Company::new(2000),
            hq: None,
            assets: Assets::default(),
            facts: Facts::default(),
            player_roads: 0,
            assets_version: u64::MAX,
            resume_speed: Speed::X1,
            light: daylight::NOON,
            light_strength: 1.0,
            last_light_render: 0.0,
            message: None,
            plan: Plan::default(),
            plan_network: Network::Road,
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
        app.company.sandbox = demo;
        if demo {
            app.start_demo();
            let tiles = std::mem::take(&mut app.demo_levelled);
            app.remesh_tiles(rs, &tiles);
            if let Some(sea) = &mut app.sea {
                sea.refresh(&app.world, &tiles);
            }
        }
        let c = app.params.size as f32 / 2.0;
        app.camera.focus = Vec3::new(c, app.world.surface_height(c as i32, c as i32), c);
        if !demo {
            // A new game opens on your garage.
            app.look_at_hq();
        }
        #[cfg(not(target_arch = "wasm32"))]
        app.dev_setup(c as i32);
        app
    }

    /// Native-only: applies `STARCITY_*` dev settings (see `devtools`).
    #[cfg(not(target_arch = "wasm32"))]
    fn dev_setup(&mut self, c: i32) {
        use crate::devtools::DevTools;
        if DevTools::demo().is_some() {
            self.company.sandbox = true;
        }
        if let Some(year) = DevTools::year() {
            self.seek_to(year as f64);
        }
        if let Some(hour) = DevTools::hour() {
            self.clock = Clock::starting_at(hour);
        }
        if let Some(zoom) = DevTools::zoom() {
            self.camera.view_height = zoom;
        }
        if let Some((dx, dz)) = std::env::var("STARCITY_FOCUS").ok().and_then(|v| {
            let (x, z) = v.split_once(',')?;
            Some((x.trim().parse::<f32>().ok()?, z.trim().parse::<f32>().ok()?))
        }) {
            self.camera.focus.x += dx;
            self.camera.focus.z += dz;
        }
        if DevTools::demo().as_deref() == Some("towns") {
            // Look at the biggest town.
            if let Some(town) = self.towns.list.iter().max_by_key(|t| self.towns.population(t.id)) {
                self.camera.focus.x = town.centre.0 as f32 + 0.5;
                self.camera.focus.z = town.centre.1 as f32 + 0.5;
            }
        }
        if DevTools::demo().as_deref() == Some("tools") {
            self.dev_try_sea_tools();
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
                let plan = roads::plan(&self.world, &self.roads, &self.rails, &self.towns.occupied, a, b);
                if let Some(why) = plan.blocked {
                    eprintln!("demo road {a:?} -> {b:?} blocked: {why:?}");
                }
                built.extend(plan.build(&mut self.roads));
            }
            for (a, b) in [
                ((c - 12, c + 14), (c + 2, c + 14)),
                ((c + 2, c + 14), (c + 8, c + 20)),
                ((c - 6, c + 14), (c, c + 20)),
                ((c - 12, c + 17), (c - 6, c + 17)),
            ] {
                let plan = rails::plan(
                    &self.world,
                    &self.rails,
                    &self.roads,
                    &self.towns.occupied,
                    &self.signals,
                    a,
                    b,
                );
                if let Some(why) = plan.blocked {
                    eprintln!("demo track {a:?} -> {b:?} blocked: {why:?}");
                }
                built.extend(plan.build(&mut self.rails));
            }
            self.mark_infra(&built);
            for (t, forward) in [
                ((c - 9, c + 14), true),
                ((c - 2, c + 14), false),
                ((c - 9, c + 17), true),
            ] {
                self.signals.click(&self.rails, t, forward);
            }
            // Two stations on the long line, and a train running between them.
            let mut station_ids = Vec::new();
            for (a, b) in [((c - 12, c + 14), (c - 10, c + 14)), ((c - 1, c + 14), (c + 1, c + 14))] {
                let plan = self.stations.plan(
                    &self.world,
                    &self.rails,
                    &self.roads,
                    &self.towns.occupied,
                    &self.signals,
                    a,
                    b,
                );
                if let Some(why) = plan.blocked {
                    eprintln!("demo station {a:?} blocked: {why:?}");
                }
                self.stations.build(&plan, &mut self.rails);
                station_ids.extend(self.stations.at(a));
            }
            for (t, forward) in [((c - 6, c + 14), true), ((c - 10, c + 17), false)] {
                match self.trains.spawn(&self.rails, t, forward, 3) {
                    Ok(id) if t.1 == c + 14 => {
                        self.trains
                            .set_orders(id, station_ids.iter().rev().copied().collect(), 0);
                        self.selected_train = Some(id);
                    }
                    Ok(_) => {}
                    Err(e) => eprintln!("demo train at {t:?}: {e:?}"),
                }
            }
            self.props_changed = true;
            self.static_changed = true;
            // A road being dragged, to show the preview.
            self.drag_start = Some((c - 10, c + 4));
            self.last_hover_tile = Some((c - 4, c + 10));
            self.ui.tool = Tool::Road;
        }
    }

    /// Demo mode (`--demo` / `?demo`): rail lines between neighbouring towns,
    /// each with a train running back and forth.
    fn start_demo(&mut self) {
        let mut railway = crate::scenario::Railway {
            world: &self.world,
            roads: &self.roads,
            buildings: &self.towns.occupied,
            signals: &self.signals,
            rails: &mut self.rails,
            stations: &mut self.stations,
        };
        let lines = crate::scenario::build_lines(&mut railway, &self.towns, 12);
        let mut running = 0;
        for line in &lines {
            if let Ok(id) = self.trains.spawn(&self.rails, line.start, line.forward, 3) {
                self.trains.set_orders(id, vec![line.to, line.from], 0);
                running += 1;
            }
        }
        let ids: Vec<StationId> = self.stations.list.keys().copied().collect();
        for id in ids {
            self.name_station(id);
        }
        let last = self.world.size_x as i32 - 1;
        self.infra_dirty = Some([0, 0, last, self.world.size_z as i32 - 1]);
        self.static_changed = true;
        self.props_changed = true;
        self.track_version += 1;
        let (ports, boats, airports, planes) = self.demo_sea_and_air();
        log::info!(
            "demo: {ports} ports, {} boats, {airports} airports, {} planes",
            self.boats.list.len(),
            self.planes.list.len()
        );
        self.message = Some((
            format!("Demo: {} rail, {boats} ferries, {planes} planes", lines.len()),
            10.0,
        ));
        let _ = running;
    }

    /// Dev check: drives the port, airport, ferry and plane tools the way the
    /// mouse would, and prints what they said.
    #[cfg(not(target_arch = "wasm32"))]
    fn dev_try_sea_tools(&mut self) {
        let mut done = Vec::new();
        for tool in [Tool::Port, Tool::Airport, Tool::Boat, Tool::Plane] {
            // Hover across the map until the tool is happy, then click.
            'scan: for z in (8..self.world.size_z as i32 - 8).step_by(3) {
                for x in (8..self.world.size_x as i32 - 8).step_by(3) {
                    let tile = (x, z);
                    self.hover_tile = Some(tile);
                    let before = (self.terminals.list.len(), self.boats.list.len(), self.planes.list.len());
                    let hover = self.sea_tool_hover(tool);
                    let hint = self.site_hint.clone().unwrap_or_default();
                    if hover.is_some_and(|h| h.kind == HighlightKind::Cursor) {
                        let terrain = self.sea_tool_click(tool, tile, 0.0);
                        let after = (self.terminals.list.len(), self.boats.list.len(), self.planes.list.len());
                        if after != before {
                            let line = format!(
                                "{tool:?} at {tile:?}: \"{hint}\" -> {after:?}, levelled {} tiles",
                                terrain.len()
                            );
                            done.push(line);
                            self.demo_levelled.extend(terrain);
                            break 'scan;
                        }
                    }
                }
            }
        }
        // Give the new ferry and plane orders, as the window would.
        if let (Some(b), Some(p)) = (
            self.boats.list.first().map(|b| b.id),
            self.planes.list.first().map(|p| p.id),
        ) {
            let ports: Vec<_> = self
                .terminals
                .of_kind(crate::terminals::Kind::Port)
                .map(|t| t.id)
                .collect();
            let airports: Vec<_> = self
                .terminals
                .of_kind(crate::terminals::Kind::Airport)
                .map(|t| t.id)
                .collect();
            self.boats.set_orders(b, crate::terminals::Orders::new(ports));
            self.planes.set_orders(p, crate::terminals::Orders::new(airports));
        }
        for line in &done {
            eprintln!("tools: {line}");
        }
        let names: Vec<_> = self.terminals.list.values().map(|t| t.name.clone()).collect();
        eprintln!("tools: {} of 4 tools worked; terminals: {names:?}", done.len());
    }

    /// Jumps the game to a date (a year with a fraction: 2025.5 is mid-2025),
    /// for testing. Going forward plays the years in between (towns grow and
    /// are redeveloped, fleets turn over); going back re-ages everything built
    /// later, so the world looks like it did at that date.
    fn seek_to(&mut self, target: f64) {
        let year = target.floor() as i32;
        while self.year < year {
            let next = self.year + 1;
            self.clock.set_year_f64(next as f64);
            self.set_year(next);
            // A year of town life in 30-second steps, then fleets renew themselves.
            for _ in 0..12 {
                self.towns
                    .update(30.0, &self.world, &mut self.roads, &self.rails, &self.models);
            }
            for _ in 0..20 {
                self.vehicles.update(1.0, &self.roads, &self.towns);
            }
        }
        if year < self.year {
            self.towns.rewind(year);
            self.vehicles.rewind(year);
            for t in &mut self.trains.list {
                t.born = t.born.min(year);
            }
            for b in &mut self.boats.list {
                b.born = b.born.min(year);
            }
            for p in &mut self.planes.list {
                p.born = p.born.min(year);
            }
            self.set_year(year);
        }
        self.clock.set_year_f64(target);
        self.renderer.era_chrome = crate::era::chrome(target);
        self.static_changed = true;
        self.props_changed = true;
        self.scene_dirty = true;
    }

    /// A new calendar year: new vehicles and buildings take the new look, and
    /// stations, ports and airports are refurbished to match.
    fn set_year(&mut self, year: i32) {
        self.year = year;
        self.trains.year = year;
        self.vehicles.year = year;
        self.boats.year = year;
        self.planes.year = year;
        self.towns.year = year;
        self.static_changed = true;
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
        self.roads = Layer::new(self.world.size_x, self.world.size_z);
        self.rails = Layer::new(self.world.size_x, self.world.size_z);
        self.signals = rails::Signals::default();
        self.trains = Trains::new(self.params.seed as u64);
        self.stations = Stations::default();
        self.selected_train = None;
        self.selected_station = None;
        self.selected_town = None;
        self.towns = Towns::generate(&self.world, &mut self.roads, &self.rails, self.params.seed, self.year);
        self.vehicles = Vehicles::new(self.params.seed as u64);
        self.sea = None;
        self.terminals = Terminals::default();
        self.boats = Boats::default();
        self.planes = Planes::default();
        self.selected_craft = None;
        self.selected_terminal = None;
        self.new_company();
        self.set_year(self.year);
        self.props_changed = true;
        self.static_changed = true;
        self.plan = Plan::default();
        self.drag_start = None;
        self.renderer
            .resize_infra(&rs.device, self.world.size_x as u32, self.world.size_z as u32);
        // Towns come with roads: upload the whole infrastructure layer.
        let last = self.world.size_x as i32 - 1;
        self.infra_dirty = Some([0, 0, last, self.world.size_z as i32 - 1]);
        let block = crate::terrain::BLOCK_HEIGHT;
        let size = self.world.size_x as f32;
        self.renderer.ocean = [
            size,
            self.world.sea_level as f32 * block,
            generation::seabed(&self.params) as f32 * block,
            size * 3.0,
        ];
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

    /// Points the camera back at the middle of the island.
    fn centre_on_island(&mut self) {
        let c = self.world.size_x as f32 / 2.0;
        self.camera.focus.x = c;
        self.camera.focus.z = c;
        self.scene_dirty = true;
    }

    /// Clouds fade away as you zoom in, so they never fill the screen.
    fn cloud_opacity(&self) -> f32 {
        let t = ((self.camera.view_height - 20.0) / 25.0).clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    }

    fn view_proj(&self, size: [u32; 2]) -> Mat4 {
        let aspect = size[0] as f32 / size[1] as f32;
        // Covers the map plus the endless ocean out to where it has faded into the sky.
        let radius = self.world.size_x as f32 * 3.0 + self.world.height as f32;
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
        if !self.company.tool_unlocked(self.ui.tool) {
            self.ui.tool = Tool::Pointer;
        }
        let tool = self.ui.tool;
        if tool == Tool::Pointer && response.clicked_by(primary) {
            self.select_at(ctx, rect, view_proj);
        }
        if let (Tool::Train, Some(t), true) = (tool, self.hover_tile, response.clicked_by(primary)) {
            // Trains set off away from the camera, like signals face it.
            let piece = self.rails.get(t.0, t.1);
            let forward = piece == 0 || {
                let ((fx, fz), (tx, tz)) = rails::piece_ends(piece);
                let view = self.camera.forward();
                (tx - fx) * view.x + (tz - fz) * view.z >= 0.0
            };
            let now = ctx.input(|i| i.time);
            if !self.can_add_vehicle(company::TRAIN) {
                return changes;
            }
            match self.trains.spawn(&self.rails, t, forward, 3) {
                Ok(_) => {
                    self.company.spend(Item::Vehicles, company::TRAIN);
                    self.props_changed = true;
                }
                Err(SpawnError::NoTrack) => self.show_message("Trains go on track", now),
                Err(SpawnError::Junction) => self.show_message("Place trains on plain track, not junctions", now),
                Err(SpawnError::TooShort) => self.show_message("Not enough track behind here for a train", now),
                Err(SpawnError::Occupied) => self.show_message("There's already a train there", now),
            }
        }
        if let (Tool::Signal, Some(t)) = (tool, self.hover_tile) {
            if response.clicked_by(primary) {
                // A new signal faces the camera: trains it controls travel away from the viewer.
                let ((fx, fz), (tx, tz)) = rails::piece_ends(self.rails.get(t.0, t.1));
                let view = self.camera.forward();
                let forward = (tx - fx) * view.x + (tz - fz) * view.z >= 0.0;
                let new = matches!(self.signals.click_effect(&self.rails, t), rails::SignalClick::Place);
                if !new || self.pay(Item::Construction, company::SIGNAL) {
                    self.props_changed |= self.signals.click(&self.rails, t, forward);
                }
            } else if response.clicked_by(egui::PointerButton::Secondary) {
                self.props_changed |= self.signals.remove(t);
            }
        }
        let mut sea_terrain = Vec::new();
        if matches!(tool, Tool::Port | Tool::Airport | Tool::Boat | Tool::Plane) {
            if let (Some(t), true) = (self.hover_tile, response.clicked_by(primary)) {
                let now = ctx.input(|i| i.time);
                sea_terrain = self.sea_tool_click(tool, t, now);
            }
        } else {
            self.site_hint = None;
        }
        if !matches!(
            tool,
            Tool::Bulldoze | Tool::RemoveRoad | Tool::Road | Tool::Rail | Tool::RemoveRail | Tool::Station
        ) {
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

        changes.terrain.extend(sea_terrain);

        let (plan, network) = match (tool, self.drag_start, self.last_hover_tile) {
            (Tool::Road, Some(a), Some(b)) => (
                roads::plan(&self.world, &self.roads, &self.rails, &self.towns.occupied, a, b),
                Network::Road,
            ),
            (Tool::Rail, Some(a), Some(b)) => (
                rails::plan_terraforming(
                    &self.world,
                    &self.rails,
                    &self.roads,
                    &self.towns.occupied,
                    &self.signals,
                    a,
                    b,
                ),
                Network::Rail,
            ),
            (Tool::Station, Some(a), Some(b)) => {
                let sp = self.stations.plan(
                    &self.world,
                    &self.rails,
                    &self.roads,
                    &self.towns.occupied,
                    &self.signals,
                    a,
                    b,
                );
                let pieces = sp
                    .tiles
                    .iter()
                    .map(|&t| crate::network::Piece::add(t, sp.piece))
                    .collect();
                (
                    Plan {
                        pieces,
                        blocked: sp.blocked,
                        ..Default::default()
                    },
                    Network::Rail,
                )
            }
            _ => (Plan::default(), self.plan_network),
        };
        if plan != self.plan || network != self.plan_network {
            let old = std::mem::replace(&mut self.plan, plan);
            self.plan_network = network;
            self.renderer.preview_is_rail = network == Network::Rail;
            let touched: Vec<_> = old.tiles().chain(self.plan.tiles()).collect();
            self.mark_infra(&touched);
        }

        let highlight = match tool {
            Tool::Pointer => None,
            Tool::Bulldoze | Tool::RemoveRoad | Tool::RemoveRail => {
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
            // While dragging a road or track, the preview shows where it goes.
            Tool::Road | Tool::Rail | Tool::Station if self.drag_start.is_some() => None,
            Tool::Port | Tool::Airport | Tool::Boat | Tool::Plane => self.sea_tool_hover(tool),
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

    /// Pointer click: selects the train under the cursor, or else the station.
    fn select_at(&mut self, ctx: &egui::Context, rect: egui::Rect, view_proj: Mat4) {
        let Some(pos) = ctx.pointer_latest_pos() else { return };
        let mut best: Option<(f32, u32)> = None;
        for train in &self.trains.list {
            for (front, back) in train.car_ends() {
                let c = (front + back) / 2.0;
                let p = Vec3::new(c.x, self.world.height_at(c.x, c.y) + 0.25, c.y);
                if let Some(screen) = project(view_proj, rect, p) {
                    let d = screen.distance(pos);
                    if d < 28.0 && best.is_none_or(|(bd, _)| d < bd) {
                        best = Some((d, train.id));
                    }
                }
            }
        }
        self.selected_train = best.map(|(_, id)| id);
        self.selected_station = None;
        self.selected_town = None;
        self.selected_craft = None;
        self.selected_terminal = None;
        if best.is_none() {
            self.selected_craft = self.pick_craft(view_proj, rect, pos);
        }
        if best.is_none() && self.selected_craft.is_none() {
            if let Some(t) = self.hover_tile {
                self.selected_terminal = self.terminals.at(t);
                if self.hq.is_some_and(|h| h.contains(t)) {
                    self.ui.show_hq = true;
                } else if self.selected_terminal.is_none() {
                    self.selected_station = self.stations.at(t);
                    if self.selected_station.is_none() {
                        self.selected_town = self.towns.buildings.get(&t).map(|b| b.town);
                    }
                }
            }
        }
    }

    /// Applies the active tool to a dragged-out span of tiles.
    fn apply_tool(&mut self, a: (i32, i32), b: (i32, i32)) -> Changes {
        match self.ui.tool {
            Tool::Bulldoze => self.clear(a, b, Clear::Everything),
            Tool::RemoveRoad => self.clear(a, b, Clear::Roads),
            Tool::RemoveRail => self.clear(a, b, Clear::Rails),
            Tool::Road => {
                let plan = roads::plan(&self.world, &self.roads, &self.rails, &self.towns.occupied, a, b);
                if plan.blocked.is_none() && !plan.is_empty() {
                    let cost = plan.pieces.len() as u32 * company::ROAD_PIECE;
                    if !self.pay(Item::Construction, cost) {
                        return Changes::default();
                    }
                    self.player_roads += plan.pieces.len() as u32;
                }
                Changes {
                    infra: plan.build(&mut self.roads),
                    ..Default::default()
                }
            }
            Tool::Rail => {
                let plan = rails::plan_terraforming(
                    &self.world,
                    &self.rails,
                    &self.roads,
                    &self.towns.occupied,
                    &self.signals,
                    a,
                    b,
                );
                if plan.blocked.is_none() && !plan.is_empty() {
                    let cost = plan.pieces.len() as u32 * company::RAIL_PIECE + plan.terraform_cost();
                    if !self.pay(Item::Construction, cost) {
                        return Changes::default();
                    }
                }
                let terrain = if plan.blocked.is_none() {
                    crate::terrain::edit::apply(&mut self.world, &plan.terraform)
                } else {
                    Vec::new()
                };
                if !terrain.is_empty() {
                    // The ground moved: buildings, signals and trains sit at new heights.
                    self.static_changed = true;
                    self.props_changed = true;
                }
                Changes {
                    infra: plan.build(&mut self.rails),
                    terrain,
                }
            }
            Tool::Station => {
                let plan = self.stations.plan(
                    &self.world,
                    &self.rails,
                    &self.roads,
                    &self.towns.occupied,
                    &self.signals,
                    a,
                    b,
                );
                if plan.blocked.is_none() && !plan.tiles.is_empty() {
                    let cost = plan.tiles.len() as u32 * company::STATION_TILE;
                    if !self.pay(Item::Construction, cost) {
                        return Changes::default();
                    }
                }
                if plan.blocked.is_none() {
                    // Platforms on existing track change routing without changing the track.
                    self.track_version += 1;
                    self.static_changed = true;
                }
                let before: Vec<StationId> = self.stations.list.keys().copied().collect();
                let infra = self.stations.build(&plan, &mut self.rails);
                let new: Vec<StationId> = self
                    .stations
                    .list
                    .keys()
                    .copied()
                    .filter(|id| !before.contains(id))
                    .collect();
                for id in new {
                    self.name_station(id);
                }
                Changes {
                    infra,
                    ..Default::default()
                }
            }
            _ => Changes::default(),
        }
    }

    fn clear(&mut self, a: (i32, i32), b: (i32, i32), what: Clear) -> Changes {
        let changes = self
            .sim
            .clear(&mut self.world, &mut self.roads, &mut self.rails, a, b, what);
        // Trains go with the track under them.
        let tiles: std::collections::HashSet<_> = changes.infra.iter().copied().collect();
        if what != Clear::Roads && self.trains.remove_on_tiles(&tiles) {
            self.props_changed = true;
        }
        if what == Clear::Everything {
            if self.towns.demolish(a, b) {
                self.static_changed = true;
            }
            self.remove_terminals(a, b);
        }
        changes
    }

    /// Names a new station after the nearest town, adding a compass point if
    /// the town already has a station by that name.
    fn name_station(&mut self, id: StationId) {
        let Some(station) = self.stations.get(id) else { return };
        let (x, z) = station.centre();
        let Some(town) = self.towns.nearest((x as i32, z as i32), 30) else {
            return;
        };
        let town = &self.towns.list[town as usize];
        let (dx, dz) = (x - town.centre.0 as f32, z - town.centre.1 as f32);
        let compass = if dx.abs() > dz.abs() {
            if dx > 0.0 { "East" } else { "West" }
        } else if dz > 0.0 {
            "South"
        } else {
            "North"
        };
        let taken = |name: &str| self.stations.list.values().any(|s| s.id != id && s.name == name);
        let candidates = [
            town.name.clone(),
            format!("{} {compass}", town.name),
            format!("{} Central", town.name),
        ];
        if let Some(name) = candidates.into_iter().find(|n| !taken(n)) {
            self.stations.list.get_mut(&id).unwrap().name = name;
        }
    }

    /// Once a second: towns grow and stations collect passengers.
    fn tick_towns(&mut self, dt: f32) {
        self.town_clock += dt;
        if self.town_clock < 1.0 {
            return;
        }
        self.town_clock -= 1.0;
        self.refresh_assets();
        let (road_tiles, built) = self
            .towns
            .update(1.0, &self.world, &mut self.roads, &self.rails, &self.models);
        self.mark_infra(&road_tiles);
        if built {
            self.static_changed = true;
        }
        for station in self.stations.list.values_mut() {
            // Each person nearby makes about one trip every 100 seconds.
            let people = self.towns.population_near(&station.tiles) as f32;
            station.waiting = (station.waiting + people * 0.01).min(500.0);
        }
        self.tick_terminals();
    }

    /// Trains that have just pulled in: unload everyone, then load from the platform.
    fn handle_arrivals(&mut self) {
        for (train, station) in std::mem::take(&mut self.trains.arrivals) {
            let Some(st) = self.stations.list.get_mut(&station) else {
                continue;
            };
            let t = self.trains.train(train);
            let delivered = std::mem::take(&mut t.passengers);
            let came_from = t.boarded.take();
            let boarding = (st.waiting as u32).min(t.capacity());
            t.passengers = boarding;
            st.waiting -= boarding as f32;
            let (x, z) = st.centre();
            if boarding > 0 {
                t.boarded = Some((x, z, self.clock.days()));
            }
            self.pay_fare(delivered, came_from, (x, z));
            if let Some(town) = self.towns.nearest((x as i32, z as i32), 30) {
                let town = &mut self.towns.list[town as usize];
                town.served += delivered as f32;
                town.delivered += delivered;
            }
        }
    }

    fn apply_changes(&mut self, rs: &RenderState, changes: Changes) {
        if !changes.infra.is_empty() {
            self.track_version += 1;
        }
        self.remesh_tiles(rs, &changes.terrain);
        if let Some(sea) = &mut self.sea {
            sea.refresh(&self.world, &changes.terrain);
        }
        self.mark_infra(&changes.infra);
        self.update_props(rs);
    }

    /// Rebuilds the prop boxes if signals, trains or the track under them changed.
    fn update_props(&mut self, rs: &RenderState) {
        if self.signals.prune(&self.rails) {
            self.props_changed = true;
        }
        let platforms = self.stations.tile_count();
        for gone in self.stations.prune(&self.rails) {
            self.trains.forget_station(gone);
            if self.selected_station == Some(gone) {
                self.selected_station = None;
            }
        }
        if self.stations.tile_count() != platforms {
            self.static_changed = true;
            self.track_version += 1;
        }
        if std::mem::take(&mut self.static_changed) {
            let mut placed = props::stations(&self.models, &self.world, &self.rails, &self.stations, self.year);
            placed.extend(props::buildings(&self.models, &self.world, &self.towns));
            placed.extend(props::terminals(&self.models, &self.world, &self.terminals, self.year));
            if let Some(hq) = &self.hq {
                placed.extend(props::hq(&self.models, &self.world, hq, self.company.hq_level));
            }
            self.renderer
                .set_models(&rs.device, &rs.queue, ModelLayer::Static, &mut placed);
            self.scene_dirty = true;
        }
        if std::mem::take(&mut self.props_changed) {
            let mut placed = props::signals(&self.models, &self.world, &self.signals, &self.trains);
            placed.extend(props::trains(&self.models, &self.world, &self.trains));
            placed.extend(props::road_vehicles(&self.models, &self.world, &self.vehicles));
            placed.extend(props::boats(&self.models, &self.world, &self.boats));
            placed.extend(props::planes(&self.models, &self.world, &self.planes));
            self.renderer
                .set_models(&rs.device, &rs.queue, ModelLayer::Dynamic, &mut placed);
            self.scene_dirty = true;
        }
    }

    /// Win95 tooltip-style name labels floating above stations.
    fn station_labels(&self, ui: &egui::Ui, view_proj: Mat4, rect: egui::Rect) {
        let painter = ui.painter_at(rect);
        // Town names (with population) show from further out than station names.
        if self.camera.view_height < 500.0 {
            for town in &self.towns.list {
                let (x, z) = (town.centre.0 as f32 + 0.5, town.centre.1 as f32 + 0.5);
                let p = Vec3::new(x, self.world.height_at(x, z) + 3.0, z);
                let Some(at) = project(view_proj, rect, p) else {
                    continue;
                };
                let text = format!("{}\n{}", town.name, self.towns.population(town.id));
                let galley = painter.layout(
                    text,
                    egui::FontId::proportional(14.0),
                    egui::Color32::WHITE,
                    f32::INFINITY,
                );
                let r = egui::Rect::from_center_size(at, galley.size() + egui::vec2(10.0, 4.0));
                painter.rect_filled(r, 0.0, egui::Color32::from_rgba_unmultiplied(0, 0, 128, 200));
                painter.galley(r.min + egui::vec2(5.0, 2.0), galley, egui::Color32::WHITE);
            }
        }
        if self.camera.view_height > 150.0 {
            return;
        }
        let font = egui::FontId::proportional(12.0);
        for station in self.stations.list.values() {
            let (x, z) = station.centre();
            let p = Vec3::new(x, self.world.height_at(x, z) + 1.6, z);
            let Some(at) = project(view_proj, rect, p) else {
                continue;
            };
            let galley = painter.layout_no_wrap(station.name.clone(), font.clone(), egui::Color32::BLACK);
            let r = egui::Rect::from_center_size(at, galley.size() + egui::vec2(8.0, 4.0));
            painter.rect_filled(r, 0.0, egui::Color32::from_rgb(255, 255, 225));
            painter.rect_stroke(
                r,
                0.0,
                egui::Stroke::new(1.0, egui::Color32::BLACK),
                egui::StrokeKind::Inside,
            );
            painter.galley(r.min + egui::vec2(4.0, 2.0), galley, egui::Color32::BLACK);
        }
        self.terminal_labels(&painter, view_proj, rect);
    }

    /// The selected train's window: status, orders, stop/go and remove.
    fn train_window(&mut self, ctx: &egui::Context) {
        let Some(id) = self.selected_train else { return };
        let Some(train) = self.trains.get(id) else {
            self.selected_train = None;
            return;
        };
        enum Action {
            Up(usize),
            Down(usize),
            Delete(usize),
            Add(StationId),
            ToggleStop,
            Remove,
            Find,
        }
        let status = train.status(&self.stations);
        let orders = train.orders.clone();
        let current = if orders.is_empty() {
            0
        } else {
            train.order % orders.len()
        };
        let stopped = train.stopped;
        let (passengers, capacity) = (train.passengers, train.capacity());
        let name = |sid: StationId| self.stations.get(sid).map_or("?".to_string(), |s| s.name.clone());
        let mut action = None;
        let mut open = true;
        ui::win95::window(
            ctx,
            &format!("Train {}", id + 1),
            &mut open,
            egui::pos2(420.0, 110.0),
            |ui| {
                ui.set_width(240.0);
                ui.label(status);
                ui.label(format!("Carrying {} / {} passengers", passengers, capacity));
                ui.separator();
                ui.label("Orders:");
                if orders.is_empty() {
                    ui.weak("None yet. Add stations below, and the train will visit them in turn.");
                }
                for (i, &sid) in orders.iter().enumerate() {
                    ui.horizontal(|ui| {
                        let marker = if i == current { "▶" } else { "   " };
                        ui.label(format!("{marker} {}. {}", i + 1, name(sid)));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("x").on_hover_text("Delete").clicked() {
                                action = Some(Action::Delete(i));
                            }
                            if ui.small_button("⏷").on_hover_text("Move down").clicked() {
                                action = Some(Action::Down(i));
                            }
                            if ui.small_button("⏶").on_hover_text("Move up").clicked() {
                                action = Some(Action::Up(i));
                            }
                        });
                    });
                }
                egui::ComboBox::from_id_salt(("add stop", id))
                    .selected_text("Add stop…")
                    .show_ui(ui, |ui| {
                        if self.stations.list.is_empty() {
                            ui.weak("No stations yet");
                        }
                        for station in self.stations.list.values() {
                            if ui.selectable_label(false, &station.name).clicked() {
                                action = Some(Action::Add(station.id));
                            }
                        }
                    });
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button(if stopped { "Go" } else { "Stop" }).clicked() {
                        action = Some(Action::ToggleStop);
                    }
                    if ui
                        .button("Find")
                        .on_hover_text("Centre the view on this train")
                        .clicked()
                    {
                        action = Some(Action::Find);
                    }
                    if ui.button("Remove train").clicked() {
                        action = Some(Action::Remove);
                    }
                });
            },
        );
        if !open {
            self.selected_train = None;
        }
        let mut new_orders = orders.clone();
        let mut new_current = current;
        match action {
            Some(Action::Up(i)) if i > 0 => {
                new_orders.swap(i, i - 1);
                new_current = if current == i {
                    i - 1
                } else if current == i - 1 {
                    i
                } else {
                    current
                };
            }
            Some(Action::Down(i)) if i + 1 < orders.len() => {
                new_orders.swap(i, i + 1);
                new_current = if current == i {
                    i + 1
                } else if current == i + 1 {
                    i
                } else {
                    current
                };
            }
            Some(Action::Delete(i)) => {
                new_orders.remove(i);
                if i < current {
                    new_current -= 1;
                }
            }
            Some(Action::Add(sid)) => new_orders.push(sid),
            Some(Action::ToggleStop) => {
                let t = self.trains.train(id);
                t.stopped = !t.stopped;
                return;
            }
            Some(Action::Remove) => {
                self.trains.remove(id);
                self.selected_train = None;
                self.props_changed = true;
                return;
            }
            Some(Action::Find) => {
                let head = self.trains.train(id).point_behind(0.0);
                self.camera.focus.x = head.x;
                self.camera.focus.z = head.y;
                self.scene_dirty = true;
                return;
            }
            _ => return,
        }
        self.trains.set_orders(id, new_orders, new_current);
    }

    /// The selected station's window: rename it.
    fn station_window(&mut self, ctx: &egui::Context) {
        let Some(id) = self.selected_station else { return };
        let calling = self.trains.list.iter().filter(|t| t.orders.contains(&id)).count();
        let Some(station) = self.stations.list.get_mut(&id) else {
            self.selected_station = None;
            return;
        };
        let mut open = true;
        ui::win95::window(ctx, "Station", &mut open, egui::pos2(420.0, 110.0), |ui| {
            ui.set_width(220.0);
            ui.horizontal(|ui| {
                ui.label("Name:");
                ui.text_edit_singleline(&mut station.name);
            });
            ui.label(format!("{} platform tiles", station.tiles.len()));
            ui.label(format!(
                "{} passengers waiting, {} people nearby",
                station.waiting as u32,
                self.towns.population_near(&station.tiles)
            ));
            ui.label(match calling {
                0 => "No trains call here yet".to_string(),
                1 => "1 train calls here".to_string(),
                n => format!("{n} trains call here"),
            });
        });
        if !open {
            self.selected_station = None;
        }
    }

    /// The selected town's window: name, size and how well it's served.
    fn town_window(&mut self, ctx: &egui::Context) {
        let Some(id) = self.selected_town else { return };
        let population = self.towns.population(id);
        let buildings = self.towns.buildings.values().filter(|b| b.town == id).count();
        let Some(town) = self.towns.list.get_mut(id as usize) else {
            self.selected_town = None;
            return;
        };
        let mut open = true;
        ui::win95::window(ctx, "Town", &mut open, egui::pos2(420.0, 110.0), |ui| {
            ui.set_width(230.0);
            ui.horizontal(|ui| {
                ui.label("Name:");
                ui.text_edit_singleline(&mut town.name);
            });
            ui.label(format!("Population {population} in {buildings} buildings"));
            ui.label(format!("{} passengers delivered by rail", town.delivered));
            ui.label(if town.served > 50.0 {
                "Booming thanks to its rail service"
            } else if town.served > 5.0 {
                "Growing faster with its rail service"
            } else {
                "Growing slowly. A station and trains would help."
            });
        });
        if !open {
            self.selected_town = None;
        }
    }

    /// Moves the day/night light along. The scene is only redrawn when the
    /// light has visibly changed, and no more than 10 times a second.
    fn update_light(&mut self, now: f64, dt: f32) {
        let target = daylight::strength_at(self.ui.speed, self.ui.day_night);
        let ease = 1.5 * dt;
        self.light_strength += (target - self.light_strength).clamp(-ease, ease);
        if now - self.last_light_render < LIGHT_FRAME_SECONDS {
            return;
        }
        let light = Light::at(self.clock.light_phase(), self.light_strength);
        if light.differs(&self.light) {
            self.light = light;
            self.last_light_render = now;
            self.scene_dirty = true;
        }
    }

    fn show_message(&mut self, text: &str, now: f64) {
        self.message = Some((text.to_string(), now + 3.0));
    }

    /// Marks tiles whose infrastructure texels need re-uploading.
    fn mark_infra(&mut self, tiles: &[(i32, i32)]) {
        for &(x, z) in tiles {
            let r = self.infra_dirty.get_or_insert([x, z, x, z]);
            *r = [r[0].min(x), r[1].min(z), r[2].max(x), r[3].max(z)];
        }
    }

    /// Uploads the dirty rectangle of the infrastructure texture: roads, rails
    /// and the drag preview. While previewing, pieces the plan would replace
    /// are hidden; if the plan is blocked, its pieces show in the red channel.
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
        let blocked = self.plan.blocked.is_some();
        let mut preview = std::collections::HashMap::new();
        for p in &self.plan.pieces {
            let e = preview.entry(p.tile).or_insert((0u8, 0u8));
            e.0 |= p.add;
            if !blocked {
                e.1 |= p.remove;
            }
        }
        let mut rgba = Vec::with_capacity(((x1 - x0 + 1) * (z1 - z0 + 1) * 4) as usize);
        for z in z0..=z1 {
            for x in x0..=x1 {
                let (add, remove) = preview.get(&(x, z)).copied().unwrap_or_default();
                let (mut road, mut rail) = (self.roads.get(x, z), self.rails.get(x, z));
                match self.plan_network {
                    Network::Road => road &= !remove,
                    Network::Rail => rail &= !remove,
                }
                let (ok, red) = if blocked { (0, add) } else { (add, 0) };
                rgba.extend_from_slice(&[road, rail, ok, red]);
            }
        }
        let (w, h) = ((x1 - x0 + 1) as u32, (z1 - z0 + 1) as u32);
        self.renderer.write_infra(&rs.queue, x0 as u32, z0 as u32, w, h, &rgba);
        self.scene_dirty = true;
    }

    fn handle_keys(&mut self, ctx: &egui::Context, dt: f32) {
        // Track physical keys even while a text field has focus, so releases aren't missed.
        ctx.input(|i| self.physical_keys.update(i));
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
            if i.key_pressed(Key::O) {
                self.runway_along_x = !self.runway_along_x;
            }
            if i.key_pressed(Key::N) {
                self.ui.day_night = !self.ui.day_night;
            }
            if i.key_pressed(Key::Space) {
                if self.ui.speed == Speed::Paused {
                    self.ui.speed = self.resume_speed;
                } else {
                    self.resume_speed = self.ui.speed;
                    self.ui.speed = Speed::Paused;
                }
            }
            if i.key_pressed(Key::Home) {
                self.ui.go_home = true;
            }
            if i.key_pressed(Key::Escape) {
                self.ui.tool = Tool::Pointer;
            }

            // Movement keys go by physical position, so WASD/QE stay put on any layout.
            // (Letter shortcuts above use the logical key, matching the letter on the keycap.)
            let keys = &self.physical_keys;
            let axis = |neg: &[Key], pos: &[Key]| {
                let down = |ks: &[Key]| ks.iter().any(|&k| keys.down(k) || (is_arrow(k) && i.key_down(k)));
                down(pos) as i32 as f32 - down(neg) as i32 as f32
            };
            let right = axis(&[Key::A, Key::ArrowLeft], &[Key::D, Key::ArrowRight]);
            let back = axis(&[Key::W, Key::ArrowUp], &[Key::S, Key::ArrowDown]);
            let rotate = axis(&[Key::Q], &[Key::E]);
            if right != 0.0 || back != 0.0 {
                let speed = self.camera.view_height * 0.8 * dt;
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
            let units_per_point = self.camera.view_height / response.rect.height();
            let foreshorten = (-self.camera.pitch).sin().max(0.2);
            self.camera
                .pan(-delta.x * units_per_point, -delta.y * units_per_point / foreshorten);
            self.scene_dirty = true;
        }
        if response.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0 {
                self.zoom_at_cursor(ui, response.rect, (-scroll * 0.0015).exp());
                self.scene_dirty = true;
            }
        }
    }

    /// Zooms while keeping the ground point under the cursor fixed on screen.
    fn zoom_at_cursor(&mut self, ui: &egui::Ui, rect: egui::Rect, factor: f32) {
        let max_width = self.world.size_x as f32 * 2.0;
        let Some(pos) = ui.ctx().pointer_latest_pos() else {
            self.camera.zoom(factor, max_width);
            return;
        };
        let ndc = Vec2::new(
            (pos.x - rect.min.x) / rect.width() * 2.0 - 1.0,
            1.0 - (pos.y - rect.min.y) / rect.height() * 2.0,
        );
        let size = [rect.width().max(1.0) as u32, rect.height().max(1.0) as u32];
        let plane = self.camera.focus.y;
        let ground = |view_proj: Mat4| {
            let ray = picking::screen_ray(view_proj, ndc);
            (ray.dir.y < -1e-4).then(|| ray.origin + ray.dir * ((plane - ray.origin.y) / ray.dir.y))
        };
        let before = ground(self.view_proj(size));
        self.camera.zoom(factor, max_width);
        if let (Some(before), Some(after)) = (before, ground(self.view_proj(size))) {
            self.camera.focus.x += before.x - after.x;
            self.camera.focus.z += before.z - after.z;
        }
    }

    /// Keeps the focus inside the map and resting on the ground.
    fn settle_focus(&mut self, dt: f32) {
        // The view can wander out over the sea, up to a map's width past the edge.
        let size = self.world.size_x as f32;
        let f = &mut self.camera.focus;
        f.x = f.x.clamp(-size, 2.0 * size);
        f.z = f.z.clamp(-size, 2.0 * size);
        let (x, z) = (f.x.floor() as i32, f.z.floor() as i32);
        let sea = self.world.sea_level as f32 * crate::terrain::BLOCK_HEIGHT;
        let ground = if self.world.in_bounds(x, z) {
            self.world.surface_height(x, z).max(sea)
        } else {
            sea
        };
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
            ui.label(format!(
                "{} ({}) · chrome {:.2} · {} models",
                self.clock.date().year,
                crate::era::Era::of(self.year).name(),
                self.renderer.era_chrome,
                self.models.len()
            ));
            // Seek through time: 2000 on the left, 2050 on the right.
            let mut when = self.clock.year_f64().clamp(2000.0, 2050.0);
            let changed = ui
                .horizontal(|ui| {
                    ui.label("Date");
                    ui.spacing_mut().slider_width = 280.0;
                    let slider = egui::Slider::new(&mut when, 2000.0..=2050.0)
                        .custom_formatter(|v, _| {
                            let mut c = Clock::default();
                            c.set_year_f64(v);
                            c.date().date_string()
                        })
                        .custom_parser(|s| s.trim().parse::<f64>().ok());
                    let mut changed = ui.add_sized([300.0, 18.0], slider).changed();
                    for y in [2000.0, 2012.0, 2025.0, 2038.0, 2050.0] {
                        if ui.small_button(format!("{y:.0}")).clicked() {
                            when = y;
                            changed = true;
                        }
                    }
                    changed
                })
                .inner;
            if changed {
                self.seek_to(when);
            }
            ui.label(format!("scene renders: {}", self.scene_renders));
            ui.label(format!(
                "boats {} · planes {} · sea grid {:.1} ms · routes cached {} · searches pending {} (last step {} expansions)",
                self.boats.list.len(),
                self.planes.list.len(),
                self.sea_ms,
                self.boats.cached_routes(),
                self.boats.searches_pending(),
                self.boats.last_expansions()
            ));
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
                    c.yaw, c.pitch, c.view_height
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
                    ui.add(egui::Slider::new(&mut p.sea_border, 0.0..=0.3).text("sea border"));
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
        let clock_text = self.clock.date().date_string();
        let cash_text = company_ui::money(self.company.cash);
        let step = self.company.next_step(&self.assets, &self.facts);
        let company = &self.company;
        let unlocked = |t: Tool| company.tool_unlocked(t);
        let info = ui::ToolbarInfo {
            clock: &clock_text,
            daylight: self.light.day,
            cash: &cash_text,
            in_the_red: company.cash < 0,
            unlocked: &unlocked,
            suggested: step.tool.filter(|_| self.ui.show_next_step),
        };
        egui::Panel::top("toolbar")
            .frame(panel.inner_margin(egui::Margin::symmetric(4, 3)))
            .show(ui, |ui| ui::toolbar(ui, &mut self.ui, &info));
        let plan = &self.plan;
        let status = if !plan.is_empty() {
            let what = match self.plan_network {
                Network::Road => "road",
                Network::Rail => "track",
            };
            match plan.blocked {
                None => {
                    let unit = match (self.plan_network, self.ui.tool) {
                        (_, Tool::Station) => company::STATION_TILE,
                        (Network::Road, _) => company::ROAD_PIECE,
                        (Network::Rail, _) => company::RAIL_PIECE,
                    };
                    let cost = plan.pieces.len() as u32 * unit + plan.terraform_cost();
                    let short = if self.company.can_afford(cost) {
                        ""
                    } else {
                        " (not enough money)"
                    };
                    if plan.terraform_steps > 0 {
                        format!(
                            "Build {what}: {} tiles, {} ({} levelling the ground){short}",
                            plan.pieces.len(),
                            company_ui::money(cost as i64),
                            company_ui::money(plan.terraform_cost() as i64)
                        )
                    } else {
                        format!(
                            "Build {what}: {} tiles, {}{short}",
                            plan.pieces.len(),
                            company_ui::money(cost as i64)
                        )
                    }
                }
                Some(Blocked::Water) => format!("Can't build {what} on water"),
                Some(Blocked::Slope) => format!("Can't build {what} here: the ground is too steep"),
                Some(Blocked::Occupied) => "Roads and track can't share a tile yet".to_string(),
                Some(Blocked::Signal) => "There's a signal in the way: remove it first".to_string(),
            }
        } else if let Some((text, _)) = self.message.as_ref().filter(|(_, until)| *until > ui.input(|i| i.time)) {
            text.clone()
        } else if self.ui.speed == Speed::Paused {
            "Paused (Space to resume)".to_string()
        } else if let Some(hint) = self
            .site_hint
            .as_ref()
            .filter(|_| matches!(self.ui.tool, Tool::Port | Tool::Airport | Tool::Boat | Tool::Plane))
        {
            hint.clone()
        } else if let (Tool::Train, Some(_)) = (self.ui.tool, self.hover_tile) {
            "Click plain track to place a train".to_string()
        } else if let (Tool::Signal, Some(t)) = (self.ui.tool, self.hover_tile) {
            match self.signals.click_effect(&self.rails, t) {
                rails::SignalClick::Place => "Click to place a signal facing you".to_string(),
                rails::SignalClick::Flip => "Click to turn it round, right-click to remove".to_string(),
                rails::SignalClick::NoTrack => "No track here".to_string(),
                rails::SignalClick::Junction => "Signals go on plain track, not junctions".to_string(),
            }
        } else {
            match self.hover_tile {
                Some((x, z)) => {
                    let h = self.world.surface(x, z).map_or(0, |(base, _)| base);
                    format!("Tile ({x}, {z})  height {h}")
                }
                None => format!("Zoom {:.0}", self.camera.view_height),
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

        if std::mem::take(&mut self.ui.go_home) {
            self.centre_on_island();
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
        let frame_dt = ((now - self.last_time) as f32).min(0.1);
        // Simulated time this frame. Capped so a slow frame at high speed runs
        // the game slower for a moment instead of falling further behind.
        let sim_dt = (frame_dt * self.ui.speed.factor()).min(MAX_SIM_PER_FRAME);
        self.clock.tick(sim_dt);
        let year = self.clock.date().year;
        if year != self.year {
            self.set_year(year);
        }
        let chrome = crate::era::chrome(self.clock.year_f64());
        if (chrome - self.renderer.era_chrome).abs() > 0.004 {
            self.renderer.era_chrome = chrome;
            self.scene_dirty = true;
        }
        let steps = (sim_dt / MAX_SIM_STEP).ceil() as u32;
        let (mut trains_moved, mut vehicles_moved, mut craft_moved) = (false, false, false);
        let mut changed = Vec::new();
        for _ in 0..steps {
            let step = sim_dt / steps as f32;
            trains_moved |= self
                .trains
                .update(step, &self.rails, &self.signals, &self.stations, self.track_version);
            self.tick_towns(step);
            self.tick_company(step);
            vehicles_moved |= self.vehicles.update(step, &self.roads, &self.towns);
            craft_moved |= self.step_craft(step);
            changed.extend(self.sim.advance(&mut self.world, step));
        }
        self.last_time = now;
        self.trains_moving = trains_moved;
        self.vehicles_moving = vehicles_moved;
        self.craft_moving = craft_moved;
        if self.trains_moving || self.vehicles_moving || self.craft_moving {
            self.props_changed = true;
        }
        self.handle_arrivals();
        self.handle_craft_arrivals();
        self.remesh_tiles(&rs, &changed);
        self.update_light(now, frame_dt);

        self.game_ui(ui, &rs);
        if self.ui.show_debug {
            self.debug_window(&ctx, &rs);
        }
        ui::about_window(&ctx, &mut self.ui.show_about);
        self.train_window(&ctx);
        self.finance_window(&ctx);
        self.hq_window(&ctx);
        if self.ui.show_next_step {
            self.next_step_window(&ctx);
        }
        if self.bankrupt_window(&ctx) {
            self.params.seed = (now * 1e6) as u32 ^ self.params.seed.wrapping_mul(2654435761);
            self.regenerate(&rs);
        }
        self.town_window(&ctx);
        self.station_window(&ctx);
        self.craft_windows(&ctx);
        self.terminal_window(&ctx);

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
                self.renderer.render(
                    &rs,
                    size,
                    &self.settings,
                    view_proj,
                    camera_pos,
                    self.highlight,
                    clouds,
                    &self.light,
                );
                self.scene_renders += 1;
                self.scene_dirty = false;
            }
            if let Some(id) = self.renderer.texture_id() {
                let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
                ui.painter().image(id, rect, uv, egui::Color32::WHITE);
            }
            let view_proj = self.view_proj(size);
            self.last_view = Some((view_proj, rect));
            self.station_labels(ui, view_proj, rect);
        });

        // Keep frames coming only while something is moving (including egui's
        // smoothed scrolling, which spreads one wheel tick over several frames).
        let keys_held = self.physical_keys.any_down() || ctx.input(|i| !i.keys_down.is_empty());
        if keys_held
            || scene_changed
            || self.scene_dirty
            || self.trains_moving
            || self.vehicles_moving
            || self.craft_moving
        {
            ctx.request_repaint();
        } else if clouds_visible {
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(CLOUD_FRAME_SECONDS));
        } else if self.sim.is_active() {
            ctx.request_repaint_after(std::time::Duration::from_secs_f32(sim::TICK_SECONDS));
        }
        if self.ui.speed != Speed::Paused {
            // The clock keeps ticking, and the light with it (it only redraws
            // the scene when it has visibly changed).
            let wake = if self.ui.day_night {
                LIGHT_FRAME_SECONDS as f32
            } else {
                self.clock.seconds_to_next_day(self.ui.speed).max(0.02)
            };
            ctx.request_repaint_after(std::time::Duration::from_secs_f32(wake));
        }
        #[cfg(not(target_arch = "wasm32"))]
        self.devtools.update(&ctx);
        let ms = frame_start.elapsed().as_secs_f32() * 1000.0;
        self.frame_ms = self.frame_ms * 0.9 + ms * 0.1;
    }
}

fn is_arrow(key: egui::Key) -> bool {
    use egui::Key::*;
    matches!(key, ArrowLeft | ArrowRight | ArrowUp | ArrowDown)
}

/// Where a world point lands in the 3D view, if it's in front of the camera.
fn project(view_proj: Mat4, rect: egui::Rect, p: Vec3) -> Option<egui::Pos2> {
    let clip = view_proj * p.extend(1.0);
    if clip.w <= 0.0 {
        return None;
    }
    let ndc = clip.truncate() / clip.w;
    Some(egui::pos2(
        rect.min.x + (ndc.x + 1.0) / 2.0 * rect.width(),
        rect.min.y + (1.0 - ndc.y) / 2.0 * rect.height(),
    ))
}
