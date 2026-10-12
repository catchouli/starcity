//! Ports and airports: the places boats and planes call at.
//!
//! Both are "terminals": a named site on the map where passengers wait, which
//! towns serve from within a catchment, and which vehicles visit in the order
//! they're given (see [`Orders`]). A port is a single flat coastal tile with a
//! berth in the water beside it. An airport is a flat 10 × 3 tile strip: a
//! runway, an apron and a terminal row. Airports level their ground like
//! track does, for a price.

use crate::network::{self, Layer};
use crate::sea::Sea;
use crate::terrain::edit::Corners;
use crate::terrain::{BLOCK_HEIGHT, Shape, World};
use glam::Vec2;
use std::collections::{BTreeMap, HashMap};

pub type TerminalId = u32;

/// Runway length and strip depth, in tiles.
pub const AIRPORT_LEN: i32 = 10;
pub const AIRPORT_DEPTH: i32 = 3;

pub const PORT_COST: u32 = 2_500;
pub const AIRPORT_COST: u32 = 8_000;
/// A body of water smaller than this is a pond, not somewhere to sail.
const MIN_SEA: u32 = 200;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Port,
    Airport,
}

/// An airport's footprint: a rectangle of tiles, which way the runway runs,
/// and the height the ground was levelled to.
///
/// Local coordinates: `u` runs along the runway, `v` across it. Row 0 is the
/// runway, row 1 the apron, row 2 the terminal. (For a runway along z, `v`
/// grows towards -x, to match how the box renderer rotates props.)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AirportLayout {
    /// North-west tile of the footprint.
    pub x0: i32,
    pub z0: i32,
    pub along_x: bool,
    /// Ground height in blocks.
    pub level: i32,
}

impl AirportLayout {
    pub fn size(&self) -> (i32, i32) {
        if self.along_x {
            (AIRPORT_LEN, AIRPORT_DEPTH)
        } else {
            (AIRPORT_DEPTH, AIRPORT_LEN)
        }
    }

    pub fn tiles(&self) -> Vec<(i32, i32)> {
        let (w, h) = self.size();
        (0..h)
            .flat_map(|dz| (0..w).map(move |dx| (self.x0 + dx, self.z0 + dz)))
            .collect()
    }

    /// World x/z of local point (u, v), in tiles.
    pub fn point(&self, u: f32, v: f32) -> Vec2 {
        if self.along_x {
            Vec2::new(self.x0 as f32 + u, self.z0 as f32 + v)
        } else {
            Vec2::new((self.x0 + AIRPORT_DEPTH) as f32 - v, self.z0 as f32 + u)
        }
    }

    /// World direction of increasing `u`.
    pub fn u_dir(&self) -> Vec2 {
        if self.along_x { Vec2::X } else { Vec2::Y }
    }

    /// Rotation for boxes laid out in local coordinates.
    pub fn yaw(&self) -> f32 {
        if self.along_x {
            0.0
        } else {
            -std::f32::consts::FRAC_PI_2
        }
    }

    /// Height of the ground in world units.
    pub fn ground(&self) -> f32 {
        self.level as f32 * BLOCK_HEIGHT
    }

    pub fn centre(&self) -> Vec2 {
        self.point(AIRPORT_LEN as f32 / 2.0, AIRPORT_DEPTH as f32 / 2.0)
    }

