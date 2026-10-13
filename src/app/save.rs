//! Saving the game between runs: every few seconds the whole game is written out, compressed, and
//! read back when the game starts. Natively it goes to a file (written to a temporary file and
//! renamed over the old one, so a crash mid-save leaves the last good save); on the web it goes to
//! the browser's local storage, which replaces a value in one step.
//!
//! Only the game itself is saved, not what can be rebuilt from it (meshes, the sea grid, route
//! caches). The world's blocks are saved as runs (`terrain::runs`), and the whole save is deflated:
//! a new game is about 100 KB, well inside local storage's 5 MB.

use super::*;
use crate::charter::{Charter, Ending};
use crate::compute::{Allocation, Compute};
use crate::expressway::Expressway;
use crate::flood::Flood;
use crate::skycars::{Area, SkyCars};

/// Bumped when the saved state changes shape, so older saves are set aside rather than misread.
const VERSION: u32 = 1;

/// One chunk of the world as encoded for saving, and the chunk version it was encoded at.
pub(super) type SavedChunk = Option<((u64, u32), Vec<u8>)>;

/// Seconds between saves.
pub(super) const SAVE_SECONDS: f64 = 10.0;

/// Declares what is saved: one list of `StarcityApp` fields, from which come a borrowed form
/// for writing and an owned one for reading.
macro_rules! saved {
    ($($field:ident: $ty:ty,)*) => {
        #[derive(serde::Serialize)]
        struct Out<'a> {
            $($field: &'a $ty,)*
        }

        #[derive(serde::Deserialize)]
        struct In {
            $($field: $ty,)*
        }

        impl StarcityApp {
            fn save_out(&self) -> Out<'_> {
                Out { $($field: &self.$field,)* }
            }

            fn take_save(&mut self, s: In) {
                $(self.$field = s.$field;)*
            }
        }
    };
}

saved! {
    params: GenParams,
    camera: Camera,
    sim: Sim,
    roads: Layer,
    rails: Layer,
    signals: rails::Signals,
    trains: Trains,
    stations: Stations,
    towns: Towns,
    vehicles: Vehicles,
    year: i32,
    terminals: Terminals,
    boats: Boats,
    buses: crate::buses::Buses,
    express: Expressway,
    flood: Flood,
    walls: Layer,
    flood_warning: Option<String>,
    planes: Planes,
    runway_along_x: bool,
    town_clock: f32,
    track_version: u64,
    clock: Clock,
    company: Company,
    hq: Option<Hq>,
    contract_ends: Vec<(company_ui::Endpoint, company_ui::Endpoint)>,
    catchment_age: u32,
    buy: crate::catalog::TrainOrder,
    industries: Industries,
    industry_clock: f32,
    founded_year: i32,
    compute: Compute,
    compute_alloc: Allocation,
    news: Vec<usize>,
    space_launches: u32,
    agent_budget: f32,
    buy_boat: usize,
    buy_plane: usize,
    buy_bus: usize,
    buy_truck: usize,
    truck_cargo: Cargo,
    charter: Charter,
    wellbeing: f32,
    ending_shown: bool,
    converged: bool,
    ending: Option<Ending>,
    rockets: Vec<(f32, f32, f32)>,
    skycars: SkyCars,
    sky_areas: Vec<Area>,
    launch_rng: u64,
    last_event_date: f32,
    player_roads: u32,
}

/// Why a save couldn't be read.
#[derive(Debug)]
pub(super) enum LoadError {
    /// It's not a save, or it's damaged.
    Damaged,
    /// It's from a version of the game that saved things differently.
    OldVersion,
}

/// A save: the version, then the world's header, each chunk of its blocks, and everything else,
/// each serialized and compressed on its own. The blocks are most of the work and few chunks
/// change between saves, so each chunk's part is kept and reused until it changes
/// (`StarcityApp::world_bytes`).
#[derive(serde::Serialize, serde::Deserialize)]
struct Envelope<'a> {
    version: u32,
    #[serde(borrow)]
    header: &'a [u8],
    #[serde(borrow)]
    chunks: Vec<&'a [u8]>,
    #[serde(borrow)]
    rest: &'a [u8],
}

fn pack<T: serde::Serialize>(value: &T) -> Vec<u8> {
    let raw = postcard::to_allocvec(value).expect("the game state serializes");
    miniz_oxide::deflate::compress_to_vec(&raw, 6)
}

