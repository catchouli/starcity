//! The model catalog: every `.glb` in `assets/models/`, loaded at startup.
//!
//! Models are plain glTF binaries that artists author in Blender (see
//! `assets/models/README.md`). Each carries a few flat custom properties
//! ("extras" on any node) that say what it is and when it's in use:
//!
//! - `role`: building, road_vehicle, train_car, boat, plane, terminal, prop
//!   (defaults from the folder it's in)
//! - `kind`: what within the role (house, bus, loco, ...; defaults to the file name)
//! - `from`, `to`: the years it's built or bought in
//! - `min_floors`, `max_floors`, `tiles_x`, `tiles_z`: buildings only
//! - `weight`: how often it's picked among equals
//!
//! Everything about how things look is decided by these files: which model a
//! building or vehicle gets depends on the year, so adding a new generation of
//! bus is adding a file.

use glam::{Mat4, Vec3};
use std::collections::HashMap;

include!(concat!(env!("OUT_DIR"), "/model_files.rs"));

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ModelId(pub u16);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Role {
    Building,
    RoadVehicle,
    TrainCar,
    Boat,
    Plane,
    Terminal,
    Prop,
    /// A company headquarters; `kind` is `hq_<level>`.
    Hq,
    /// A mine, mill, farm, power station...; `kind` is the industry's key.
    Industry,
}

impl Role {
    fn parse(s: &str) -> Option<Role> {
        Some(match s.to_ascii_lowercase().as_str() {
            "building" | "buildings" => Role::Building,
            "road_vehicle" | "vehicle" | "vehicles" => Role::RoadVehicle,
            "train_car" | "train" | "trains" => Role::TrainCar,
            "boat" | "boats" => Role::Boat,
            "plane" | "planes" => Role::Plane,
            "terminal" | "terminals" => Role::Terminal,
            "prop" | "props" => Role::Prop,
            "hq" | "headquarters" => Role::Hq,
            "industry" | "industries" => Role::Industry,
            _ => return None,
        })
    }
}

#[derive(Clone, Debug)]
pub struct ModelMeta {
    /// Path under assets/models without the extension, e.g. `buildings/house_2000_a`.
    #[allow(dead_code)] // Handy in logs and tools.
    pub name: String,
    pub role: Role,
    pub kind: String,
    pub from: i32,
    pub to: i32,
    pub min_floors: u8,
    pub max_floors: u8,
    /// Footprint in tiles.
    pub tiles: (u8, u8),
    pub weight: u32,
}

/// One vertex as the GPU sees it.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ModelVertex {
    pub pos: [f32; 3],
    pub normal: [f32; 3],
    /// Linear base colour (or the glow colour, for emissive materials).
    pub color: [f32; 4],
    /// Metallic, roughness, emissive (x2), and 255 for "Paint" materials
    /// (tinted by each vehicle's colour).
    pub material: [u8; 4],
}

#[derive(Clone, Debug, Default)]
pub struct MeshData {
    pub vertices: Vec<ModelVertex>,
    pub indices: Vec<u32>,
}

impl MeshData {
    /// Bounding box (min, max).
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn bounds(&self) -> (Vec3, Vec3) {
        let mut lo = Vec3::splat(f32::MAX);
        let mut hi = Vec3::splat(f32::MIN);
        for v in &self.vertices {
            lo = lo.min(Vec3::from(v.pos));
            hi = hi.max(Vec3::from(v.pos));
        }
        (lo, hi)
    }
}

#[derive(Default)]
pub struct Models {
    pub meta: Vec<ModelMeta>,
    pub meshes: Vec<MeshData>,
    by_name: HashMap<String, ModelId>,
    by_role: HashMap<Role, Vec<ModelId>>,
}

fn hash(seed: u32) -> u32 {
    let mut x = seed.wrapping_mul(0x9E37_79B1) ^ 0x85EB_CA6B;
    x ^= x >> 15;
    x = x.wrapping_mul(0x2C1B_3C6D);
    x ^ (x >> 13)
}

impl Models {
    /// The models built into the binary.
    pub fn load_embedded() -> Models {
        let (models, problems) = Models::from_files(MODEL_FILES);
        for p in problems {
            log::warn!("model: {p}");
        }
        models
    }

