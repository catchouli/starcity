//! The company inside the app: paying for things, income from passengers,
//! the headquarters, and the finance, headquarters and next-step windows
//! (GAME-DESIGN.md §2, §9.1, §9.2).

use super::StarcityApp;
use crate::cargo::Cargo;
use crate::company::{self, Assets, Company, Facts, Item};
use crate::hq;
use crate::towns::TownId;

/// What a contract joins: a town or an industry.
#[derive(Clone, Copy, Debug)]
pub enum Endpoint {
    Town(TownId),
    Industry(usize),
}
use crate::ui::win95;
use eframe::egui;
use glam::Vec3;

pub use crate::company::pounds as money;

impl StarcityApp {
    /// A fresh company with its garage in the company town. Keeps the sandbox setting.
    pub(super) fn new_company(&mut self) {
        let sandbox = self.company.sandbox;
        self.company = Company::new(self.year);
        self.company.sandbox = sandbox;
        self.player_roads = 0;
        self.hq = hq::choose_town(&self.towns)
            .and_then(|town| hq::place(&self.world, &mut self.towns, &self.roads, &self.rails, town));
        self.static_changed = true;
        self.contract_ends.clear();
        if !self.company.sandbox {
            self.offer_contracts();
        }
        self.refresh_assets();
    }

    /// The council's first contracts: join the company town to the nearest town along, then that
    /// town to the next, then haul a cargo between two industries. Seed money up front covers the
    /// building (GAME-DESIGN.md §9.3).
    fn offer_contracts(&mut self) {
        let Some(hq) = self.hq else { return };
        let towns = &self.towns.list;
        let dist = |a: (f32, f32), b: (f32, f32)| ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt();
        let centre = |t: &crate::towns::Town| (t.centre.0 as f32, t.centre.1 as f32);
        let nearest_to = |from: &crate::towns::Town, not: &[TownId]| {
            towns
                .iter()
                .filter(|t| !not.contains(&t.id) && (25.0..=140.0).contains(&dist(centre(from), centre(t))))
                .min_by(|a, b| dist(centre(from), centre(a)).total_cmp(&dist(centre(from), centre(b))))
                .map(|t| t.id)
        };
        let home = &towns[hq.town as usize];
        let mut ends = Vec::new();
        let mut contracts = Vec::new();
        if let Some(b) = nearest_to(home, &[hq.town]) {
            ends.push((Endpoint::Town(hq.town), Endpoint::Town(b)));
            contracts.push(company::Contract::link(
                &home.name,
                &towns[b as usize].name,
                20_000,
                15_000,
                50,
            ));
            if let Some(c) = nearest_to(&towns[b as usize], &[hq.town, b]) {
                ends.push((Endpoint::Town(b), Endpoint::Town(c)));
                contracts.push(company::Contract::link(
                    &towns[b as usize].name,
                    &towns[c as usize].name,
                    25_000,
                    25_000,
                    100,
                ));
            }
        }
        // Then freight: the nearest pair that makes and wants the same thing, from the company town.
        let near_name = |c: (f32, f32), label: &str| {
            let t = self
                .towns
                .nearest((c.0 as i32, c.1 as i32), 80)
                .map(|t| towns[t as usize].name.clone());
            match t {
                Some(t) => format!("{label} near {t}"),
                None => label.to_string(),
            }
        };
        let industries = &self.industries.list;
        let mut best: Option<(f32, usize, usize, Cargo)> = None;
        for (maker, user, cargo) in [
            (
                crate::industry::Kind::CoalMine,
                crate::industry::Kind::PowerStation,
                Cargo::Coal,
            ),
            (
                crate::industry::Kind::Forest,
                crate::industry::Kind::Sawmill,
                Cargo::Timber,
            ),
            (
                crate::industry::Kind::Farm,
                crate::industry::Kind::FoodPlant,
                Cargo::Grain,
            ),
            (
                crate::industry::Kind::IronOreMine,
                crate::industry::Kind::SteelMill,
                Cargo::IronOre,
            ),
        ] {
            for a in industries.iter().filter(|i| i.kind == maker) {
                for b in industries.iter().filter(|i| i.kind == user) {
                    let d = dist(a.centre(), b.centre());
                    // Close to home, and a worthwhile distance apart.
                    let home_d = dist(a.centre(), centre(home));
                    if (25.0..=140.0).contains(&d) && best.is_none_or(|(s, ..)| d + home_d < s) {
                        best = Some((d + home_d, a.id, b.id, cargo));
                    }
                }
            }
        }
        if let Some((_, a, b, cargo)) = best {
            let (ia, ib) = (&industries[a], &industries[b]);
            ends.push((Endpoint::Industry(a), Endpoint::Industry(b)));
            contracts.push(company::Contract::supply(
                cargo,
                &near_name(ia.centre(), ia.kind.name()),
                &near_name(ib.centre(), ib.kind.name()),
                20_000,
                30_000,
                100,
            ));
        }
        self.contract_ends = ends;
        if let Some(text) = self.company.offer_contracts(contracts) {
            let now = self.last_time;
            self.show_message(&text, now + 4.0);
        }
    }