fn unpack<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T, LoadError> {
    let raw = miniz_oxide::inflate::decompress_to_vec_with_limit(bytes, 1 << 30).map_err(|_| LoadError::Damaged)?;
    match postcard::take_from_bytes::<T>(&raw) {
        Ok((value, [])) => Ok(value),
        _ => Err(LoadError::OldVersion),
    }
}

/// The game, ready to store.
pub(super) fn encode(app: &mut StarcityApp) -> Vec<u8> {
    let count = app.world.chunks_x() * app.world.chunks_z();
    app.world_bytes.resize_with(count, || None);
    for chunk in 0..count {
        let version = app.world.chunk_version(chunk);
        if app.world_bytes[chunk].as_ref().is_none_or(|(v, _)| *v != version) {
            app.world_bytes[chunk] = Some((version, pack(&app.world.chunk_runs(chunk))));
        }
    }
    let header = pack(&app.world.header());
    let rest = pack(&app.save_out());
    let envelope = Envelope {
        version: VERSION,
        header: &header,
        chunks: app
            .world_bytes
            .iter()
            .map(|c| c.as_ref().unwrap().1.as_slice())
            .collect(),
        rest: &rest,
    };
    postcard::to_allocvec(&envelope).expect("the save serializes")
}

/// Reads a save into the app (only if all of it reads).
pub(super) fn decode(app: &mut StarcityApp, bytes: &[u8]) -> Result<(), LoadError> {
    // The version comes first; check it before reading the rest in its layout.
    let (version, _) = postcard::take_from_bytes::<u32>(bytes).map_err(|_| LoadError::Damaged)?;
    if version != VERSION {
        return Err(LoadError::OldVersion);
    }
    let envelope: Envelope = postcard::from_bytes(bytes).map_err(|_| LoadError::Damaged)?;
    let header = unpack(envelope.header)?;
    let chunks = envelope
        .chunks
        .iter()
        .map(|c| unpack(c))
        .collect::<Result<Vec<_>, _>>()?;
    let world = World::from_saved(header, chunks).ok_or(LoadError::Damaged)?;
    let rest: In = unpack(envelope.rest)?;
    // The chunks as read are already encoded for the next save.
    app.world_bytes = (envelope.chunks.iter().enumerate())
        .map(|(i, c)| Some((world.chunk_version(i), c.to_vec())))
        .collect();
    app.world = world;
    app.take_save(rest);
    Ok(())
}

/// Where saves live.
pub(super) trait Store {
    fn read(&self) -> Option<Vec<u8>>;
    fn write(&self, bytes: &[u8]) -> Result<(), String>;
    /// Keeps an unreadable save aside (so a new game doesn't overwrite it).
    fn set_aside(&self);
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) struct FileStore {
    pub path: std::path::PathBuf,
}

#[cfg(not(target_arch = "wasm32"))]
impl FileStore {
    /// The save in the usual place for app data, or `STARCITY_SAVE` if set.
    pub fn default_place() -> Option<FileStore> {
        if let Some(p) = std::env::var_os("STARCITY_SAVE") {
            return Some(FileStore { path: p.into() });
        }
        let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
        let dir = if cfg!(target_os = "macos") {
            home?.join("Library/Application Support/Starcity")
        } else if cfg!(windows) {
            std::path::PathBuf::from(std::env::var_os("APPDATA")?).join("Starcity")
        } else {
            std::env::var_os("XDG_DATA_HOME")
                .map(std::path::PathBuf::from)
                .or_else(|| home.map(|h| h.join(".local/share")))?
                .join("starcity")
        };
        Some(FileStore {
            path: dir.join("save.bin"),
        })
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Store for FileStore {
    fn read(&self) -> Option<Vec<u8>> {
        std::fs::read(&self.path).ok()
    }

    fn write(&self, bytes: &[u8]) -> Result<(), String> {
        use std::io::Write;
        let fail = |e: std::io::Error| e.to_string();
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir).map_err(fail)?;
        }
        // Write it all and flush it to disk before it replaces the old save.
        let tmp = self.path.with_extension("tmp");
        let mut f = std::fs::File::create(&tmp).map_err(fail)?;
        f.write_all(bytes).map_err(fail)?;
        f.sync_all().map_err(fail)?;
        std::fs::rename(&tmp, &self.path).map_err(fail)
    }

    fn set_aside(&self) {
        let _ = std::fs::rename(&self.path, self.path.with_extension("old"));
    }
}

