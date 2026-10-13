//! Ports, airports, boats and planes in the app: building them, their
//! windows, the props that draw them, passenger loading, and the demo.

use super::*;
use crate::catalog::{BOATS, BUSES, PLANES, TRUCKS};
use crate::sea::Sea;
use crate::terminals::{Blockers, Kind, OrderEdit, Orders, SitePlan, TerminalId, plan_airport, plan_port};
use crate::terrain::{BLOCK_HEIGHT, edit};
use glam::Vec3Swizzles as _;

/// A boat or a plane.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Craft {
    Boat(u32),
    Plane(u32),
    Bus(u32),
}

enum CraftAction {
    Edit(OrderEdit),
    ToggleStop,
    Remove,
    Find,
}

fn thousands(n: u32) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

impl StarcityApp {
    /// The water grid, built the first time anything needs it.
    pub(super) fn sea(&mut self) -> &Sea {
        if self.sea.is_none() {
            let t = Instant::now();
            self.sea = Some(Sea::build(&self.world));
            self.sea_ms = t.elapsed().as_secs_f32() * 1000.0;
        }
        self.sea.as_ref().unwrap()
    }

    fn blockers(&self) -> Blockers<'_> {
        Blockers {
            roads: &self.roads,
            rails: &self.rails,
            built: &self.towns.occupied,
        }
    }

    /// Plans what the port or airport tool would build at a tile.
    fn plan_site(&mut self, tool: Tool, tile: (i32, i32)) -> Option<SitePlan> {
        self.sea();
        let sea = self.sea.as_ref().unwrap();
        let blockers = self.blockers();
        match tool {
            Tool::Port => Some(plan_port(&self.world, sea, &blockers, tile)),
            Tool::Airport => Some(plan_airport(&self.world, sea, &blockers, tile, self.runway_along_x)),
            _ => None,
        }
    }

    /// Hover feedback for the port, airport, boat and plane tools: the
    /// highlighted footprint, and the price or the reason it can't be built.
    pub(super) fn sea_tool_hover(&mut self, tool: Tool) -> Option<Highlight> {
        self.site_hint = None;
        let tile = self.hover_tile?;
        match tool {
            Tool::Port | Tool::Airport => {
                let kind = if tool == Tool::Port { Kind::Port } else { Kind::Airport };
                let plan = self.plan_site(tool, tile)?;
                let (mut lo, mut hi) = (plan.tiles[0], plan.tiles[0]);
                for &(x, z) in &plan.tiles {
                    (lo, hi) = ((lo.0.min(x), lo.1.min(z)), (hi.0.max(x), hi.1.max(z)));
                }
                if plan.blocked.is_none() {
                    self.show_cost(
                        self.company.build_price(plan.cost()),
                        self.company.build_price(plan.terraform_steps * crate::network::TERRAFORM_COST),
                    );
                }
                self.site_hint = Some(match plan.blocked {
                    Some(why) => why.message(kind).to_string(),
                    None if plan.terraform_steps > 0 => format!(
                        "Build {}: £{} (£{} levelling the ground)",
                        if kind == Kind::Port { "port" } else { "airport" },
                        thousands(plan.cost()),
                        thousands(plan.terraform_steps * crate::network::TERRAFORM_COST)
                    ),
                    None => format!(
                        "Build {}: £{}",
                        if kind == Kind::Port { "port" } else { "airport" },
                        thousands(plan.cost())
                    ),
                });
                Some(Highlight {
                    a: lo,
                    b: hi,
                    kind: if plan.blocked.is_some() {
                        HighlightKind::Bulldoze
                    } else {
                        HighlightKind::Cursor
                    },
                })
            }
            Tool::Boat => {
                let water = self.sea().is_water(tile.0, tile.1);
                if water {
                    self.show_cost(BOATS[self.buy_boat].price, 0);
                }
                self.site_hint = Some(if water {
                    format!("Click to put a {} here", BOATS[self.buy_boat].name.to_lowercase())
                } else {
                    "Ferries go on water".to_string()
                });
                Some(Highlight {
                    a: tile,
                    b: tile,
                    kind: if water {
                        HighlightKind::Cursor
                    } else {
                        HighlightKind::Bulldoze
                    },
                })
            }
            Tool::BusStop => {
                let road = self.roads.get(tile.0, tile.1) != 0 && self.terminals.at(tile).is_none();
                if road {
                    self.show_cost(self.company.build_price(company::BUS_STOP), 0);
                }
                self.site_hint = Some(if road {
                    format!("Build a bus stop: £{}", thousands(company::BUS_STOP))
                } else {
                    "Bus stops go on a road".to_string()
                });
                Some(Highlight {
                    a: tile,
                    b: tile,
                    kind: if road { HighlightKind::Cursor } else { HighlightKind::Bulldoze },
                })
            }
            Tool::Bus | Tool::Truck => {
                let stop = self
                    .terminals
                    .at(tile)
                    .and_then(|id| self.terminals.get(id))
                    .is_some_and(|t| t.kind() == Kind::BusStop);
                let (price, what) = if tool == Tool::Bus {
                    (BUSES[self.buy_bus].price, "bus")
                } else {
                    (TRUCKS[self.buy_truck].price, "truck")
                };
                if stop {
                    self.show_cost(price, 0);
                }
                self.site_hint = Some(if stop {
                    format!("Click to put a {what} here")
                } else {
                    format!("Click a road stop to put a {what} there")
                });
                Some(Highlight {
                    a: tile,
                    b: tile,
                    kind: if stop { HighlightKind::Cursor } else { HighlightKind::Bulldoze },
                })
            }
            Tool::Plane => {
                let airport = self
                    .terminals
                    .at(tile)
                    .and_then(|id| self.terminals.get(id))
                    .is_some_and(|t| t.kind() == Kind::Airport);
                if airport {
                    self.show_cost(PLANES[self.buy_plane].price, 0);
                }
                self.site_hint = Some(if airport {
                    "Click to park a plane here".to_string()
                } else {
                    "Click an airport to place a plane".to_string()
                });
                Some(Highlight {
                    a: tile,
                    b: tile,
                    kind: if airport {
                        HighlightKind::Cursor
                    } else {
                        HighlightKind::Bulldoze
                    },
                })
            }
            _ => None,
        }
    }

    /// A click with one of the sea and air tools. Returns terrain tiles that
    /// were levelled (to re-mesh).
    pub(super) fn sea_tool_click(&mut self, tool: Tool, tile: (i32, i32), now: f64) -> Vec<(i32, i32)> {
        match tool {
            Tool::Port | Tool::Airport => {
                let Some(plan) = self.plan_site(tool, tile) else {
                    return Vec::new();
                };
                let kind = if tool == Tool::Port { Kind::Port } else { Kind::Airport };
                if let Some(why) = plan.blocked {
                    self.show_message(why.message(kind), now);
                    return Vec::new();
                }
                if !self.pay(Item::Construction, plan.cost()) {
                    return Vec::new();
                }
                let tiles = edit::apply(&mut self.world, &plan.terraform);
                let site = plan.site.unwrap();
                let id = self.terminals.build(site, String::new());
                for &(x, z) in &plan.tiles {
                    self.towns.occupied.set(x, z, 2);
                }
                self.name_terminal(id);
                self.static_changed = true;
                let name = self.terminals.get(id).map(|t| t.name.clone()).unwrap_or_default();
                self.show_message(&format!("Built {name} (£{})", thousands(plan.cost())), now);
                tiles
            }
            Tool::Boat => {
                if self.sea().is_water(tile.0, tile.1) {
                    let model = BOATS[self.buy_boat];
                    if !self.can_add_vehicle(model.price) {
                        return Vec::new();
                    }
                    let sea = self.sea.as_ref().unwrap();
                    if let Some(id) = self.boats.spawn(sea, tile) {
                        self.company.spend(Item::Vehicles, model.price);
                        if let Some(b) = self.boats.get_mut(id) {
                            (b.speed, b.capacity, b.price, b.running) = (model.speed, model.capacity, model.price, model.running);
                        }
                        self.selected_craft = Some(Craft::Boat(id));
                        self.props_changed = true;
                    }
                } else {
                    self.show_message("Ferries go on water", now);
                }
                Vec::new()
            }
            Tool::BusStop => {
                if self.roads.get(tile.0, tile.1) == 0 || self.terminals.at(tile).is_some() {
                    self.show_message("Bus stops go on a road", now);
                    return Vec::new();
                }
                if !self.pay(Item::Construction, company::BUS_STOP) {
                    return Vec::new();
                }
                let id = self.terminals.build(crate::terminals::Site::BusStop { tile }, String::new());
                self.name_terminal(id);
                self.static_changed = true;
                let name = self.terminals.get(id).map(|t| t.name.clone()).unwrap_or_default();
                self.show_message(&format!("Built {name}"), now);
                Vec::new()
            }
            Tool::Bus | Tool::Truck => {
                match self.terminals.at(tile) {
                    Some(at) if self.terminals.get(at).is_some_and(|t| t.kind() == Kind::BusStop) => {
                        let model = if tool == Tool::Bus { BUSES[self.buy_bus] } else { TRUCKS[self.buy_truck] };
                        let cargo = if tool == Tool::Bus { Cargo::Passengers } else { self.truck_cargo };
                        if !self.can_add_vehicle(model.price) {
                            return Vec::new();
                        }
                        if let Some(id) = self.buses.spawn(&self.roads, tile) {
                            self.company.spend(Item::Vehicles, model.price);
                            if let Some(b) = self.buses.get_mut(id) {
                                (b.speed, b.capacity, b.price, b.running) = (model.speed, model.capacity, model.price, model.running);
                                b.cargo = cargo;
                            }
                            self.selected_craft = Some(Craft::Bus(id));
                            self.props_changed = true;
                        }
                    }
                    _ => self.show_message("Buses and trucks start at a road stop", now),
                }
                Vec::new()
            }
            Tool::Plane => {
                match self.terminals.at(tile) {
                    Some(at) if self.terminals.get(at).is_some_and(|t| t.kind() == Kind::Airport) => {
                        let model = PLANES[self.buy_plane];
                        if !self.can_add_vehicle(model.price) {
                            return Vec::new();
                        }
                        if let Some(id) = self.planes.spawn(&self.terminals, at) {
                            self.company.spend(Item::Vehicles, model.price);
                            if let Some(p) = self.planes.get_mut(id) {
                                (p.pace, p.capacity, p.price, p.running) =
                                    (model.speed / 9.0, model.capacity, model.price, model.running);
                            }
                            self.selected_craft = Some(Craft::Plane(id));
                            self.props_changed = true;
                        }
                    }
                    _ => self.show_message("Planes go on an airport", now),
                }
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    /// Names a terminal after the nearest town: "Ashford Harbour", or with a
    /// compass point if that's taken.
    pub(super) fn name_terminal(&mut self, id: TerminalId) {
        let Some(t) = self.terminals.get(id) else { return };
        let (kind, centre) = (t.kind(), t.centre());
        let word = match kind {
            Kind::Port => "Harbour",
            Kind::Airport => "Airport",
            Kind::BusStop => "Stop",
        };
        let near = self.towns.nearest((centre.x as i32, centre.y as i32), 40);
        let name = match near {
            None => format!("{word} {}", id + 1),
            Some(town) => {
                let town = &self.towns.list[town as usize];
                let (dx, dz) = (centre.x - town.centre.0 as f32, centre.y - town.centre.1 as f32);
                let compass = if dx.abs() > dz.abs() {
                    if dx > 0.0 { "East" } else { "West" }
                } else if dz > 0.0 {
                    "South"
                } else {
                    "North"
                };
                let taken = |name: &str| self.terminals.list.values().any(|o| o.id != id && o.name == name);
                [
                    format!("{} {word}", town.name),
                    format!("{} {compass} {word}", town.name),
                    format!("{} {word} {}", town.name, id + 1),
                ]
                .into_iter()
                .find(|n| !taken(n))
                .unwrap_or_else(|| format!("{word} {}", id + 1))
            }
        };
        self.terminals.list.get_mut(&id).unwrap().name = name;
    }

    /// Bulldozing: ports and airports in the rectangle go, and with them
    /// their vehicles' orders (and parked planes).
    pub(super) fn remove_terminals(&mut self, a: (i32, i32), b: (i32, i32)) {
        let gone = self.terminals.remove_in(a, b);
        if gone.is_empty() {
            return;
        }
        for t in &gone {
            for tile in t.tiles() {
                self.towns.occupied.set(tile.0, tile.1, 0);
            }
            self.boats.forget_terminal(t.id);
            self.buses.forget_terminal(t.id);
            self.planes.forget_terminal(t.id, &self.world, &self.terminals);
            if self.selected_terminal == Some(t.id) {
                self.selected_terminal = None;
            }
        }
        self.static_changed = true;
        self.props_changed = true;
    }

    /// Steps boats and planes. Returns whether any is busy.
    pub(super) fn step_craft(&mut self, step: f32) -> bool {
        let mut busy = false;
        if let Some(sea) = &self.sea {
            busy |= self.boats.update(step, sea, &self.terminals);
        }
        busy |= self.planes.update(step, &self.world, &self.terminals);
        busy |= self.buses.update(step, &self.roads, &self.terminals);
        busy
    }

    /// Once a second: people near each terminal turn up to wait.
    pub(super) fn tick_terminals(&mut self) {
        let now = self.clock.days();
        for t in self.terminals.list.values_mut() {
            let people = self.towns.population_near(&t.tiles()) as f32;
            t.waiting = (t.waiting + people * 0.01).min(500.0);
            if t.kind() == Kind::BusStop {
                for c in Cargo::ALL {
                    if c != Cargo::Passengers {
                        t.service[c.index()].update(now, t.stock.get(c), 1.0);
                    }
                }
            }
        }
    }

    /// Boats and planes that have just docked or parked: unload everyone
    /// (crediting the nearest town) and load from the waiting passengers.
    pub(super) fn handle_craft_arrivals(&mut self) {
        let boat_calls = std::mem::take(&mut self.boats.arrivals);
        let bus_calls = std::mem::take(&mut self.buses.arrivals);
        let plane_calls = std::mem::take(&mut self.planes.arrivals);
        let calls = boat_calls
            .into_iter()
            .map(|(id, t)| (Craft::Boat(id), t))
            .chain(plane_calls.into_iter().map(|(id, t)| (Craft::Plane(id), t)))
            .chain(bus_calls.into_iter().map(|(id, t)| (Craft::Bus(id), t)));
        for (craft, at) in calls {
            if let Craft::Bus(id) = craft {
                if self.buses.get(id).is_some_and(|b| b.cargo != Cargo::Passengers) {
                    self.truck_arrives(id, at);
                    continue;
                }
            }
            let Some(terminal) = self.terminals.list.get_mut(&at) else {
                continue;
            };
            let (passengers, boarded, capacity) = match craft {
                Craft::Boat(id) => match self.boats.get_mut(id) {
                    Some(b) => (&mut b.passengers, &mut b.boarded, b.capacity),
                    None => continue,
                },
                Craft::Plane(id) => match self.planes.get_mut(id) {
                    Some(p) => (&mut p.passengers, &mut p.boarded, p.capacity),
                    None => continue,
                },
                Craft::Bus(id) => match self.buses.get_mut(id) {
                    Some(b) => (&mut b.passengers, &mut b.boarded, b.capacity),
                    None => continue,
                },
            };
            let delivered = std::mem::take(passengers);
            let came_from = boarded.take();
            let boarding = (terminal.waiting as u32).min(capacity);
            *passengers = boarding;
            terminal.waiting -= boarding as f32;
            let c = terminal.centre();
            if boarding > 0 {
                *boarded = Some((c.x, c.y, self.clock.days()));
            }
            self.pay_fare(Cargo::Passengers, delivered, came_from, (c.x, c.y));
            if let Some(town) = self.towns.nearest((c.x as i32, c.y as i32), 40) {
                let town = &mut self.towns.list[town as usize];
                town.served += delivered as f32;
                town.delivered += delivered;
            }
        }
    }

    /// A truck at a road stop: what the industries and towns nearby take comes off and is paid for,
    /// then it loads what's waiting of its cargo.
    fn truck_arrives(&mut self, id: u32, at: TerminalId) {
        let Some(t) = self.terminals.get(at) else { return };
        let (tiles, c) = (t.tiles(), t.centre());
        let Some(truck) = self.buses.get(id) else { return };
        let (cargo, load) = (truck.cargo, truck.passengers);
        let room = super::industry_ui::room_for(&self.industries, &self.towns, &tiles, cargo);
        let units = load.min(room);
        let came_from = if units > 0 {
            let truck = self.buses.get_mut(id).unwrap();
            truck.passengers -= units;
            truck.boarded.take()
        } else {
            None
        };
        if units > 0 {
            let left = self.deliver_to_industries(&tiles, cargo, units);
            if left > 0 && super::industry_ui::is_for_towns(cargo) {
                if let Some(town) = self.towns.nearest((c.x as i32, c.y as i32), 30) {
                    self.towns.list[town as usize].receive(cargo, left);
                }
            }
            self.pay_fare(cargo, units, came_from, (c.x, c.y));
        }
        let now = self.clock.days();
        let year = self.year;
        let (Some(truck), Some(stop)) = (self.buses.get_mut(id), self.terminals.list.get_mut(&at)) else {
            return;
        };
        let boarding = stop.stock.take(cargo, truck.capacity - truck.passengers);
        if boarding > 0 {
            if truck.passengers == 0 {
                truck.boarded = Some((c.x, c.y, now));
            }
            truck.passengers += boarding;
        }
        stop.service[cargo.index()].loaded(now, truck.speed, year - truck.born);
    }

    /// Pointer clicks: the boat or plane under the cursor, if any.
    pub(super) fn pick_craft(&self, view_proj: Mat4, rect: egui::Rect, pos: egui::Pos2) -> Option<Craft> {
        let sea_y = self.world.sea_level as f32 * BLOCK_HEIGHT;
        let boats = self
            .boats
            .list
            .iter()
            .map(|b| (Craft::Boat(b.id), Vec3::new(b.pos.x, sea_y + 0.2, b.pos.y)));
        let planes = self.planes.list.iter().map(|p| (Craft::Plane(p.id), p.pos));
        let buses = self.buses.list.iter().map(|b| {
            let (p, _) = b.placement();
            (Craft::Bus(b.id), Vec3::new(p.x, self.world.height_at(p.x, p.y) + 0.2, p.y))
        });
        boats
            .chain(planes)
            .chain(buses)
            .filter_map(|(c, p)| Some((c, project(view_proj, rect, p)?.distance(pos))))
            .filter(|&(_, d)| d < 28.0)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(c, _)| c)
    }

    /// Name labels for ports and airports (the same tooltip look as stations).
    pub(super) fn terminal_labels(&self, painter: &egui::Painter, view_proj: Mat4, rect: egui::Rect) {
        let font = egui::FontId::proportional(12.0);
        for t in self.terminals.list.values() {
            let c = t.centre();
            let p = Vec3::new(c.x, self.world.height_at(c.x, c.y) + 1.6, c.y);
            let Some(at) = project(view_proj, rect, p) else {
                continue;
            };
            let galley = painter.layout_no_wrap(t.name.clone(), font.clone(), egui::Color32::BLACK);
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
    }

    /// The window shared by boats and planes: status, orders, stop and go.
    fn craft_window(
        &mut self,
        ctx: &egui::Context,
        craft: Craft,
        title: String,
        status: String,
        load: (u32, u32),
        stopped: bool,
        orders: Orders,
        kind: Kind,
    ) {
        let name = |id: TerminalId| self.terminals.get(id).map_or("?".to_string(), |t| t.name.clone());
        let choices: Vec<(TerminalId, String)> = self.terminals.of_kind(kind).map(|t| (t.id, t.name.clone())).collect();
        let current = if orders.list.is_empty() {
            0
        } else {
            orders.current % orders.list.len()
        };
        let (noun, plural) = match kind {
            Kind::Port => ("port", "ports"),
            Kind::Airport => ("airport", "airports"),
            Kind::BusStop => ("bus stop", "bus stops"),
        };
        let mut action = None;
        let mut open = true;
        let salt = match craft {
            Craft::Boat(id) => ("boat", id),
            Craft::Plane(id) => ("plane", id),
            Craft::Bus(id) => ("bus", id),
        };
        ui::win95::window(ctx, &title, &mut open, egui::pos2(420.0, 110.0), |ui| {
            ui.set_width(240.0);
            ui.label(status);
            let unit = match craft {
                Craft::Bus(id) => self.buses.get(id).map_or("passengers", |b| b.cargo.unit()),
                _ => "passengers",
            };
            ui.label(format!("Carrying {} / {} {unit}", load.0, load.1));
            ui.separator();
            ui.label("Orders:");
            if orders.list.is_empty() {
                ui.weak(format!("None yet. Add {plural} below, and it will visit them in turn."));
            }
            for (i, &tid) in orders.list.iter().enumerate() {
                ui.horizontal(|ui| {
                    let marker = if i == current { "▶" } else { "   " };
                    ui.label(format!("{marker} {}. {}", i + 1, name(tid)));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("x").on_hover_text("Delete").clicked() {
                            action = Some(CraftAction::Edit(OrderEdit::Delete(i)));
                        }
                        if ui.small_button("⏷").on_hover_text("Move down").clicked() {
                            action = Some(CraftAction::Edit(OrderEdit::Down(i)));
                        }
                        if ui.small_button("⏶").on_hover_text("Move up").clicked() {
                            action = Some(CraftAction::Edit(OrderEdit::Up(i)));
                        }
                    });
                });
            }
            egui::ComboBox::from_id_salt(("add stop", salt))
                .selected_text("Add stop…")
                .show_ui(ui, |ui| {
                    if choices.is_empty() {
                        ui.weak(format!("No {plural} yet"));
                    }
                    for (id, n) in &choices {
                        if ui.selectable_label(false, n).clicked() {
                            action = Some(CraftAction::Edit(OrderEdit::Add(*id)));
                        }
                    }
                });
            ui.separator();
            ui.horizontal(|ui| {
                if ui.button(if stopped { "Go" } else { "Stop" }).clicked() {
                    action = Some(CraftAction::ToggleStop);
                }
                if ui.button("Find").on_hover_text("Centre the view on it").clicked() {
                    action = Some(CraftAction::Find);
                }
                if ui
                    .button(format!(
                        "Remove {}",
                        match kind {
                            Kind::Port => "boat",
                            Kind::Airport => "plane",
                            Kind::BusStop => "bus",
                        }
                    ))
                    .clicked()
                {
                    action = Some(CraftAction::Remove);
                }
            });
            let _ = noun;
        });
        if !open {
            self.selected_craft = None;
        }
        let Some(action) = action else { return };
        let focus = match craft {
            Craft::Boat(id) => self.boats.get(id).map(|b| b.pos),
            Craft::Plane(id) => self.planes.get(id).map(|p| p.pos.xz()),
            Craft::Bus(id) => self.buses.get(id).map(|b| b.placement().0),
        };
        match action {
            CraftAction::Edit(edit) => {
                let mut o = orders;
                o.apply(edit);
                match craft {
                    Craft::Boat(id) => self.boats.set_orders(id, o),
                    Craft::Plane(id) => self.planes.set_orders(id, o),
                    Craft::Bus(id) => self.buses.set_orders(id, o),
                }
            }
            CraftAction::ToggleStop => match craft {
                Craft::Boat(id) => {
                    if let Some(b) = self.boats.get_mut(id) {
                        b.stopped = !b.stopped;
                    }
                }
                Craft::Plane(id) => {
                    if let Some(p) = self.planes.get_mut(id) {
                        p.stopped = !p.stopped;
                    }
                }
                Craft::Bus(id) => {
                    if let Some(b) = self.buses.get_mut(id) {
                        b.stopped = !b.stopped;
                    }
                }
            },
            CraftAction::Remove => {
                match craft {
                    Craft::Boat(id) => self.boats.remove(id),
                    Craft::Plane(id) => self.planes.remove(id),
                    Craft::Bus(id) => self.buses.remove(id),
                }
                self.selected_craft = None;
                self.props_changed = true;
            }
            CraftAction::Find => {
                if let Some(p) = focus {
                    self.camera.focus.x = p.x;
                    self.camera.focus.z = p.y;
                    self.scene_dirty = true;
                }
            }
        }
    }

    /// The selected boat's or plane's window.
    pub(super) fn craft_windows(&mut self, ctx: &egui::Context) {
        let Some(craft) = self.selected_craft else { return };
        match craft {
            Craft::Boat(id) => {
                let Some(b) = self.boats.get(id) else {
                    self.selected_craft = None;
                    return;
                };
                let (status, load, stopped, orders) = (
                    b.status(&self.terminals),
                    (b.passengers, b.capacity),
                    b.stopped,
                    b.orders.clone(),
                );
                self.craft_window(
                    ctx,
                    craft,
                    format!("Ferry {}", id + 1),
                    status,
                    load,
                    stopped,
                    orders,
                    Kind::Port,
                );
            }
            Craft::Plane(id) => {
                let Some(p) = self.planes.get(id) else {
                    self.selected_craft = None;
                    return;
                };
                let (status, load, stopped, orders) = (
                    p.status(&self.terminals),
                    (p.passengers, p.capacity),
                    p.stopped,
                    p.orders.clone(),
                );
                self.craft_window(
                    ctx,
                    craft,
                    format!("Plane {}", id + 1),
                    status,
                    load,
                    stopped,
                    orders,
                    Kind::Airport,
                );
            }
            Craft::Bus(id) => {
                let Some(b) = self.buses.get(id) else {
                    self.selected_craft = None;
                    return;
                };
                let (status, load, stopped, orders) = (
                    b.status(&self.terminals),
                    (b.passengers, b.capacity),
                    b.stopped,
                    b.orders.clone(),
                );
                let title = if b.cargo == Cargo::Passengers {
                    format!("Bus {}", id + 1)
                } else {
                    format!("Truck {} ({})", id + 1, b.cargo.name().to_lowercase())
                };
                self.craft_window(ctx, craft, title, status, load, stopped, orders, Kind::BusStop);
            }
        }
    }

    /// The selected port's or airport's window: rename it, see who's waiting.
    pub(super) fn terminal_window(&mut self, ctx: &egui::Context) {
        let Some(id) = self.selected_terminal else { return };
        let calling = self.boats.list.iter().filter(|b| b.orders.list.contains(&id)).count()
            + self.buses.list.iter().filter(|b| b.orders.list.contains(&id)).count()
            + self.planes.list.iter().filter(|p| p.orders.list.contains(&id)).count();
        let Some(people) = self.terminals.get(id).map(|t| self.towns.population_near(&t.tiles())) else {
            self.selected_terminal = None;
            return;
        };
        let Some(t) = self.terminals.list.get_mut(&id) else {
            return;
        };
        let title = match t.kind() {
            Kind::Port => "Port",
            Kind::Airport => "Airport",
            Kind::BusStop => "Bus stop",
        };
        let mut open = true;
        ui::win95::window(ctx, title, &mut open, egui::pos2(420.0, 110.0), |ui| {
            ui.set_width(230.0);
            ui.horizontal(|ui| {
                ui.label("Name:");
                ui.text_edit_singleline(&mut t.name);
            });
            ui.label(format!(
                "{} passengers waiting, {people} people nearby",
                t.waiting as u32
            ));
            let (single, many) = match t.kind() {
                Kind::Port => ("boat", "boats"),
                Kind::Airport => ("plane", "planes"),
                Kind::BusStop => ("bus", "buses"),
            };
            ui.label(match calling {
                0 => format!("No {many} call here yet"),
                1 => format!("1 {single} calls here"),
                n => format!("{n} {many} call here"),
            });
        });
        if !open {
            self.selected_terminal = None;
        }
    }

    /// Demo mode: ferry lines between coastal towns and flights between the
    /// biggest cities. Returns (ports, boats, airports, planes).
    /// Demo: a bus line across each of the eight biggest towns, between two stops on its roads.
    pub(super) fn demo_buses(&mut self) -> usize {
        let mut towns: Vec<(u32, (i32, i32))> = self.towns.list.iter().map(|t| (self.towns.population(t.id), t.centre)).collect();
        towns.sort_by(|a, b| b.0.cmp(&a.0));
        let mut lines = 0;
        for &(_, (cx, cz)) in towns.iter().take(8) {
            let roads: Vec<(i32, i32)> = (cz - 12..=cz + 12)
                .flat_map(|z| (cx - 12..=cx + 12).map(move |x| (x, z)))
                .filter(|&(x, z)| {
                    self.roads.get(x, z) != 0 && self.terminals.at((x, z)).is_none() && self.rails.get(x, z) == 0
                })
                .collect();
            let Some(&a) = roads.iter().min_by_key(|&&(x, z)| (x - cx).abs() + (z - cz).abs()) else {
                continue;
            };
            // The far stop: as far from the first as the roads allow, and reachable.
            let mut far: Vec<(i32, i32)> = roads.clone();
            far.sort_by_key(|&(x, z)| -((x - a.0).abs() + (z - a.1).abs()));
            let Some(b) = far.into_iter().take(12).find(|&b| {
                let bits = self.roads.get(a.0, a.1);
                (0..8).map(|i| 1u8 << i).filter(|p| bits & p != 0).any(|piece| {
                    let seg = crate::vehicles::RoadSeg { tile: a, piece, forward: true };
                    crate::buses::route(&self.roads, seg, b).is_some()
                })
            }) else {
                continue;
            };
            let stops: Vec<TerminalId> = [a, b]
                .into_iter()
                .map(|tile| {
                    let id = self.terminals.build(crate::terminals::Site::BusStop { tile }, String::new());
                    self.name_terminal(id);
                    id
                })
                .collect();
            if let Some(id) = self.buses.spawn(&self.roads, a) {
                self.buses.set_orders(id, Orders::new(vec![stops[1], stops[0]]));
                lines += 1;
            }
        }
        self.static_changed = true;
        self.props_changed = true;
        lines
    }

    pub(super) fn demo_sea_and_air(&mut self) -> (usize, usize, usize, usize) {
        let mut towns: Vec<_> = self
            .towns
            .list
            .iter()
            .map(|t| (self.towns.population(t.id), t.centre))
            .collect();
        towns.sort_by(|a, b| b.0.cmp(&a.0));

        // Ports: the nearest spot on the coast to each town, within reach.
        self.sea();
        let mut ports: Vec<TerminalId> = Vec::new();
        for &(_, centre) in &towns {
            if ports.len() >= 8 {
                break;
            }
            let mut spots: Vec<(i32, i32)> = (-40..=40)
                .flat_map(|dz| (-40..=40).map(move |dx| (centre.0 + dx, centre.1 + dz)))
                .collect();
            spots.sort_by_key(|&(x, z)| (x - centre.0).pow(2) + (z - centre.1).pow(2));
            let site = spots.into_iter().find_map(|t| {
                let sea = self.sea.as_ref().unwrap();
                let plan = plan_port(&self.world, sea, &self.blockers(), t);
                plan.blocked.is_none().then_some(plan)
            });
            if let Some(plan) = site {
                let id = self.terminals.build(plan.site.unwrap(), String::new());
                self.towns.occupied.set(plan.tiles[0].0, plan.tiles[0].1, 2);
                self.name_terminal(id);
                ports.push(id);
            }
        }
        let boats_made = self.demo_lines(&ports, 4, 40.0, |app, from, to| {
            // One ferry at each end of a line, so both directions are sailed.
            for (a, b) in [(from, to), (to, from)] {
                let berth = app.terminals.get(a)?.berth()?;
                let id = app.boats.spawn(app.sea.as_ref()?, berth)?;
                app.boats.set_orders(id, Orders::new(vec![b, a]));
            }
            Some(())
        });

        // Airports: beside the biggest towns, levelled flat where needed.
        let mut airports: Vec<TerminalId> = Vec::new();
        'towns: for &(_, centre) in towns.iter().take(7) {
            let mut spots: Vec<(i32, i32)> = (-6..=6)
                .flat_map(|dz| (-6..=6).map(move |dx| (centre.0 + dx * 4, centre.1 + dz * 4)))
                .collect();
            spots.sort_by_key(|&(x, z)| (x - centre.0).pow(2) + (z - centre.1).pow(2));
            for (x, z) in spots {
                for along_x in [true, false] {
                    let sea = self.sea.as_ref().unwrap();
                    let plan = plan_airport(&self.world, sea, &self.blockers(), (x, z), along_x);
                    if plan.blocked.is_some() {
                        continue;
                    }
                    let terrain = edit::apply(&mut self.world, &plan.terraform);
                    self.demo_levelled.extend(terrain);
                    let id = self.terminals.build(plan.site.unwrap(), String::new());
                    for &t in &plan.tiles {
                        self.towns.occupied.set(t.0, t.1, 2);
                    }
                    self.name_terminal(id);
                    airports.push(id);
                    continue 'towns;
                }
            }
        }
        let planes_made = self.demo_lines(&airports, 5, 70.0, |app, from, to| {
            // One plane at each end of a route, so both directions are flown.
            let a = app.planes.spawn(&app.terminals, from)?;
            app.planes.set_orders(a, Orders::new(vec![to, from]));
            let b = app.planes.spawn(&app.terminals, to)?;
            app.planes.set_orders(b, Orders::new(vec![from, to]));
            Some(())
        });
        let boats_n = self.boats.list.len();
        let planes_n = self.planes.list.len();
        let _ = (boats_made, planes_made);
        (ports.len(), boats_n, airports.len(), planes_n)
    }

    /// Pairs up terminals by distance (nearest first, none closer than
    /// `min_dist`, each used at most twice) and runs `make` for up to `max`
    /// pairs. Returns how many were made.
    fn demo_lines(
        &mut self,
        ids: &[TerminalId],
        max: usize,
        min_dist: f32,
        make: impl Fn(&mut StarcityApp, TerminalId, TerminalId) -> Option<()>,
    ) -> usize {
        let mut pairs = Vec::new();
        for (i, &a) in ids.iter().enumerate() {
            for &b in &ids[i + 1..] {
                let (ca, cb) = (
                    self.terminals.get(a).unwrap().centre(),
                    self.terminals.get(b).unwrap().centre(),
                );
                let kind = self.terminals.get(a).unwrap().kind();
                // Ferries only join ports on the same body of water.
                let same_sea = kind != Kind::Port || {
                    let sea = self.sea.as_ref().unwrap();
                    let (ba, bb) = (
                        self.terminals.get(a).unwrap().berth().unwrap(),
                        self.terminals.get(b).unwrap().berth().unwrap(),
                    );
                    sea.component(ba.0, ba.1) == sea.component(bb.0, bb.1)
                };
                let d = ca.distance(cb);
                if same_sea && d >= min_dist {
                    pairs.push(((d * 10.0) as u32, a, b));
                }
            }
        }
        pairs.sort();
        let mut uses: HashMap<TerminalId, u32> = HashMap::new();
        let mut made = 0;
        for (_, a, b) in pairs {
            if made >= max {
                break;
            }
            if uses.get(&a).copied().unwrap_or(0) >= 2 || uses.get(&b).copied().unwrap_or(0) >= 2 {
                continue;
            }
            if make(self, a, b).is_some() {
                *uses.entry(a).or_default() += 1;
                *uses.entry(b).or_default() += 1;
                made += 1;
            }
        }
        made
    }
}