    /// Loads a set of `.glb` files. Files that fail to load are skipped, and
    /// each problem is reported in the second result.
    pub fn from_files(files: &[(&str, &[u8])]) -> (Models, Vec<String>) {
        let mut models = Models::default();
        let mut problems = Vec::new();
        for &(name, bytes) in files {
            match load_glb(name, bytes) {
                Ok((meta, mesh)) => {
                    let id = ModelId(models.meta.len() as u16);
                    models.by_name.insert(name.to_string(), id);
                    models.by_role.entry(meta.role).or_default().push(id);
                    models.meta.push(meta);
                    models.meshes.push(mesh);
                }
                Err(why) => problems.push(format!("{name}.glb: {why}")),
            }
        }
        (models, problems)
    }

    #[allow(dead_code)] // For tools and scenarios that want a specific model.
    pub fn id(&self, name: &str) -> Option<ModelId> {
        self.by_name.get(name).copied()
    }

    pub fn meta(&self, id: ModelId) -> &ModelMeta {
        &self.meta[id.0 as usize]
    }

    pub fn len(&self) -> usize {
        self.meta.len()
    }

    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.meta.is_empty()
    }

    /// Models of a role (and kind, if given) in use in `year`, whose floors
    /// range includes `floors` and whose footprint is `tiles` (when given).
    fn matching<'a>(
        &'a self,
        role: Role,
        kind: Option<&'a str>,
        year: i32,
        floors: Option<u8>,
        tiles: Option<(u8, u8)>,
    ) -> impl Iterator<Item = ModelId> + 'a {
        self.by_role
            .get(&role)
            .into_iter()
            .flatten()
            .copied()
            .filter(move |&id| {
                let m = self.meta(id);
                kind.is_none_or(|k| m.kind == k)
                    && m.from <= year
                    && year <= m.to
                    && floors.is_none_or(|f| (m.min_floors..=m.max_floors).contains(&f))
                    && tiles.is_none_or(|t| m.tiles == t)
            })
    }

    /// A weighted pick among the models `make()` yields, by seed. Runs the
    /// iterator twice instead of collecting it, so it doesn't allocate (vehicles
    /// ask for their model every frame).
    fn choose<I: Iterator<Item = ModelId>>(&self, make: impl Fn() -> I, seed: u32) -> Option<ModelId> {
        let total: u32 = make().map(|i| self.meta(i).weight.max(1)).sum();
        if total == 0 {
            return None;
        }
        let mut target = hash(seed) % total;
        for id in make() {
            let w = self.meta(id).weight.max(1);
            if target < w {
                return Some(id);
            }
            target -= w;
        }
        None
    }

    /// The model for something of this role and kind built or bought in
    /// `year`. `seed` picks among equals (so it's stable for a given thing).
    /// If nothing is in use that year, the nearest generation is used.
    pub fn pick(&self, role: Role, kind: &str, year: i32, seed: u32) -> Option<ModelId> {
        self.pick_with(role, Some(kind), year, None, None, seed)
    }

    pub fn pick_building(&self, year: i32, floors: u8, tiles: (u8, u8), seed: u32) -> Option<ModelId> {
        self.pick_with(Role::Building, None, year, Some(floors), Some(tiles), seed)
    }

    fn pick_with(
        &self,
        role: Role,
        kind: Option<&str>,
        year: i32,
        floors: Option<u8>,
        tiles: Option<(u8, u8)>,
        seed: u32,
    ) -> Option<ModelId> {
        self.choose(|| self.matching(role, kind, year, floors, tiles), seed)
            // Nothing for those floors that year: any floors.
            .or_else(|| self.choose(|| self.matching(role, kind, year, None, tiles), seed))
            // Nothing in use that year: the nearest generation.
            .or_else(|| {
                let ids: Vec<ModelId> = self
                    .by_role
                    .get(&role)?
                    .iter()
                    .copied()
                    .filter(|&id| {
                        let m = self.meta(id);
                        kind.is_none_or(|k| m.kind == k) && tiles.is_none_or(|t| m.tiles == t)
                    })
                    .collect();
                let gap = |id: ModelId| {
                    let m = self.meta(id);
                    (m.from - year).max(year - m.to).max(0)
                };
                let nearest = ids.iter().copied().map(gap).min()?;
                self.choose(|| ids.iter().copied().filter(|&id| gap(id) == nearest), seed)
            })
    }

    /// The biggest building footprint (in tiles on a side) there's a model for in `year`.
    pub fn largest_building_footprint(&self, year: i32) -> u8 {
        self.matching(Role::Building, None, year, None, None)
            .map(|id| self.meta(id).tiles.0.max(self.meta(id).tiles.1))
            .max()
            .unwrap_or(1)
    }

    /// Whether there's a building model of exactly this footprint in `year` for `floors`.
    pub fn has_building(&self, year: i32, floors: u8, tiles: (u8, u8)) -> bool {
        self.matching(Role::Building, None, year, Some(floors), Some(tiles))
            .next()
            .is_some()
    }
}