    /// Where a plane parks.
    pub fn parking(&self) -> Vec2 {
        self.point(AIRPORT_LEN as f32 / 2.0, 1.5)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Site {
    /// A land tile on the coast, and the water tile boats tie up in.
    Port {
        tile: (i32, i32),
        berth: (i32, i32),
    },
    Airport(AirportLayout),
}

#[derive(Clone, Debug)]
pub struct Terminal {
    pub id: TerminalId,
    pub name: String,
    pub site: Site,
    /// Passengers waiting (fractional while they trickle in).
    pub waiting: f32,
}

impl Terminal {
    pub fn kind(&self) -> Kind {
        match self.site {
            Site::Port { .. } => Kind::Port,
            Site::Airport(_) => Kind::Airport,
        }
    }

    pub fn tiles(&self) -> Vec<(i32, i32)> {
        match self.site {
            Site::Port { tile, .. } => vec![tile],
            Site::Airport(a) => a.tiles(),
        }
    }

    /// The middle of the terminal, for its label.
    pub fn centre(&self) -> Vec2 {
        match self.site {
            Site::Port { tile, .. } => Vec2::new(tile.0 as f32 + 0.5, tile.1 as f32 + 0.5),
            Site::Airport(a) => a.centre(),
        }
    }

    pub fn berth(&self) -> Option<(i32, i32)> {
        match self.site {
            Site::Port { berth, .. } => Some(berth),
            Site::Airport(_) => None,
        }
    }

    pub fn airport(&self) -> Option<&AirportLayout> {
        match &self.site {
            Site::Airport(a) => Some(a),
            Site::Port { .. } => None,
        }
    }
}

#[derive(Clone, Default)]
pub struct Terminals {
    pub list: BTreeMap<TerminalId, Terminal>,
    by_tile: HashMap<(i32, i32), TerminalId>,
    next_id: TerminalId,
}

impl Terminals {
    pub fn at(&self, tile: (i32, i32)) -> Option<TerminalId> {
        self.by_tile.get(&tile).copied()
    }

    pub fn get(&self, id: TerminalId) -> Option<&Terminal> {
        self.list.get(&id)
    }

    pub fn of_kind(&self, kind: Kind) -> impl Iterator<Item = &Terminal> {
        self.list.values().filter(move |t| t.kind() == kind)
    }

    pub fn build(&mut self, site: Site, name: String) -> TerminalId {
        let id = self.next_id;
        self.next_id += 1;
        let terminal = Terminal {
            id,
            name,
            site,
            waiting: 0.0,
        };
        for t in terminal.tiles() {
            self.by_tile.insert(t, id);
        }
        self.list.insert(id, terminal);
        id
    }

    /// Removes every terminal with a tile in the inclusive rectangle.
    pub fn remove_in(&mut self, a: (i32, i32), b: (i32, i32)) -> Vec<Terminal> {
        let (x0, x1) = (a.0.min(b.0), a.0.max(b.0));
        let (z0, z1) = (a.1.min(b.1), a.1.max(b.1));
        let hit: Vec<TerminalId> = self
            .list
            .values()
            .filter(|t| {
                t.tiles()
                    .iter()
                    .any(|&(x, z)| (x0..=x1).contains(&x) && (z0..=z1).contains(&z))
            })
            .map(|t| t.id)
            .collect();
        let mut gone = Vec::new();
        for id in hit {
            if let Some(t) = self.list.remove(&id) {
                for tile in t.tiles() {
                    self.by_tile.remove(&tile);
                }
                gone.push(t);
            }
        }
        gone
    }
}

/// What's in the way of a site: roads, track and everything else built
/// (town buildings and other terminals mark themselves in `built`).
pub struct Blockers<'a> {
    pub roads: &'a Layer,
    pub rails: &'a Layer,
    pub built: &'a Layer,
}

impl Blockers<'_> {
    pub fn taken(&self, x: i32, z: i32) -> bool {
        self.roads.get(x, z) != 0 || self.rails.get(x, z) != 0 || self.built.get(x, z) != 0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SiteBlocked {
    Water,
    NotCoast,
    Pond,
    Slope,
    Occupied,
    TooSteep,
}

impl SiteBlocked {
    pub fn message(self, kind: Kind) -> &'static str {
        match (self, kind) {
            (SiteBlocked::Water, _) => "Can't build on water",
            (SiteBlocked::NotCoast, _) => "A port needs to be on the coast",
            (SiteBlocked::Pond, _) => "That water is too small to sail on",
            (SiteBlocked::Slope, _) => "A port needs flat ground",
            (SiteBlocked::Occupied, _) => "Something is in the way",
            (SiteBlocked::TooSteep, _) => "The ground is too steep to level for an airport",
        }
    }
}

/// A planned terminal: where it goes, what it costs, and any ground to level.
#[derive(Clone, Debug, Default)]
pub struct SitePlan {
    pub site: Option<Site>,
    pub tiles: Vec<(i32, i32)>,
    pub blocked: Option<SiteBlocked>,
    pub terraform: Vec<((i32, i32), u16)>,
    pub terraform_steps: u32,
}

impl SitePlan {
    fn blocked(tiles: Vec<(i32, i32)>, why: SiteBlocked) -> SitePlan {
        SitePlan {
            tiles,
            blocked: Some(why),
            ..Default::default()
        }
    }

