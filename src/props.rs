//! Turns the game's state into placed models: which model each building,
//! vehicle, station and so on is drawn with, and where.
//!
//! The choice of model is by year (see `models.rs`): buildings and vehicles
//! keep the look of the year they were built or bought in, while stations,
//! ports and airports are refurbished, so they follow the current year.

use crate::boats::Boats;
use crate::models::{ModelId, Models, Role};
use crate::network::Layer;
use crate::planes::Planes;
use crate::rails::{self, Signals};
use crate::render::ModelInstance;
use crate::stations::Stations;
use crate::terminals::{Site, Terminals};
use crate::terrain::{BLOCK_HEIGHT, World};
use crate::towns::Towns;
use crate::trains::Trains;
use crate::vehicles::{Kind, Vehicles};
use glam::Vec2;
use std::f32::consts::FRAC_PI_2;

pub type Placed = Vec<(ModelId, ModelInstance)>;

/// The rotation that turns a model facing +X to face along `dir` (x, z).
fn yaw_of(dir: Vec2) -> f32 {
    (-dir.y).atan2(dir.x)
}

fn tile_hash(tile: (i32, i32), salt: u32) -> u32 {
    (tile.0 as u32).wrapping_mul(73_856_093) ^ (tile.1 as u32).wrapping_mul(19_349_663) ^ salt.wrapping_mul(83_492_791)
}

/// Signals beside the track: lamp red while a train holds the block, green when free.
pub fn signals(models: &Models, world: &World, signals: &Signals, trains: &Trains) -> Placed {
    let (Some(red), Some(green)) = (
        models.pick(Role::Prop, "signal_red", 2000, 0),
        models.pick(Role::Prop, "signal_green", 2000, 0),
    ) else {
        return Vec::new();
    };
    let mut out = Vec::with_capacity(signals.by_tile.len());
    for (&(x, z), sig) in &signals.by_tile {
        let ((fx, fz), (tx, tz)) = rails::piece_ends(sig.piece);
        let along = Vec2::new(tx - fx, tz - fz).normalize() * if sig.forward { 1.0 } else { -1.0 };
        let right = Vec2::new(-along.y, along.x);
        let mid = Vec2::new(x as f32 + (fx + tx) / 2.0, z as f32 + (fz + tz) / 2.0);
        let p = mid + right * 0.4;
        let model = if trains.is_owned((x, z), sig.piece) { red } else { green };
        out.push((
            model,
            ModelInstance::at([p.x, world.height_at(p.x, p.y), p.y], yaw_of(along)),
        ));
    }
    out
}

/// Train cars: a locomotive, then coaches, in the look of the year the train was built.
pub fn trains(models: &Models, world: &World, trains: &Trains) -> Placed {
    let mut out = Vec::new();
    for train in &trains.list {
        let wreck = train.crashed.is_some();
        for (i, (front, back)) in train.car_ends().into_iter().enumerate() {
            let (kind, paint) = if i == 0 {
                ("loco", [35, 105, 60])
            } else {
                ("coach", [135, 40, 38])
            };
            let Some(model) = models.pick(Role::TrainCar, kind, train.born, train.id) else {
                continue;
            };
            let centre = (front + back) / 2.0;
            let along = (front - back).normalize_or_zero();
            let ground = (world.height_at(front.x, front.y) + world.height_at(back.x, back.y)) / 2.0;
            let mut inst = ModelInstance::at([centre.x, ground, centre.y], yaw_of(along)).tinted(paint);
            if wreck {
                inst = inst.wrecked();
            }
            out.push((model, inst));
        }
    }
    out
}

/// Station platforms along each platform tile, and a station building by the first.
pub fn stations(models: &Models, world: &World, rails_layer: &Layer, stations: &Stations, year: i32) -> Placed {
    let (Some(platform), Some(building)) = (
        models.pick(Role::Prop, "station_platform", year, 0),
        models.pick(Role::Prop, "station_building", year, 0),
    ) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for station in stations.list.values() {
        for (i, &(x, z)) in station.tiles.iter().enumerate() {
            let along = if rails_layer.get(x, z) == rails::Y {
                Vec2::new(0.0, 1.0)
            } else {
                Vec2::new(1.0, 0.0)
            };
            let side = Vec2::new(-along.y, along.x);
            let centre = Vec2::new(x as f32 + 0.5, z as f32 + 0.5);
            let ground = world.height_at(centre.x, centre.y);
            out.push((platform, ModelInstance::at([centre.x, ground, centre.y], yaw_of(along))));
            if i == 0 {
                let p = centre + side * 0.82;
                out.push((building, ModelInstance::at([p.x, ground, p.y], yaw_of(along))));
            }
        }
    }
    out
}