fn num(v: &serde_json::Value) -> Option<f64> {
    match v {
        serde_json::Value::Number(n) => n.as_f64(),
        serde_json::Value::String(s) => s.trim().parse().ok(),
        serde_json::Value::Bool(b) => Some(*b as u8 as f64),
        _ => None,
    }
}

fn text(v: &serde_json::Value) -> Option<String> {
    v.as_str().map(|s| s.to_string())
}

/// Custom properties from the scene and every node, flattened (first wins).
fn extras(doc: &gltf::Document) -> serde_json::Map<String, serde_json::Value> {
    let mut all = serde_json::Map::new();
    let mut add = |raw: &Option<Box<serde_json::value::RawValue>>| {
        if let Some(raw) = raw {
            if let Ok(serde_json::Value::Object(map)) = serde_json::from_str(raw.get()) {
                for (k, v) in map {
                    all.entry(k).or_insert(v);
                }
            }
        }
    };
    for scene in doc.scenes() {
        add(scene.extras());
    }
    for node in doc.nodes() {
        add(node.extras());
    }
    all
}

fn load_glb(name: &str, bytes: &[u8]) -> Result<(ModelMeta, MeshData), String> {
    let gltf = gltf::Gltf::from_slice(bytes).map_err(|e| format!("not a valid glTF: {e}"))?;
    let blob = gltf.blob.as_deref();
    let doc = &gltf.document;
    let scene = doc.default_scene().or_else(|| doc.scenes().next()).ok_or("no scene")?;

    let mut mesh = MeshData::default();
    let mut stack: Vec<(gltf::Node, Mat4)> = scene.nodes().map(|n| (n, Mat4::IDENTITY)).collect();
    while let Some((node, parent)) = stack.pop() {
        let world = parent * Mat4::from_cols_array_2d(&node.transform().matrix());
        let normal_matrix = world.inverse().transpose();
        for child in node.children() {
            stack.push((child, world));
        }
        let Some(m) = node.mesh() else { continue };
        for prim in m.primitives() {
            if prim.mode() != gltf::mesh::Mode::Triangles {
                continue;
            }
            let reader = prim.reader(|b| match b.source() {
                gltf::buffer::Source::Bin => blob,
                gltf::buffer::Source::Uri(_) => None,
            });
            let positions: Vec<[f32; 3]> = reader
                .read_positions()
                .ok_or("a mesh has no positions (are the buffers embedded? export as .glb)")?
                .collect();
            let indices: Vec<u32> = match reader.read_indices() {
                Some(i) => i.into_u32().collect(),
                None => (0..positions.len() as u32).collect(),
            };
            if indices.iter().any(|&i| i as usize >= positions.len()) {
                return Err("an index points past the vertices".to_string());
            }
            let normals: Vec<[f32; 3]> = match reader.read_normals() {
                Some(n) => n.collect(),
                None => flat_normals(&positions, &indices),
            };
            let colours: Option<Vec<[f32; 4]>> = reader.read_colors(0).map(|c| c.into_rgba_f32().collect());

            let mat = prim.material();
            let pbr = mat.pbr_metallic_roughness();
            let base = pbr.base_color_factor();
            let emissive = mat.emissive_factor();
            let strength = mat.emissive_strength().unwrap_or(1.0);
            let glow = emissive.iter().cloned().fold(0.0f32, f32::max) * strength;
            let (rgb, glow) = if glow > 0.0 {
                // Glow in the emissive colour.
                let peak = emissive.iter().cloned().fold(0.0f32, f32::max);
                ([emissive[0] / peak, emissive[1] / peak, emissive[2] / peak], glow)
            } else {
                ([base[0], base[1], base[2]], 0.0)
            };
            let paint = mat.name().is_some_and(|n| n.to_ascii_lowercase().starts_with("paint"));
            let packed = [
                (pbr.metallic_factor().clamp(0.0, 1.0) * 255.0).round() as u8,
                (pbr.roughness_factor().clamp(0.0, 1.0) * 255.0).round() as u8,
                (glow / 2.0).clamp(0.0, 1.0).mul_add(255.0, 0.0).round() as u8,
                if paint { 255 } else { 0 },
            ];

            let first = mesh.vertices.len() as u32;
            for (i, p) in positions.iter().enumerate() {
                let pos = world.transform_point3(Vec3::from(*p));
                let n = normal_matrix
                    .transform_vector3(Vec3::from(normals[i]))
                    .normalize_or_zero();
                let vc = colours.as_ref().map_or([1.0; 4], |c| c[i]);
                mesh.vertices.push(ModelVertex {
                    pos: pos.to_array(),
                    normal: n.to_array(),
                    color: [rgb[0] * vc[0], rgb[1] * vc[1], rgb[2] * vc[2], base[3] * vc[3]],
                    material: packed,
                });
            }
            // A mirrored node flips the winding; keep triangles facing out.
            let flip = world.determinant() < 0.0;
            for t in indices.chunks_exact(3) {
                if flip {
                    mesh.indices.extend([first + t[0], first + t[2], first + t[1]]);
                } else {
                    mesh.indices.extend([first + t[0], first + t[1], first + t[2]]);
                }
            }
        }
    }
    if mesh.indices.is_empty() {
        return Err("no triangles".to_string());
    }

    let props = extras(doc);
    let folder = name.split('/').next().unwrap_or("");
    let stem = name.rsplit('/').next().unwrap_or(name);
    let role = match props.get("role").and_then(text) {
        Some(r) => Role::parse(&r).ok_or_else(|| format!("unknown role {r:?}"))?,
        None => Role::parse(folder).ok_or("no `role` property and the folder doesn't name one")?,
    };
    let get = |k: &str, default: f64| props.get(k).and_then(num).unwrap_or(default);
    let meta = ModelMeta {
        name: name.to_string(),
        role,
        kind: props.get("kind").and_then(text).unwrap_or_else(|| stem.to_string()),
        from: get("from", 1900.0) as i32,
        to: get("to", 2200.0) as i32,
        min_floors: get("min_floors", 1.0).clamp(1.0, 255.0) as u8,
        max_floors: get("max_floors", 255.0).clamp(1.0, 255.0) as u8,
        tiles: (
            get("tiles_x", 1.0).clamp(1.0, 16.0) as u8,
            get("tiles_z", 1.0).clamp(1.0, 16.0) as u8,
        ),
        weight: get("weight", 1.0).max(0.0) as u32,
    };
    if meta.to < meta.from {
        return Err(format!("`to` ({}) is before `from` ({})", meta.to, meta.from));
    }
    Ok((meta, mesh))
}