#[cfg(target_arch = "wasm32")]
pub(super) struct WebStore;

#[cfg(target_arch = "wasm32")]
const KEY: &str = "starcity-save";

#[cfg(target_arch = "wasm32")]
impl WebStore {
    fn storage() -> Option<web_sys::Storage> {
        web_sys::window()?.local_storage().ok()?
    }
}

#[cfg(target_arch = "wasm32")]
impl Store for WebStore {
    fn read(&self) -> Option<Vec<u8>> {
        use base64::Engine;
        let text = Self::storage()?.get_item(KEY).ok()??;
        base64::engine::general_purpose::STANDARD.decode(text).ok()
    }

    fn write(&self, bytes: &[u8]) -> Result<(), String> {
        use base64::Engine;
        let storage = Self::storage().ok_or("local storage isn't available")?;
        let text = base64::engine::general_purpose::STANDARD.encode(bytes);
        // One `setItem` replaces the old save whole, or (when full) leaves it as it was.
        storage
            .set_item(KEY, &text)
            .map_err(|_| "the browser's storage is full".to_string())
    }

    fn set_aside(&self) {
        if let Some(s) = Self::storage()
            && let Ok(Some(old)) = s.get_item(KEY)
        {
            let _ = s.set_item(&format!("{KEY}-old"), &old);
            let _ = s.remove_item(KEY);
        }
    }
}

/// The store for this platform, if saving is on.
pub(super) fn store() -> Option<Box<dyn Store>> {
    #[cfg(target_arch = "wasm32")]
    {
        Some(Box::new(WebStore))
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        // Dev runs (`STARCITY_*` settings, screenshots) don't touch the player's save unless
        // pointed at one with `STARCITY_SAVE`.
        let dev = std::env::vars_os().any(|(k, _)| {
            k.to_str()
                .is_some_and(|k| k.starts_with("STARCITY_") && k != "STARCITY_SAVE")
        });
        if dev && std::env::var_os("STARCITY_SAVE").is_none() {
            return None;
        }
        FileStore::default_place().map(|s| Box::new(s) as Box<dyn Store>)
    }
}

impl StarcityApp {
    /// Loads the saved game, if there is one. Rebuilds what isn't saved. Returns whether it loaded.
    pub(super) fn load_saved(&mut self, rs: &RenderState) -> bool {
        let Some(bytes) = self.store.as_ref().and_then(|s| s.read()) else {
            return false;
        };
        match decode(self, &bytes) {
            Ok(()) => {
                log::info!(
                    "loaded the saved game ({} KB), at {:?}",
                    bytes.len() / 1024,
                    self.clock.date()
                );
                self.after_load(rs);
                true
            }
            Err(e) => {
                log::warn!("save not loaded: {e:?}");
                if let Some(store) = &self.store {
                    store.set_aside();
                }
                let text = "Your saved game couldn't be opened (it may be from an older version of Starcity). A new game has started.";
                self.show_message(text, self.last_time + 8.0);
                false
            }
        }
    }

    /// After loading: the derived state (meshes, textures, caches) is rebuilt from the game.
    fn after_load(&mut self, rs: &RenderState) {
        let mut meshes = Vec::new();
        for cz in 0..self.world.chunks_z() {
            for cx in 0..self.world.chunks_x() {
                meshes.push(mesh::mesh_chunk(&self.world, cx, cz));
            }
        }
        self.renderer.upload_chunks(&rs.device, &meshes);
        self.sea = None;
        self.route_cache = None;
        self.plan_epoch += 1;
        self.track_version += 1;
        self.selected_train = None;
        self.selected_station = None;
        self.selected_town = None;
        self.selected_industry = None;
        self.selected_craft = None;
        self.selected_terminal = None;
        self.plan = Plan::default();
        self.drag_start = None;
        self.ui.year = self.year;
        self.renderer
            .resize_infra(&rs.device, self.world.size_x as u32, self.world.size_z as u32);
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
        self.set_year(self.year);
        self.refresh_assets();
        self.props_changed = true;
        self.static_changed = true;
        self.light_dirty = true;
        self.scene_dirty = true;
    }

