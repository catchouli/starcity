//! Railway stations: runs of platform tiles along straight track.

use crate::network::{self, Blocked, Layer};
use crate::rails::{self, Signals};
use crate::terrain::World;
use std::collections::{BTreeMap, HashSet};

pub type StationId = u32;

pub struct Station {
    pub id: StationId,
    pub name: String,
    pub tiles: Vec<(i32, i32)>,
    /// Passengers waiting for a train (fractional while they trickle in).
    pub waiting: f32,
}

impl Station {
    /// The middle of the station, for its name label.
    pub fn centre(&self) -> (f32, f32) {
        let n = self.tiles.len().max(1) as f32;
        let (sx, sz) = self
            .tiles
            .iter()
            .fold((0.0, 0.0), |(x, z), t| (x + t.0 as f32, z + t.1 as f32));
        (sx / n + 0.5, sz / n + 0.5)
    }
}

#[derive(Default)]
pub struct Stations {
    pub list: BTreeMap<StationId, Station>,
    by_tile: BTreeMap<(i32, i32), StationId>,
    next_id: StationId,
}

/// A planned run of platform tiles along one axis.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StationPlan {
    pub tiles: Vec<(i32, i32)>,
    /// The straight track piece the platforms run along (X or Y).
    pub piece: u8,
    pub blocked: Option<Blocked>,
}

impl Stations {
    pub fn at(&self, tile: (i32, i32)) -> Option<StationId> {
        self.by_tile.get(&tile).copied()
    }

    pub fn get(&self, id: StationId) -> Option<&Station> {
        self.list.get(&id)
    }

    /// Plans platforms dragged from `a` to `b` along whichever axis the drag
    /// is mostly along. Platforms need flat ground, nothing but matching
    /// straight track, and no signals.
    pub fn plan(
        &self,
        world: &World,
        rails_layer: &Layer,
        roads: &Layer,
        buildings: &Layer,
        signals: &Signals,
        a: (i32, i32),
        b: (i32, i32),
    ) -> StationPlan {
        let (dx, dz) = (b.0 - a.0, b.1 - a.1);
        let along_x = dx.abs() >= dz.abs();
        // A single click follows the track that's already there, if any.
        let along_x = if dx == 0 && dz == 0 {
            rails_layer.get(a.0, a.1) != rails::Y
        } else {
            along_x
        };
        let (piece, step, len) = if along_x {
            (rails::X, (dx.signum(), 0), dx.abs())
        } else {
            (rails::Y, (0, dz.signum()), dz.abs())
        };
        let tiles: Vec<_> = (0..=len).map(|i| (a.0 + step.0 * i, a.1 + step.1 * i)).collect();
        let mut plan = StationPlan {
            tiles,
            piece,
            blocked: None,
        };
        for &(x, z) in &plan.tiles {
            let existing = rails_layer.get(x, z);
            let why = if roads.get(x, z) != 0 || buildings.get(x, z) != 0 || (existing != 0 && existing != piece) {
                Some(Blocked::Occupied)
            } else if signals.by_tile.contains_key(&(x, z)) {
                Some(Blocked::Signal)
            } else {
                match network::slope_check(world, x, z, true, true, false) {
                    // Platforms want flat ground, so any slope at all blocks them.
                    Ok(()) if world.surface(x, z).is_some_and(|(_, shape)| shape.0 == 0) => None,
                    Ok(()) => Some(Blocked::Slope),
                    Err(why) => Some(why),
                }
            };
            if why.is_some() {
                plan.blocked = why;
                break;
            }
        }
        plan
    }

    /// Builds a plan: lays the track and adds the tiles to a station. Tiles
    /// touching an existing station join it; otherwise a new one is made.
    /// Returns the tiles whose track changed.
    pub fn build(&mut self, plan: &StationPlan, rails_layer: &mut Layer) -> Vec<(i32, i32)> {
        if plan.blocked.is_some() || plan.tiles.is_empty() {
            return Vec::new();
        }
        let neighbour = plan.tiles.iter().find_map(|&(x, z)| {
            [(x, z), (x + 1, z), (x - 1, z), (x, z + 1), (x, z - 1)]
                .iter()
                .find_map(|t| self.at(*t))
        });
        let id = neighbour.unwrap_or_else(|| {
            let id = self.next_id;
            self.next_id += 1;
            self.list.insert(
                id,
                Station {
                    id,
                    name: format!("Station {}", id + 1),
                    tiles: Vec::new(),
                    waiting: 0.0,
                },
            );
            id
        });
        let mut changed = Vec::new();
        for &t in &plan.tiles {
            if rails_layer.get(t.0, t.1) != plan.piece {
                rails_layer.set(t.0, t.1, plan.piece);
                changed.push(t);
            }
            if self.by_tile.insert(t, id).is_none() {
                self.list.get_mut(&id).unwrap().tiles.push(t);
            }
        }
        changed
    }