    /// Which of the company's stations serve each end of a contract, and whether they are joined by track.
    fn link_facts(&self, a: Endpoint, b: Endpoint, cargo: Cargo) -> company::LinkFacts {
        let serving = |end: Endpoint| -> Vec<crate::stations::StationId> {
            self.stations
                .list
                .values()
                .filter(|s| match end {
                    Endpoint::Town(town) => {
                        let (x, z) = s.centre();
                        self.towns.nearest((x as i32, z as i32), 30) == Some(town)
                            && self.towns.population_near(&s.tiles) > 0
                    }
                    Endpoint::Industry(i) => self
                        .industries
                        .get(i)
                        .is_some_and(|ind| ind.near(&s.tiles, crate::industry::CATCHMENT)),
                })
                .map(|s| s.id)
                .collect()
        };
        let (sa, sb) = (serving(a), serving(b));
        let connected = sa.iter().any(|&x| {
            sb.iter().any(|&y| {
                crate::trains::reachable(&self.rails, &self.stations, x, y)
                    && crate::trains::reachable(&self.rails, &self.stations, y, x)
            })
        });
        company::LinkFacts {
            station_a: !sa.is_empty(),
            station_b: !sb.is_empty(),
            connected,
            train: self.trains.list.iter().any(|t| t.cargo == cargo),
        }
    }