    pub fn cost(&self) -> u32 {
        let base = match self.site {
            Some(Site::Airport(_)) => AIRPORT_COST,
            _ => PORT_COST,
        };
        base + self.terraform_steps * network::TERRAFORM_COST
    }
}

/// Plans a port on one tile: flat, free, dry, with water beside it that
/// boats can actually sail on.
pub fn plan_port(world: &World, sea: &Sea, blockers: &Blockers, tile: (i32, i32)) -> SitePlan {
    let (x, z) = tile;
    let tiles = vec![tile];
    let Some((base, shape)) = world.surface(x, z) else {
        return SitePlan::blocked(tiles, SiteBlocked::Water);
    };
    if sea.is_water(x, z) || base < world.sea_level {
        return SitePlan::blocked(tiles, SiteBlocked::Water);
    }
    if blockers.taken(x, z) {
        return SitePlan::blocked(tiles, SiteBlocked::Occupied);
    }
    if shape.0 != 0 {
        return SitePlan::blocked(tiles, SiteBlocked::Slope);
    }
    // The biggest body of water among the four neighbours is the berth.
    let berth = [(0, -1), (1, 0), (0, 1), (-1, 0)]
        .iter()
        .map(|&(dx, dz)| (x + dx, z + dz))
        .filter(|&(bx, bz)| sea.is_water(bx, bz))
        .max_by_key(|&(bx, bz)| sea.body_size(bx, bz));
    match berth {
        None => SitePlan::blocked(tiles, SiteBlocked::NotCoast),
        Some((bx, bz)) if sea.body_size(bx, bz) < MIN_SEA => SitePlan::blocked(tiles, SiteBlocked::Pond),
        Some(berth) => SitePlan {
            site: Some(Site::Port { tile, berth }),
            tiles,
            ..Default::default()
        },
    }
}

/// Plans an airport centred on `centre`, levelling the ground under it.
pub fn plan_airport(world: &World, sea: &Sea, blockers: &Blockers, centre: (i32, i32), along_x: bool) -> SitePlan {
    let (w, h) = if along_x {
        (AIRPORT_LEN, AIRPORT_DEPTH)
    } else {
        (AIRPORT_DEPTH, AIRPORT_LEN)
    };
    let (x0, z0) = (centre.0 - w / 2, centre.1 - h / 2);
    let mut layout = AirportLayout {
        x0,
        z0,
        along_x,
        level: 0,
    };
    let tiles = layout.tiles();
    for &(x, z) in &tiles {
        if !world.in_bounds(x, z) || sea.is_water(x, z) {
            return SitePlan::blocked(tiles, SiteBlocked::Water);
        }
        if blockers.taken(x, z) {
            return SitePlan::blocked(tiles, SiteBlocked::Occupied);
        }
    }

    // Level everything to the median corner height: the least earth moved.
    let mut corners = Corners::new(world);
    let mut heights: Vec<i32> = tiles
        .iter()
        .flat_map(|&(x, z)| corners.tile(x, z).unwrap_or([0; 4]))
        .collect();
    heights.sort();
    let level = heights[heights.len() / 2];
    for &(x, z) in &tiles {
        if !corners.level(x, z, level) {
            return SitePlan::blocked(tiles, SiteBlocked::TooSteep);
        }
    }
    let flat_and_dry = tiles
        .iter()
        .all(|&(x, z)| corners.surface(x, z) == Some((level as u16, Shape(0))));
    if !flat_and_dry || level < world.sea_level as i32 {
        return SitePlan::blocked(tiles, SiteBlocked::TooSteep);
    }
    // Moving the ground mustn't disturb anything built beside the airport.
    for (x, z) in corners.affected_tiles() {
        if !tiles.contains(&(x, z)) && blockers.taken(x, z) && corners.surface(x, z) != world.surface(x, z) {
            return SitePlan::blocked(tiles, SiteBlocked::Occupied);
        }
    }
    layout.level = level;
    SitePlan {
        site: Some(Site::Airport(layout)),
        tiles,
        blocked: None,
        terraform: corners.changes(),
        terraform_steps: corners.steps(),
    }
}

/// A vehicle's list of terminals to visit in turn, and which is next.
#[derive(Clone, Debug, Default)]
pub struct Orders {
    pub list: Vec<TerminalId>,
    pub current: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OrderEdit {
    Up(usize),
    Down(usize),
    Delete(usize),
    Add(TerminalId),
}

impl Orders {
    pub fn new(list: Vec<TerminalId>) -> Orders {
        Orders { list, current: 0 }
    }

    /// The terminal to head for next.
    pub fn target(&self) -> Option<TerminalId> {
        if self.list.is_empty() {
            None
        } else {
            Some(self.list[self.current % self.list.len()])
        }
    }

    pub fn advance(&mut self) {
        if !self.list.is_empty() {
            self.current = (self.current + 1) % self.list.len();
        }
    }

    pub fn apply(&mut self, edit: OrderEdit) {
        let cur = self.current;
        match edit {
            OrderEdit::Up(i) if i > 0 && i < self.list.len() => {
                self.list.swap(i, i - 1);
                self.current = if cur == i {
                    i - 1
                } else if cur == i - 1 {
                    i
                } else {
                    cur
                };
            }
            OrderEdit::Down(i) if i + 1 < self.list.len() => {
                self.list.swap(i, i + 1);
                self.current = if cur == i {
                    i + 1
                } else if cur == i + 1 {
                    i
                } else {
                    cur
                };
            }
            OrderEdit::Delete(i) if i < self.list.len() => {
                self.list.remove(i);
                if i < cur {
                    self.current -= 1;
                }
                if self.current >= self.list.len() {
                    self.current = 0;
                }
            }
            OrderEdit::Add(id) => self.list.push(id),
            _ => {}
        }
    }