/// Town buildings, each in the look of the year it was built (or rebuilt).
pub fn buildings(models: &Models, world: &World, towns: &Towns) -> Placed {
    let mut out = Vec::with_capacity(towns.buildings.len());
    for (&tile, b) in &towns.buildings {
        if b.part_of.is_some() {
            continue;
        }
        let seed = tile_hash(tile, b.style as u32);
        let Some(model) = models.pick_building(b.year, b.floors, b.size, seed) else {
            continue;
        };
        let (w, d) = (b.size.0 as f32, b.size.1 as f32);
        let (cx, cz) = (tile.0 as f32 + w / 2.0, tile.1 as f32 + d / 2.0);
        // Small buildings face any of four ways, so streets don't look cloned.
        let yaw = if b.size == (1, 1) {
            (seed >> 5 & 3) as f32 * FRAC_PI_2
        } else {
            0.0
        };
        out.push((model, ModelInstance::at([cx, world.height_at(cx, cz), cz], yaw)));
    }
    out
}

/// The headquarters, in the model for its level (or the nearest lower one).
pub fn hq(models: &Models, world: &World, hq: &crate::hq::Hq, level: u8) -> Placed {
    let (cx, cz) = hq.centre();
    let model = (0..=level)
        .rev()
        .find_map(|l| models.pick(Role::Hq, &format!("hq_{l}"), 2000, 0));
    model
        .map(|m| (m, ModelInstance::at([cx, world.height_at(cx, cz), cz], 0.0).revealed()))
        .into_iter()
        .collect()
}

/// Mines, forests, farms, power stations, mills and the factory.
pub fn industries(models: &Models, world: &World, industries: &crate::industry::Industries) -> Placed {
    let mut out = Vec::with_capacity(industries.list.len());
    for i in industries.list.iter().filter(|i| !i.closed) {
        let Some(model) = models.pick(Role::Industry, i.kind.key(), 2000, 0) else {
            continue;
        };
        let (cx, cz) = i.centre();
        out.push((model, ModelInstance::at([cx, world.height_at(cx, cz), cz], 0.0)));
    }
    out
}

/// Cars, buses and trucks, in the look of the year they were bought, in their own colours.
pub fn road_vehicles(models: &Models, world: &World, vehicles: &Vehicles) -> Placed {
    let mut out = Vec::with_capacity(vehicles.list.len());
    for v in &vehicles.list {
        let kind = match v.kind {
            Kind::Car => "car",
            Kind::Bus => "bus",
            Kind::Truck => "truck",
        };
        let Some(model) = models.pick(Role::RoadVehicle, kind, v.born, 0) else {
            continue;
        };
        let (p, dir) = v.placement();
        out.push((
            model,
            ModelInstance::at([p.x, world.height_at(p.x, p.y), p.y], yaw_of(dir)).tinted(v.color),
        ));
    }
    out
}

/// Ports (a quay, warehouse and crane, with a pier at sea level) and airports.
pub fn terminals(models: &Models, world: &World, terminals: &Terminals, year: i32) -> Placed {
    let sea_y = world.sea_level as f32 * BLOCK_HEIGHT;
    let mut out = Vec::new();
    for t in terminals.list.values() {
        match t.site {
            Site::Port { tile, berth } => {
                let (cx, cz) = (tile.0 as f32 + 0.5, tile.1 as f32 + 0.5);
                let d = Vec2::new((berth.0 - tile.0) as f32, (berth.1 - tile.1) as f32);
                let yaw = yaw_of(d);
                if let Some(m) = models.pick(Role::Terminal, "port", year, t.id) {
                    out.push((m, ModelInstance::at([cx, world.height_at(cx, cz), cz], yaw)));
                }
                if let Some(m) = models.pick(Role::Prop, "pier", year, t.id) {
                    out.push((m, ModelInstance::at([cx + d.x * 0.95, sea_y, cz + d.y * 0.95], yaw)));
                }
            }
            Site::Airport(l) => {
                if let Some(m) = models.pick(Role::Terminal, "airport", year, t.id) {
                    // The model's origin is the north-west corner of the strip, runway along +X.
                    let origin_x = if l.along_x {
                        l.x0
                    } else {
                        l.x0 + crate::terminals::AIRPORT_DEPTH
                    };
                    out.push((
                        m,
                        ModelInstance::at([origin_x as f32, l.ground(), l.z0 as f32], l.yaw()),
                    ));
                }
            }
        }
    }
    out
}

pub fn boats(models: &Models, world: &World, boats: &Boats) -> Placed {
    let sea_y = world.sea_level as f32 * BLOCK_HEIGHT;
    boats
        .list
        .iter()
        .filter_map(|b| {
            let m = models.pick(Role::Boat, "ferry", b.born, 0)?;
            Some((m, ModelInstance::at([b.pos.x, sea_y, b.pos.y], b.heading)))
        })
        .collect()
}

/// Planes, with a shadow on the ground while they're in the air.
pub fn planes(models: &Models, world: &World, planes: &Planes) -> Placed {
    let shadow = models.pick(Role::Prop, "shadow", 2000, 0);
    let mut out = Vec::with_capacity(planes.list.len() * 2);
    for p in &planes.list {
        let Some(m) = models.pick(Role::Plane, "airliner", p.born, 0) else {
            continue;
        };
        out.push((m, ModelInstance::at(p.pos.to_array(), p.yaw)));
        if let (Some(s), true) = (shadow, p.is_airborne(world)) {
            let ground = world.height_at(p.pos.x, p.pos.z);
            out.push((s, ModelInstance::at([p.pos.x, ground + 0.03, p.pos.z], p.yaw)));
        }
    }
    out
}