fn flat_normals(positions: &[[f32; 3]], indices: &[u32]) -> Vec<[f32; 3]> {
    let mut n = vec![Vec3::ZERO; positions.len()];
    for t in indices.chunks_exact(3) {
        let [a, b, c] = [t[0], t[1], t[2]].map(|i| Vec3::from(positions[i as usize]));
        let f = (b - a).cross(c - a);
        for &i in t {
            n[i as usize] += f;
        }
    }
    n.into_iter().map(|v| v.normalize_or_zero().to_array()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn models() -> Models {
        let (models, problems) = Models::from_files(MODEL_FILES);
        assert!(problems.is_empty(), "models that failed to load: {problems:#?}");
        models
    }

    #[test]
    fn every_model_loads_and_is_a_sensible_size() {
        let models = models();
        assert!(models.len() >= 40, "{} models", models.len());
        for (meta, mesh) in models.meta.iter().zip(&models.meshes) {
            let (lo, hi) = mesh.bounds();
            let name = &meta.name;
            // (A pier's posts stand in the water, below its deck.)
            assert!(
                lo.y >= if meta.kind == "pier" { -0.15 } else { -0.05 },
                "{name} dips below the ground (y = {})",
                lo.y
            );
            let (w, d) = (meta.tiles.0 as f32, meta.tiles.1 as f32);
            let (limit, max_x, max_z, max_y) = match meta.role {
                Role::Building => ((w / 2.0 + 0.1, d / 2.0 + 0.1), w / 2.0 + 0.1, d / 2.0 + 0.1, 8.0),
                Role::RoadVehicle => ((0.5, 0.2), 0.5, 0.2, 0.6),
                Role::TrainCar => ((0.5, 0.2), 0.5, 0.2, 0.7),
                Role::Boat => ((0.6, 0.3), 0.6, 0.3, 0.8),
                Role::Plane => ((0.7, 0.55), 0.7, 0.55, 0.5),
                // A port's pier reaches out into the next tile.
                Role::Terminal if meta.kind == "port" => ((1.6, 0.6), 1.6, 0.6, 1.2),
                // An airport's origin is its north-west corner.
                Role::Terminal => ((11.0, 4.0), 11.0, 4.0, 1.5),
                Role::Prop => ((1.1, 1.1), 1.1, 1.1, 1.5),
                // Headquarters stand on a 4 x 4 plot.
                Role::Hq => ((2.1, 2.1), 2.1, 2.1, 8.0),
                // Industries are 3 x 3 or 4 x 4.
                Role::Industry => ((2.1, 2.1), 2.1, 2.1, 8.0),
            };
            let _ = limit;
            if meta.kind == "airport" {
                assert!(
                    lo.x >= -0.5 && hi.x <= max_x && lo.z >= -0.5 && hi.z <= max_z,
                    "{name} spills out of its 10 x 3 strip: {lo} .. {hi}"
                );
            } else {
                assert!(
                    lo.x.abs() <= max_x && hi.x.abs() <= max_x && lo.z.abs() <= max_z && hi.z.abs() <= max_z,
                    "{name} is too big for a {} model: {lo} .. {hi}",
                    meta.kind
                );
            }
            assert!(hi.y <= max_y, "{name} is {} tall", hi.y);
            for v in &mesh.vertices {
                assert!(
                    (v.normal.iter().map(|c| c * c).sum::<f32>() - 1.0).abs() < 0.05,
                    "{name} has a bad normal"
                );
            }
        }
    }

    /// Models are drawn by the hundreds, in the web build too, so there's a
    /// triangle budget per role. These limits are generous: they stop a
    /// 200,000-triangle sculpt from going in by accident, not careful art.
    #[test]
    fn models_stay_within_their_triangle_budgets() {
        let models = models();
        for (meta, mesh) in models.meta.iter().zip(&models.meshes) {
            let tris = mesh.indices.len() / 3;
            let budget = match meta.role {
                Role::Building | Role::Hq | Role::Industry => 4000,
                Role::RoadVehicle | Role::TrainCar | Role::Prop => 1500,
                Role::Boat | Role::Plane => 3000,
                Role::Terminal => 8000,
            };
            assert!(
                tris <= budget,
                "{} has {tris} triangles (budget for a {:?} is {budget})",
                meta.name,
                meta.role
            );
        }
    }

    #[test]
    fn every_year_has_something_to_build_and_ride() {
        let models = models();
        for year in (1990..=2100).step_by(1) {
            for floors in [1u8, 2, 4, 7, 12] {
                assert!(
                    models.pick_building(year, floors, (1, 1), year as u32).is_some(),
                    "no 1x1 building for {floors} floors in {year}"
                );
            }
            for (role, kinds) in [
                (Role::RoadVehicle, &["car", "bus", "truck"][..]),
                (Role::TrainCar, &["loco", "coach"][..]),
                (Role::Boat, &["ferry"][..]),
                (Role::Plane, &["airliner"][..]),
                (Role::Terminal, &["port", "airport"][..]),
                (Role::Hq, &["hq_0", "hq_1", "hq_2"][..]),
                (
                    Role::Industry,
                    &[
                        "coal_mine",
                        "iron_ore_mine",
                        "forest",
                        "farm",
                        "power_station",
                        "steel_mill",
                        "sawmill",
                        "food_plant",
                        "factory",
                    ][..],
                ),
                (
                    Role::Prop,
                    &["signal_red", "signal_green", "station_platform", "station_building"][..],
                ),
            ] {
                for kind in kinds {
                    assert!(models.pick(role, kind, year, 1).is_some(), "no {kind} for {year}");
                }
            }
        }
    }

    #[test]
    fn newer_years_get_newer_looks_and_chrome_arrives() {
        let models = models();
        // The share of a model's surface that's chrome-grade metal (metallic > 0.8).
        let metal = |id: ModelId| {
            let mesh = &models.meshes[id.0 as usize];
            let (mut chrome, mut total) = (0.0f32, 0.0f32);
            for t in mesh.indices.chunks_exact(3) {
                let [a, b, c] = [t[0], t[1], t[2]].map(|i| &mesh.vertices[i as usize]);
                let area = (Vec3::from(b.pos) - Vec3::from(a.pos))
                    .cross(Vec3::from(c.pos) - Vec3::from(a.pos))
                    .length();
                total += area;
                if a.material[0] > 204 {
                    chrome += area;
                }
            }
            chrome / total
        };
        let avg = |year: i32| {
            let picks: Vec<f32> = (0..40)
                .filter_map(|s| models.pick_building(year, 6, (1, 1), s))
                .map(metal)
                .collect();
            picks.iter().sum::<f32>() / picks.len() as f32
        };
        assert!(avg(2000) < 0.05, "2000 towers are plain: {}", avg(2000));
        assert!(avg(2012) < 0.05, "2012 towers are glass, not chrome: {}", avg(2012));
        assert!(avg(2030) > 0.25, "chrome has started by 2030: {}", avg(2030));
        assert!(avg(2045) > 0.25, "everything is chrome by 2045: {}", avg(2045));
        // Orbs only exist late.
        assert_eq!(models.largest_building_footprint(2000), 1);
        assert!(models.largest_building_footprint(2030) >= 2);
        assert_eq!(models.largest_building_footprint(2045), 3);
        assert!(models.has_building(2045, 9, (3, 3)));
        assert!(!models.has_building(2020, 9, (3, 3)));
    }

    /// Loading runs at every startup (in the browser, two to three times slower).
    #[test]
    fn loading_every_model_is_quick() {
        let t = std::time::Instant::now();
        let (models, problems) = Models::from_files(MODEL_FILES);
        let took = t.elapsed();
        let (v, i): (usize, usize) = models
            .meshes
            .iter()
            .fold((0, 0), |a, m| (a.0 + m.vertices.len(), a.1 + m.indices.len()));
        let bytes: usize = MODEL_FILES.iter().map(|f| f.1.len()).sum();
        eprintln!(
            "{} models, {v} vertices, {} triangles, {} KB of files, loaded in {took:?}",
            models.len(),
            i / 3,
            bytes / 1024
        );
        assert!(problems.is_empty());
        assert!(took.as_millis() < 60, "loading took {took:?}");
    }

    #[test]
    fn bad_files_are_reported_not_fatal() {
        let (models, problems) = Models::from_files(&[("buildings/junk", b"not a glb at all")]);
        assert_eq!(models.len(), 0);
        assert_eq!(problems.len(), 1, "{problems:?}");
    }

    #[test]
    fn picking_is_stable_and_weighted_by_seed() {
        let models = models();
        let a = models.pick(Role::RoadVehicle, "car", 2020, 7);
        assert_eq!(a, models.pick(Role::RoadVehicle, "car", 2020, 7));
        // Several house models are in use in 2000, and the seed spreads across them.
        let seen: std::collections::HashSet<_> = (0..50)
            .filter_map(|s| models.pick_building(2005, 1, (1, 1), s))
            .collect();
        assert!(seen.len() >= 2, "{seen:?}");
    }
}