    /// Drops a terminal that's gone from the list.
    pub fn forget(&mut self, id: TerminalId) {
        while let Some(i) = self.list.iter().position(|&t| t == id) {
            self.apply(OrderEdit::Delete(i));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terrain::generation::{self, GenParams};

    fn empty(n: usize) -> (Layer, Layer, Layer) {
        (Layer::new(n, n), Layer::new(n, n), Layer::new(n, n))
    }

    #[test]
    fn orders_move_and_forget() {
        let mut o = Orders::new(vec![1, 2, 3]);
        assert_eq!(o.target(), Some(1));
        o.advance();
        o.advance();
        assert_eq!(o.target(), Some(3));
        o.apply(OrderEdit::Up(2));
        assert_eq!((o.list.clone(), o.target()), (vec![1, 3, 2], Some(3)));
        o.forget(3);
        assert_eq!(o.list, vec![1, 2]);
        assert!(o.target().is_some());
        o.forget(1);
        o.forget(2);
        assert_eq!(o.target(), None);
    }

    #[test]
    fn airport_layout_maps_both_ways_inside_its_footprint() {
        for along_x in [true, false] {
            let a = AirportLayout {
                x0: 20,
                z0: 30,
                along_x,
                level: 8,
            };
            let tiles = a.tiles();
            assert_eq!(tiles.len(), (AIRPORT_LEN * AIRPORT_DEPTH) as usize);
            for (u, v) in [(0.5, 0.5), (9.5, 0.5), (5.0, 1.5), (3.5, 2.5), (9.5, 2.5)] {
                let p = a.point(u, v);
                assert!(
                    tiles.contains(&(p.x.floor() as i32, p.y.floor() as i32)),
                    "{along_x} ({u}, {v}) -> {p}"
                );
            }
            // The runway row is the far side from the terminal row, in world terms too.
            assert!((a.point(2.0, 0.5) - a.point(2.0, 2.5)).length() > 1.9);
            // +u runs in u_dir.
            assert!(((a.point(3.0, 1.0) - a.point(2.0, 1.0)) - a.u_dir()).length() < 1e-5);
        }
    }

    #[test]
    fn ports_need_flat_dry_coast_and_airports_level_the_ground() {
        let world = generation::generate(&GenParams::default());
        let sea = Sea::build(&world);
        let (roads, rails, built) = empty(512);
        let blockers = Blockers {
            roads: &roads,
            rails: &rails,
            built: &built,
        };
        let mut ports = 0;
        let mut airports = 0;
        let mut levelled = 0;
        'scan: for z in (20..492).step_by(3) {
            for x in (20..492).step_by(3) {
                let plan = plan_port(&world, &sea, &blockers, (x, z));
                if plan.blocked.is_none() {
                    ports += 1;
                    let Some(Site::Port { tile, berth }) = plan.site else {
                        panic!()
                    };
                    assert!(sea.is_water(berth.0, berth.1) && !sea.is_water(tile.0, tile.1));
                    assert_eq!((tile.0 - berth.0).abs() + (tile.1 - berth.1).abs(), 1);
                }
                if (x - 20) % 12 == 0 && (z - 20) % 12 == 0 {
                    let plan = plan_airport(&world, &sea, &blockers, (x, z), (x - 20) % 24 == 0);
                    if plan.blocked.is_none() {
                        airports += 1;
                        levelled += (plan.terraform_steps > 0) as u32;
                        assert!(plan.cost() >= AIRPORT_COST);
                        assert!(plan.tiles.iter().all(|&(tx, tz)| !sea.is_water(tx, tz)));
                    }
                }
                if ports > 30 && airports > 30 {
                    break 'scan;
                }
            }
        }
        assert!(ports > 10, "{ports} ports");
        assert!(
            airports > 5 && levelled > 0,
            "{airports} airports, {levelled} needed levelling"
        );
    }

    #[test]
    fn occupied_tiles_block_a_port() {
        let world = generation::generate(&GenParams::default());
        let sea = Sea::build(&world);
        // Find a tile a port fits on, then mark it taken.
        let (roads, rails, built) = empty(512);
        let blockers = Blockers {
            roads: &roads,
            rails: &rails,
            built: &built,
        };
        let mut occupied = built.clone();
        let tile = (0..512)
            .flat_map(|z| (0..512).map(move |x| (x, z)))
            .find(|&(x, z)| plan_port(&world, &sea, &blockers, (x, z)).blocked.is_none())
            .unwrap();
        occupied.set(tile.0, tile.1, 1);
        let taken = Blockers {
            roads: &roads,
            rails: &rails,
            built: &occupied,
        };
        assert_eq!(
            plan_port(&world, &sea, &taken, tile).blocked,
            Some(SiteBlocked::Occupied)
        );
    }
}