    pub fn tile_count(&self) -> usize {
        self.by_tile.len()
    }

    /// Drops platform tiles whose track has gone, and stations left with no
    /// tiles. Returns the ids of stations removed entirely.
    pub fn prune(&mut self, rails_layer: &Layer) -> Vec<StationId> {
        self.by_tile
            .retain(|&(x, z), _| matches!(rails_layer.get(x, z), rails::X | rails::Y));
        let live: HashSet<_> = self.by_tile.keys().copied().collect();
        let mut removed = Vec::new();
        for station in self.list.values_mut() {
            station.tiles.retain(|t| live.contains(t));
            if station.tiles.is_empty() {
                removed.push(station.id);
            }
        }
        for id in &removed {
            self.list.remove(id);
        }
        removed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terrain::{Block, Material, Shape};

    fn flat_world(n: usize) -> World {
        let mut w = World::new(n, n, 4, 0);
        for x in 0..n {
            for z in 0..n {
                w.set(x, 0, z, Block::full(Material::Grass));
            }
        }
        w
    }

    #[test]
    fn platforms_lay_track_and_join_up() {
        let w = flat_world(16);
        let mut r = Layer::new(16, 16);
        let roads = Layer::new(16, 16);
        let sigs = Signals::default();
        let mut st = Stations::default();
        let p = st.plan(&w, &r, &roads, &Layer::new(16, 16), &sigs, (2, 3), (5, 4));
        assert_eq!(p.tiles.len(), 4, "a mostly-x drag runs along x");
        assert_eq!(p.piece, rails::X);
        st.build(&p, &mut r);
        assert_eq!(r.get(4, 3), rails::X);
        let id = st.at((2, 3)).unwrap();
        // Extending the platform joins the same station.
        let p = st.plan(&w, &r, &roads, &Layer::new(16, 16), &sigs, (6, 3), (7, 3));
        st.build(&p, &mut r);
        assert_eq!(st.at((7, 3)), Some(id));
        assert_eq!(st.get(id).unwrap().tiles.len(), 6);
        // Somewhere else is a new station.
        let p = st.plan(&w, &r, &roads, &Layer::new(16, 16), &sigs, (2, 10), (5, 10));
        st.build(&p, &mut r);
        assert_ne!(st.at((3, 10)), Some(id));
        assert_eq!(st.list.len(), 2);
    }

    #[test]
    fn platforms_need_flat_clear_ground() {
        let mut w = flat_world(16);
        w.set(
            4,
            1,
            3,
            Block {
                material: Material::Grass,
                shape: Shape(0b0011),
            },
        );
        let mut r = Layer::new(16, 16);
        let sigs = Signals::default();
        let st = Stations::default();
        let p = st.plan(&w, &r, &Layer::new(16, 16), &Layer::new(16, 16), &sigs, (2, 3), (6, 3));
        assert_eq!(p.blocked, Some(Blocked::Slope));
        r.set(3, 8, rails::Y);
        let p = st.plan(&w, &r, &Layer::new(16, 16), &Layer::new(16, 16), &sigs, (1, 8), (5, 8));
        assert_eq!(p.blocked, Some(Blocked::Occupied), "crossing track the other way");
    }

    #[test]
    fn removing_track_removes_platforms_and_empty_stations() {
        let w = flat_world(16);
        let mut r = Layer::new(16, 16);
        let mut st = Stations::default();
        let p = st.plan(
            &w,
            &r,
            &Layer::new(16, 16),
            &Layer::new(16, 16),
            &Signals::default(),
            (2, 3),
            (3, 3),
        );
        st.build(&p, &mut r);
        r.set(2, 3, 0);
        assert!(st.prune(&r).is_empty());
        r.set(3, 3, 0);
        assert_eq!(st.prune(&r).len(), 1);
        assert!(st.list.is_empty());
    }
}