    /// Where the headquarters could go if the cursor tile is the middle of its plot.
    fn relocation_site(&self, centre: (i32, i32)) -> Result<((i32, i32), TownId), &'static str> {
        let anchor = (centre.0 - hq::PLOT / 2, centre.1 - hq::PLOT / 2);
        if !hq::plot_fits(&self.world, &self.towns, &self.roads, &self.rails, anchor.0, anchor.1) {
            return Err("The headquarters needs flat, clear ground (no roads or track)");
        }
        let (cx, cz) = (anchor.0 + hq::PLOT / 2, anchor.1 + hq::PLOT / 2);
        match self.towns.nearest((cx, cz), 20) {
            Some(town) => Ok((anchor, town)),
            None => Err("Put it in a town: there's none near here"),
        }
    }

    /// Choosing a new place for the headquarters: a plot follows the cursor, green if it fits,
    /// and a click pays for the move.
    pub(super) fn relocation_tool(&mut self, response: &egui::Response, now: f64) {
        let cost = company::relocation_cost(self.company.hq_level);
        let site = self.hover_tile.map(|t| self.relocation_site(t));
        let highlight = self.hover_tile.map(|t| {
            let a = (t.0 - hq::PLOT / 2, t.1 - hq::PLOT / 2);
            crate::render::Highlight {
                a,
                b: (a.0 + hq::PLOT - 1, a.1 + hq::PLOT - 1),
                kind: if matches!(site, Some(Ok(_))) {
                    crate::render::HighlightKind::Cursor
                } else {
                    crate::render::HighlightKind::Bulldoze
                },
            }
        });
        if highlight != self.highlight {
            self.highlight = highlight;
            self.scene_dirty = true;
        }
        self.site_hint = Some(match &site {
            Some(Ok(_)) if self.company.can_afford(cost) => {
                format!(
                    "Move the headquarters here: {} (click, Esc to cancel)",
                    money(cost as i64)
                )
            }
            Some(Ok(_)) => format!("Moving costs {}: not enough money", money(cost as i64)),
            Some(Err(why)) => why.to_string(),
            None => "Click a spot in a town for the headquarters (Esc to cancel)".to_string(),
        });
        if !response.clicked_by(egui::PointerButton::Primary) {
            return;
        }
        let (Some(Ok((anchor, town))), Some(old)) = (site, self.hq) else {
            return;
        };
        if !self.pay(Item::Headquarters, cost) {
            return;
        }
        self.hq = Some(hq::relocate(&mut self.towns, &old, anchor, town));
        self.relocating = false;
        self.highlight = None;
        self.site_hint = None;
        self.static_changed = true;
        self.plan_epoch += 1;
        self.scene_dirty = true;
        let name = self.towns.list[town as usize].name.clone();
        self.show_message(&format!("Your headquarters has moved to {name}"), now);
    }

    /// Points the camera at the headquarters.
    pub(super) fn look_at_hq(&mut self) {
        if let Some(hq) = self.hq {
            let (x, z) = hq.centre();
            self.camera.focus = Vec3::new(x, self.world.height_at(x, z), z);
            self.scene_dirty = true;
        }
    }

    /// Counts what the company owns (cheap; once a second).
    pub(super) fn refresh_assets(&mut self) {
        self.assets = Assets {
            trains: self.trains.list.len() as u32,
            ferries: self.boats.list.len() as u32,
            planes: self.planes.list.len() as u32,
            road_pieces: self.player_roads,
            rail_pieces: if self.assets_version == self.track_version {
                self.assets.rail_pieces
            } else {
                self.rails.count()
            },
            station_tiles: self.stations.tile_count() as u32,
            ports: self.terminals.of_kind(crate::terminals::Kind::Port).count() as u32,
            airports: self.terminals.of_kind(crate::terminals::Kind::Airport).count() as u32,
        };
        self.assets_version = self.track_version;
        let town_served = self
            .stations
            .list
            .values()
            .map(|s| self.towns.population_near(&s.tiles))
            .chain(
                self.terminals
                    .list
                    .values()
                    .map(|t| self.towns.population_near(&t.tiles())),
            )
            .max()
            .unwrap_or(0);
        self.facts = Facts {
            rail_pieces: self.assets.rail_pieces,
            stations: self.stations.list.len() as u32,
            trains: self.assets.trains,
            town_served,
            vehicles: self.assets.trains + self.assets.ferries + self.assets.planes,
        };
        let active = self.company.contracts.iter().position(|c| !c.done);
        self.link =
            match active.and_then(|i| Some((self.contract_ends.get(i)?, self.company.contracts.get(i)?.cargo()))) {
                Some((&(a, b), cargo)) => self.link_facts(a, b, cargo),
                None => company::LinkFacts::default(),
            };
        let messages = self.company.update_contract(&self.link, &self.facts);
        let now = self.last_time;
        for m in messages {
            self.show_message(&m, now + 4.0);
        }
    }

    /// The rail route for the preview, searched again only when the drag or the map changes.
    pub(super) fn rail_plan_cached(&mut self, a: (i32, i32), b: (i32, i32)) -> crate::network::Plan {
        if let Some((key, epoch, plan)) = &self.route_cache {
            if *key == (a, b) && *epoch == self.plan_epoch {
                return plan.clone();
            }
        }
        let plan = crate::rails::plan_route(
            &self.world,
            &self.rails,
            &self.roads,
            &self.towns.occupied,
            &self.signals,
            a,
            b,
        );
        self.route_cache = Some(((a, b), self.plan_epoch, plan.clone()));
        plan
    }

    /// Charges for something; says so if the money isn't there.
    pub(super) fn pay(&mut self, item: Item, cost: u32) -> bool {
        if self.company.spend(item, cost) {
            return true;
        }
        let now = self.last_time;
        self.show_message(&format!("Not enough money: {} needed", money(cost as i64)), now);
        false
    }

    /// Whether another vehicle fits under the headquarters' limit and the
    /// money is there. Says why not.
    pub(super) fn can_add_vehicle(&mut self, price: u32) -> bool {
        let now = self.last_time;
        if self.facts.vehicles >= self.company.vehicle_limit() {
            let limit = self.company.vehicle_limit();
            self.show_message(
                &format!("Your headquarters only has room for {limit} vehicles. Upgrade it to run more."),
                now,
            );
            return false;
        }
        if !self.company.can_afford(price) {
            self.show_message(&format!("Not enough money: {} needed", money(price as i64)), now);
            return false;
        }
        true
    }

    /// Passengers got off: pay the fare for the trip from where they boarded.
    pub(super) fn pay_fare(
        &mut self,
        cargo: crate::cargo::Cargo,
        units: u32,
        boarded: Option<(f32, f32, f64)>,
        at: (f32, f32),
    ) {
        if units == 0 {
            return;
        }
        self.company.delivered[cargo.index()] += units;
        let Some((x, z, since)) = boarded else { return };
        let tiles = ((at.0 - x).powi(2) + (at.1 - z).powi(2)).sqrt();
        let seconds = (self.clock.days() - since).max(0.0) as f32;
        let item = if cargo == crate::cargo::Cargo::Passengers {
            Item::Passengers
        } else {
            Item::Freight
        };
        self.company
            .earn(item, crate::cargo::fare(cargo, units, tiles, seconds));
    }

    /// Once per sim step: the monthly books.
    pub(super) fn tick_company(&mut self, dt: f32) {
        let messages = self.company.tick(dt, self.year, &self.assets);
        let now = self.last_time;
        for m in messages {
            self.show_message(&m, now);
        }
    }

    pub(super) fn finance_window(&mut self, ctx: &egui::Context) {
        let mut open = self.ui.show_finance;
        let value = self.company.value(&self.assets);
        let (mut borrow, mut repay) = (false, false);
        let c = &self.company;
        win95::window(ctx, "Finances", &mut open, egui::pos2(200.0, 110.0), |ui| {
            ui.set_width(380.0);
            egui::Grid::new("balance").num_columns(2).show(ui, |ui| {
                ui.label("Cash");
                ui.label(egui::RichText::new(money(c.cash)).strong());
                ui.end_row();
                ui.label("Loan");
                ui.label(money(c.loan));
                ui.end_row();
                ui.label("Company value");
                ui.label(money(value));
                ui.end_row();
            });
            ui.horizontal(|ui| {
                let limit = c.loan_limit();
                borrow = ui
                    .add_enabled(
                        c.loan + company::LOAN_STEP <= limit,
                        egui::Button::new("Borrow £10,000"),
                    )
                    .clicked();
                repay = ui
                    .add_enabled(c.loan >= company::LOAN_STEP, egui::Button::new("Repay £10,000"))
                    .clicked();
                ui.label(if limit == 0 {
                    "Build a Depot office to open a loan".to_string()
                } else {
                    format!("up to {}", money(limit))
                });
            });
            ui.separator();
            // This year and the three before, newest first.
            let mut years: Vec<(i32, company::Ledger)> = vec![(c.year, c.ledger)];
            years.extend(c.history.iter().copied());
            egui::Grid::new("books")
                .num_columns(years.len() + 1)
                .striped(true)
                .show(ui, |ui| {
                    ui.label("");
                    for (y, _) in &years {
                        ui.label(egui::RichText::new(y.to_string()).strong());
                    }
                    ui.end_row();
                    for item in Item::ALL {
                        ui.label(item.name());
                        for (_, l) in &years {
                            ui.label(money(l.get(item)));
                        }
                        ui.end_row();
                    }
                    ui.label(egui::RichText::new("Profit").strong());
                    for (_, l) in &years {
                        ui.label(egui::RichText::new(money(l.income() - l.expenses())).strong());
                    }
                    ui.end_row();
                });
        });
        self.ui.show_finance = open;
        if borrow {
            self.company.borrow();
        }
        if repay {
            self.company.repay();
        }
    }

    pub(super) fn hq_window(&mut self, ctx: &egui::Context) {
        let Some(hq) = self.hq else {
            self.ui.show_hq = false;
            return;
        };
        let mut open = self.ui.show_hq;
        let level = self.company.hq_level;
        let reqs = self.company.requirements(&self.assets, &self.facts);
        let can_upgrade = self.company.can_upgrade(&self.assets, &self.facts);
        let town = self
            .towns
            .list
            .get(hq.town as usize)
            .map(|t| t.name.clone())
            .unwrap_or_default();
        let (mut upgrade, mut look, mut relocate) = (false, false, false);
        let sandbox = self.company.sandbox;
        win95::window(ctx, "Headquarters", &mut open, egui::pos2(220.0, 130.0), |ui| {
            ui.set_width(330.0);
            let current = company::level(level);
            ui.label(egui::RichText::new(format!("{} — {town}", current.name)).strong());
            ui.label(format!("Level {level}. Unlocks: {}.", current.unlocks.to_lowercase()));
            ui.label(format!(
                "Room for {} vehicles; loan up to {}.",
                self.company.vehicle_limit(),
                money(self.company.loan_limit())
            ));
            ui.separator();
            match company::LEVELS.get(level as usize + 1) {
                Some(next) => {
                    ui.label(egui::RichText::new(format!("Next: {}", next.name)).strong());
                    ui.label(format!("Unlocks: {}.", next.unlocks.to_lowercase()));
                    for r in &reqs {
                        ui.label(format!("{} {}", if r.met { "[x]" } else { "[ ]" }, r.text));
                    }
                    upgrade = ui
                        .add_enabled(
                            can_upgrade,
                            egui::Button::new(format!("Build {} ({})", next.name, money(next.cost as i64))),
                        )
                        .clicked();
                }
                None => {
                    ui.label(company::LATER_LEVELS);
                }
            }
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                look = ui.button("Look at it").clicked();
                if level >= company::RELOCATE_FROM || sandbox {
                    relocate = ui
                        .button(format!("Move it… ({})", money(company::relocation_cost(level) as i64)))
                        .on_hover_text("Choose another town for your headquarters. It costs money, so think about where you want to focus.")
                        .clicked();
                } else {
                    ui.weak("It can be moved from the Regional office.");
                }
            });
        });
        self.ui.show_hq = open;
        if relocate {
            self.relocating = true;
            self.ui.tool = crate::ui::Tool::Pointer;
            self.ui.show_hq = false;
        }
        if look {
            self.look_at_hq();
        }
        if upgrade && self.company.upgrade(&self.assets, &self.facts) {
            self.static_changed = true;
            let now = self.last_time;
            let name = company::level(self.company.hq_level).name;
            self.show_message(&format!("Your headquarters is now a {name}"), now);
        }
    }

    /// The panel that always has one thing for the player to do.
    pub(super) fn next_step_window(&mut self, ctx: &egui::Context) {
        let step = self.company.next_step(&self.assets, &self.facts, &self.link);
        let hint = self.company.stuck_hint(&self.facts);
        let contract = self
            .company
            .contract()
            .map(|c| (c.from.clone(), c.to.clone(), c.seed, c.reward));
        let reqs = self.company.contract_requirements(&self.link, &self.facts);
        let mut look = false;
        win95::window(
            ctx,
            "Next step",
            &mut self.ui.show_next_step,
            egui::pos2(12.0, 98.0),
            |ui| {
                ui.set_width(250.0);
                ui.label(egui::RichText::new(&step.title).strong());
                ui.label(&step.detail);
                if let Some((from, to, seed, reward)) = &contract {
                    ui.add_space(2.0);
                    ui.label(egui::RichText::new(format!("Council contract: {from} - {to}")).strong());
                    ui.label(format!(
                        "{} paid up front, {} on completion",
                        money(*seed as i64),
                        money(*reward as i64)
                    ));
                    for r in &reqs {
                        ui.label(format!("{} {}", if r.met { "[x]" } else { "[ ]" }, r.text));
                    }
                }
                if let Some(h) = hint {
                    ui.add_space(2.0);
                    ui.label(egui::RichText::new(h).color(egui::Color32::from_rgb(0x90, 0x30, 0x10)));
                }
                if step.hq {
                    look = ui.button("Show my headquarters").clicked();
                }
            },
        );
        if look {
            self.look_at_hq();
            self.ui.show_hq = true;
        }
    }

    /// Returns true when the player asks to start again.
    pub(super) fn bankrupt_window(&mut self, ctx: &egui::Context) -> bool {
        if !self.company.bankrupt {
            return false;
        }
        let mut open = true;
        let mut restart = false;
        win95::window(ctx, "Bankrupt", &mut open, egui::pos2(260.0, 160.0), |ui| {
            ui.set_width(280.0);
            ui.label("Your company has been in the red for a year and the bank has closed it down.");
            ui.label("Sell what you can next time, and keep an eye on the finance window.");
            restart = ui.button("Start a new company").clicked();
        });
        restart
    }
}
