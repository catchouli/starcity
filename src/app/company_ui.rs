//! The company inside the app: paying for things, income from passengers,
//! the headquarters, and the finance, headquarters and next-step windows
//! (GAME-DESIGN.md §2, §9.1, §9.2).

use super::StarcityApp;
use crate::cargo::Cargo;
use crate::company::{self, Assets, Company, Facts, Item};
use crate::hq;
use crate::towns::TownId;

/// A place the headquarters could move to.
struct RelocationSite {
    anchor: (i32, i32),
    town: TownId,
    terraform: Vec<((i32, i32), u16)>,
    steps: u32,
}

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
            for a in industries.iter().filter(|i| i.kind == maker && !i.closed) {
                for b in industries.iter().filter(|i| i.kind == user && !i.closed) {
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

    /// Where the headquarters could go if the cursor tile is the middle of its plot, with the ground work it needs.
    fn relocation_site(&self, centre: (i32, i32)) -> Result<RelocationSite, &'static str> {
        let anchor = (centre.0 - hq::PLOT / 2, centre.1 - hq::PLOT / 2);
        let Some(terraform) = hq::plot_levelling(&self.world, &self.towns, &self.roads, &self.rails, anchor) else {
            return Err("No room for it here: roads, track, water or steep ground are in the way");
        };
        let (cx, cz) = (anchor.0 + hq::PLOT / 2, anchor.1 + hq::PLOT / 2);
        let Some(town) = self.towns.nearest((cx, cz), 20) else {
            return Err("Put it in a town: there's none near here");
        };
        let steps = {
            // Block-steps moved, for the price.
            let mut overlay = crate::terrain::edit::Corners::with_limits(&self.world, 3, 120);
            for &(c, h) in &terraform {
                overlay.set_for_pricing(c, h as i32);
            }
            overlay.steps()
        };
        Ok(RelocationSite {
            anchor,
            town,
            terraform,
            steps,
        })
    }

    /// Choosing a new place for the headquarters: a plot follows the cursor, green if it fits (the
    /// ground is levelled for a fee), and a click pays for the move. The price, levelling included,
    /// follows the cursor.
    pub(super) fn relocation_tool(&mut self, response: &egui::Response, now: f64) -> crate::sim::Changes {
        let base = company::relocation_cost(self.company.hq_level);
        let site = self.hover_tile.map(|t| self.relocation_site(t));
        let cost = match &site {
            Some(Ok(s)) => base + s.steps * company::TERRAFORM_STEP,
            _ => base,
        };
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
        let affordable = self.company.can_afford(cost);
        self.cursor_label = match &site {
            Some(Ok(s)) if s.steps > 0 => Some((
                format!(
                    "{} ({} levelling)",
                    money(cost as i64),
                    money((s.steps * company::TERRAFORM_STEP) as i64)
                ),
                affordable,
            )),
            Some(Ok(_)) => Some((money(cost as i64), affordable)),
            _ => None,
        };
        self.site_hint = Some(match &site {
            Some(Ok(_)) if affordable => {
                format!(
                    "Move the headquarters here: {} (click, Esc to cancel)",
                    money(cost as i64)
                )
            }
            Some(Ok(_)) => format!("Moving costs {}: not enough money", money(cost as i64)),
            Some(Err(why)) => why.to_string(),
            None => "Click a spot in a town for the headquarters (Esc to cancel)".to_string(),
        });
        let mut changes = crate::sim::Changes::default();
        if !response.clicked_by(egui::PointerButton::Primary) {
            return changes;
        }
        let (Some(Ok(site)), Some(old)) = (site, self.hq) else {
            return changes;
        };
        if !self.pay(Item::Headquarters, cost) {
            return changes;
        }
        // Level the ground, then move the building onto it.
        changes.terrain = crate::terrain::edit::apply(&mut self.world, &site.terraform);
        self.hq = Some(hq::relocate(&mut self.towns, &old, site.anchor, site.town));
        self.relocating = false;
        self.highlight = None;
        self.site_hint = None;
        self.static_changed = true;
        self.props_changed = true;
        self.plan_epoch += 1;
        self.scene_dirty = true;
        let name = self.towns.list[site.town as usize].name.clone();
        self.show_message(&format!("Your headquarters has moved to {name}"), now);
        changes
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
            train_value: self.trains.list.iter().map(|t| t.price).sum(),
            train_running: self.trains.list.iter().map(|t| t.running).sum(),
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
            compute_sites: self
                .industries
                .list
                .iter()
                .filter(|i| !i.closed && i.kind.compute_scale() > 0.0)
                .count() as u32,
            compute_total: self.compute.total,
        };
        let active = self.company.contracts.iter().position(|c| !c.done);
        self.link =
            match active.and_then(|i| Some((self.contract_ends.get(i)?, self.company.contracts.get(i)?.cargo()))) {
                Some((&(a, b), cargo)) => self.link_facts(a, b, cargo),
                None => company::LinkFacts::default(),
            };
        let mut messages = self.company.update_contract(&self.link, &self.facts);
        messages.extend(self.company.update_goals(self.year, &self.assets, &self.facts));
        let now = self.last_time;
        for m in messages {
            self.show_message(&m, now + 4.0);
        }
    }

    /// A town the company serves that has grown to want food or goods and hasn't got it: the Assistant says so.
    fn hungry_town_hint(&self) -> Option<String> {
        if self.company.sandbox || self.company.hq_level < 1 {
            return None;
        }
        self.towns
            .list
            .iter()
            .filter(|t| t.pop > crate::towns::WANTS_FOOD && t.served > 1.0)
            .find_map(|t| {
                if t.food <= 0.0 {
                    Some(format!("{} has grown to {} people and wants food. Haul some from a food plant to a station in town.", t.name, t.pop))
                } else if t.pop > crate::towns::WANTS_GOODS && t.goods <= 0.0 {
                    Some(format!("{} wants goods now. A factory's goods delivered to a station in town will help it grow.", t.name))
                } else {
                    None
                }
            })
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

    /// What the dragged-out plan would cost: pieces at the network's price plus any levelling.
    pub(super) fn plan_cost(&self, plan: &crate::network::Plan) -> u32 {
        let unit = match (self.plan_network, self.ui.tool) {
            (_, crate::ui::Tool::Station) => company::STATION_TILE,
            (super::Network::Road, _) => company::ROAD_PIECE,
            (super::Network::Rail, _) => company::RAIL_PIECE,
        };
        plan.pieces.len() as u32 * unit + company::terraform_cost(plan)
    }

    /// Sets the price shown beside the cursor.
    pub(super) fn show_cost(&mut self, cost: u32, levelling: u32) {
        let text = if levelling > 0 {
            format!("{} ({} levelling)", money(cost as i64), money(levelling as i64))
        } else {
            money(cost as i64)
        };
        self.cursor_label = Some((text, self.company.can_afford(cost)));
    }

    /// The price of the current action, drawn next to the mouse: green-ish paper if you can
    /// afford it, red text if you can't.
    pub(super) fn draw_cursor_label(&self, ctx: &egui::Context) {
        let Some((text, ok)) = &self.cursor_label else { return };
        let Some(pos) = ctx.pointer_latest_pos() else { return };
        let frame = egui::Frame::NONE
            .fill(egui::Color32::from_rgb(0xff, 0xff, 0xe1))
            .stroke(egui::Stroke::new(1.0, egui::Color32::BLACK))
            .inner_margin(egui::Margin::symmetric(5, 2));
        egui::Area::new(egui::Id::new("cursor_cost"))
            .fixed_pos(pos + egui::vec2(18.0, 20.0))
            .order(egui::Order::Tooltip)
            .interactable(false)
            .show(ctx, |ui| {
                frame.show(ui, |ui| {
                    let colour = if *ok {
                        egui::Color32::BLACK
                    } else {
                        win95::bad()
                    };
                    ui.label(
                        egui::RichText::new(if *ok {
                            text.clone()
                        } else {
                            format!("{text}: can't afford")
                        })
                        .color(colour),
                    );
                });
            });
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
    ) -> u32 {
        if units == 0 {
            return 0;
        }
        self.company.delivered[cargo.index()] += units;
        let Some((x, z, since)) = boarded else { return 0 };
        let tiles = ((at.0 - x).powi(2) + (at.1 - z).powi(2)).sqrt();
        let seconds = (self.clock.days() - since).max(0.0) as f32;
        let item = if cargo == crate::cargo::Cargo::Passengers {
            Item::Passengers
        } else {
            Item::Freight
        };
        let fare = crate::cargo::fare(cargo, units, tiles, seconds);
        let world = if cargo == crate::cargo::Cargo::Passengers {
            self.company.world.passengers
        } else {
            self.company.world.freight
        };
        let fare = (fare as f32 * (1.0 + self.company.effects.fare_bonus) * world).round() as u32;
        self.company.earn(item, fare);
        fare
    }

    /// Once per sim step: the monthly books.
    pub(super) fn tick_company(&mut self, dt: f32) {
        let date = self.clock.year_f64() as f32;
        let seed = self.params.seed;
        if date < self.last_event_date {
            // The date was moved back.
            self.last_event_date = date;
        }
        for i in crate::events::started_between(self.last_event_date, date, seed) {
            self.news.push(i);
        }
        self.last_event_date = date;
        let world = crate::events::active(date, seed);
        self.company.world = world;
        self.towns.growth_mult = world.growth;
        let messages = self.company.tick(dt, self.year, &self.assets);
        let now = self.last_time;
        for m in messages {
            self.show_message(&m, now);
        }
    }

    pub(super) fn finance_window(&mut self, ctx: &egui::Context) {
        let mut open = self.ui.show_finance;
        let value = self.company.value(&self.assets);
        let wellbeing = self.wellbeing;
        let world_now = crate::events::active_titles(self.clock.year_f64() as f32, self.params.seed);
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
            ui.label(format!("Wellbeing {:.0} / 100 (Company → Charter and ledgers)", wellbeing));
            for title in &world_now {
                ui.label(egui::RichText::new(format!("In the news: {title}")).color(win95::warn()));
            }
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
                        win95::check_row(ui, r.met, &r.text);
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

    /// The Assistant: always has one thing for the player to do. Docked at the bottom left of the view
    /// so it never covers the toolbar or the middle of the map; a one-line pill when collapsed, the
    /// whole checklist when open. It starts as a tutorial and grows into the main way the game talks to
    /// the player (GAME-DESIGN.md §9.2).
    pub(super) fn assistant_panel(&mut self, ctx: &egui::Context, area: egui::Rect) {
        let step = self.company.next_step(&self.assets, &self.facts, &self.link);
        if step.title != self.last_step_title {
            self.last_step_title = step.title.clone();
            self.assistant_attention = true;
        }
        if !self.ui.show_next_step {
            return;
        }
        if self.ui.assistant_expanded {
            self.assistant_attention = false;
        }
        let hint = self
            .company
            .stuck_hint(&self.facts)
            .map(str::to_string)
            .or_else(|| self.hungry_town_hint());
        let goals: Vec<(String, bool)> = self
            .company
            .goals
            .iter()
            .map(|g| (self.company.goal_progress(g, &self.assets, &self.facts).0, g.done))
            .collect();
        let contract = self
            .company
            .contract()
            .map(|c| (c.from.clone(), c.to.clone(), c.seed, c.reward));
        let reqs = self.company.contract_requirements(&self.link, &self.facts);
        let expanded = self.ui.assistant_expanded;
        let (mut toggle, mut hide, mut look) = (false, false, false);
        let frame = egui::Frame::NONE.fill(win95::face()).inner_margin(egui::Margin::same(6));
        let response = egui::Area::new(egui::Id::new("assistant_panel"))
            .pivot(egui::Align2::LEFT_BOTTOM)
            .fixed_pos(egui::pos2(area.left() + 12.0, area.bottom() - 12.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                frame.show(ui, |ui| {
                    ui.set_width(if expanded { 290.0 } else { 330.0 });
                    ui.horizontal(|ui| {
                        let (rect, _) = ui.allocate_exact_size(egui::vec2(26.0, 26.0), egui::Sense::hover());
                        crate::ui::icon_assistant(ui.painter(), rect);
                        ui.label(egui::RichText::new(company::assistant_name(self.year)).strong());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .small_button("x")
                                .on_hover_text("Hide (the toolbar button brings it back)")
                                .clicked()
                            {
                                hide = true;
                            }
                            if ui
                                .small_button(if expanded { "-" } else { "+" })
                                .on_hover_text(if expanded {
                                    "Shrink to one line"
                                } else {
                                    "Show everything"
                                })
                                .clicked()
                            {
                                toggle = true;
                            }
                        });
                    });
                    if !expanded {
                        ui.add(egui::Label::new(egui::RichText::new(&step.title).strong()).truncate());
                        return;
                    }
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
                            win95::check_row(ui, r.met, &r.text);
                        }
                    }
                    if !goals.is_empty() {
                        ui.add_space(4.0);
                        ui.label(egui::RichText::new("Goals this year").strong());
                        for (text, done) in &goals {
                            win95::check_row(ui, *done, text);
                        }
                    }
                    if let Some(h) = hint {
                        ui.add_space(2.0);
                        ui.label(egui::RichText::new(h).color(win95::warn()));
                    }
                    if step.hq {
                        look = ui.button("Show my headquarters").clicked();
                    }
                })
            });
        win95::bevel(
            &ctx.layer_painter(response.response.layer_id),
            response.response.rect,
            true,
        );
        if toggle {
            self.ui.assistant_expanded = !self.ui.assistant_expanded;
        }
        if hide {
            self.ui.show_next_step = false;
        }
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