    /// Saves now. A failure is shown once, not every time.
    pub(super) fn save_now(&mut self) {
        if self.demo.is_some() || self.company.sandbox {
            return;
        }
        if self.store.is_none() {
            return;
        }
        let t = Instant::now();
        let bytes = encode(self);
        let Some(store) = &self.store else { return };
        match store.write(&bytes) {
            Ok(()) => {
                log::debug!(
                    "saved ({} KB, {:.0} ms) at {:?}",
                    bytes.len() / 1024,
                    t.elapsed().as_secs_f32() * 1000.0,
                    self.clock.date()
                );
                self.save_failed = false;
            }
            Err(e) => {
                log::warn!("save failed: {e}");
                if !self.save_failed {
                    self.save_failed = true;
                    let now = self.last_time;
                    self.show_message(&format!("Couldn't save the game: {e}"), now + 8.0);
                }
            }
        }
    }

    /// Saves every `SAVE_SECONDS` of real time while the game runs.
    pub(super) fn autosave(&mut self) {
        if self.last_time - self.last_save >= SAVE_SECONDS {
            self.last_save = self.last_time;
            self.save_now();
        }
    }
}

impl StarcityApp {
    /// Starts again: a new map from `seed`, back in 2000 with a fresh company. Saved at once, so
    /// the old game is gone.
    pub(super) fn new_game(&mut self, rs: &RenderState, seed: u32) {
        self.params.seed = seed;
        self.clock = Clock::default();
        self.year = 2000;
        self.ui.year = 2000;
        self.ui.speed = Speed::X1;
        self.resume_speed = Speed::X1;
        self.town_clock = 0.0;
        self.catchment_age = 0;
        self.founded_year = 0;
        self.space_launches = 0;
        self.agent_budget = 0.0;
        self.wellbeing = 0.0;
        self.rockets.clear();
        self.following = None;
        self.stop_shown.clear();
        self.regenerate(rs);
        self.look_at_hq();
        self.save_now();
    }
}

#[cfg(test)]
mod tests {
    use super::{pack, unpack};
    use crate::network::Layer;
    use crate::terrain::World;
    use crate::terrain::generation::{self, GenParams};
    use crate::towns::Towns;

    /// The bulk of a save (the map, its towns and industries) reads back exactly as it was, and is
    /// small enough for local storage many times over.
    #[test]
    fn a_new_map_saves_small_and_reads_back() {
        let p = GenParams::default();
        let mut world = generation::generate(&p);
        let mut roads = Layer::new(world.size_x, world.size_z);
        let rails = Layer::new(world.size_x, world.size_z);
        let mut towns = Towns::generate(&world, &mut roads, &rails, p.seed, 2000);
        let (industries, _) =
            crate::industry::Industries::generate(&mut world, &mut towns, &roads, &rails, p.seed, 2000);
        let count = world.chunks_x() * world.chunks_z();
        let chunks: Vec<Vec<u8>> = (0..count).map(|c| pack(&world.chunk_runs(c))).collect();
        let header = pack(&world.header());
        let rest = pack(&(&roads, &towns, &industries));
        let total = header.len() + chunks.iter().map(|c| c.len()).sum::<usize>() + rest.len();
        // Local storage holds text, so base64 makes it a third bigger.
        let stored = total * 4 / 3;
        eprintln!("stored {} KB (rest {} KB)", stored / 1024, rest.len() / 1024);
        assert!(stored < 1024 * 1024, "{stored} bytes");

        let runs = chunks.iter().map(|c| unpack(c).unwrap()).collect();
        let back = World::from_saved(unpack(&header).unwrap(), runs).unwrap();
        for z in 0..world.size_z {
            for x in 0..world.size_x {
                assert_eq!(back.column(x, z), world.column(x, z));
            }
        }
        let (_, t, i): (Layer, Towns, crate::industry::Industries) = unpack(&rest).unwrap();
        assert_eq!(t.list.len(), towns.list.len());
        assert_eq!(i.list.len(), industries.list.len());
    }

    /// Changing a block changes only its chunk's version.
    #[test]
    fn an_edit_dirties_only_its_chunk() {
        let mut world = World::new(64, 64, 4, 0);
        let before: Vec<_> = (0..4).map(|c| world.chunk_version(c)).collect();
        world.set(40, 1, 3, crate::terrain::Block::full(crate::terrain::Material::Rock));
        let after: Vec<_> = (0..4).map(|c| world.chunk_version(c)).collect();
        assert_eq!(before[0], after[0]);
        assert_ne!(before[1], after[1]);
        assert_eq!(&before[2..], &after[2..]);
    }
}
