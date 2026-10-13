//! Rising seas in the app (GAME-DESIGN.md §10.9): the monthly flood, the warnings before each rise,
//! and the sea walls that keep it out.

use super::*;
use crate::flood;

impl StarcityApp {
    /// The tiles a sea wall dragged from `a` to `b` would take: a straight or 45° line, red where the
    /// ground is already under water or something is built.
    pub(super) fn wall_plan(&self, a: (i32, i32), b: (i32, i32)) -> Plan {
        let mut plan = crate::roads::shape(&Layer::new(self.walls.size_x, self.walls.size_z), a, b);
        let blocked = plan.pieces.iter().find_map(|p| {
            let (x, z) = p.tile;
            if self.world.underwater(x, z) {
                Some(crate::network::Blocked::Water)
            } else if self.towns.occupied.get(x, z) != 0 && self.walls.get(x, z) == 0
                || self.rails.get(x, z) != 0
                || self.roads.get(x, z) != 0
            {
                Some(crate::network::Blocked::Occupied)
            } else {
                None
            }
        });
        plan.blocked = blocked;
        plan
    }

    pub(super) fn build_wall(&mut self, a: (i32, i32), b: (i32, i32)) {
        let plan = self.wall_plan(a, b);
        let now = self.last_time;
        if let Some(why) = plan.blocked {
            let text = match why {
                crate::network::Blocked::Water => "Sea walls go on dry land, along the coast",
                _ => "Something is in the way: walls can't cross buildings, roads or track",
            };
            self.show_message(text, now);
            return;
        }
        let tiles: Vec<(i32, i32)> = plan
            .pieces
            .iter()
            .map(|p| p.tile)
            .filter(|&(x, z)| self.walls.get(x, z) == 0)
            .collect();
        if tiles.is_empty() || !self.pay(Item::Construction, tiles.len() as u32 * flood::WALL) {
            return;
        }
        for &(x, z) in &tiles {
            self.walls.set(x, z, 1);
            self.towns.occupied.set(x, z, 4);
        }
        self.flood.walls_changed(&self.world, &self.walls);
        self.static_changed = true;
        self.flood_warning = None;
    }

    /// Bulldozing removes walls in the rectangle. Returns whether any went.
    pub(super) fn clear_walls(&mut self, a: (i32, i32), b: (i32, i32)) -> bool {
        let mut gone = false;
        for z in a.1.min(b.1)..=a.1.max(b.1) {
            for x in a.0.min(b.0)..=a.0.max(b.0) {
                if self.walls.get(x, z) != 0 {
                    self.walls.set(x, z, 0);
                    self.towns.occupied.set(x, z, 0);
                    gone = true;
                }
            }
        }
        gone
    }

    /// Once a month: the sea rises when its year comes, floods a share of what it will take, and
    /// warns ahead of the next rise.
    pub(super) fn flood_month(&mut self) {
        let date = self.clock.year_f64() as f32;
        let target = self.flood.start + flood::rise(date);
        let now = self.last_time;
        if self.world.sea_level < target {
            let queued = self.flood.raise(&mut self.world, target, &self.walls);
            // The endless ocean beyond the map rises with it.
            self.renderer.ocean[1] = self.world.sea_level as f32 * crate::terrain::BLOCK_HEIGHT;
            self.scene_dirty = true;
            self.show_message(
                &format!("The sea has risen. {queued} tiles of low coast will flood over the coming months."),
                now + 6.0,
            );
        }
        let flooded = self.flood.month(&mut self.world);
        if !flooded.is_empty() {
            for &t in &flooded {
                self.towns.demolish(t, t);
                self.remove_terminals(t, t);
                let changes = self
                    .sim
                    .clear(&mut self.world, &mut self.roads, &mut self.rails, t, t, Clear::Everything);
                self.mark_infra(&changes.infra);
                if let Some(id) = self.industries.at(t) {
                    for tile in self.industries.release(id) {
                        self.towns.occupied.set(tile.0, tile.1, 0);
                    }
                    self.industries.list[id].closed = true;
                }
            }
            let gone: std::collections::HashSet<_> = flooded.iter().copied().collect();
            if self.trains.remove_on_tiles(&gone) {
                self.props_changed = true;
            }
            if let Some(sea) = &mut self.sea {
                sea.refresh(&self.world, &flooded);
            }
            self.levelled_pending.extend(flooded);
            self.static_changed = true;
            self.track_version += 1;
            self.plan_epoch += 1;
        }
        // Warnings: which towns the next rise would reach, while there's time to build walls.
        self.flood_warning = flood::coming(date).and_then(|year| {
            let risk = self.flood.at_risk(&self.world, &self.walls);
            let mut hit: Vec<(String, usize)> = Vec::new();
            for t in &self.towns.list {
                let n = risk
                    .iter()
                    .filter(|&&p| self.towns.buildings.get(&p).is_some_and(|b| b.town == t.id))
                    .count();
                if n > 0 {
                    hit.push((t.name.clone(), n));
                }
            }
            hit.sort_by(|a, b| b.1.cmp(&a.1));
            let (name, n) = hit.first()?;
            let more = if hit.len() > 1 { format!(" ({} other towns too)", hit.len() - 1) } else { String::new() };
            Some(format!(
                "The sea rises in {year:.0}: {name} would lose {n} buildings{more}. A sea wall along its coast (Land toolbar) keeps it out."
            ))
        });
    }
}
