//! Game UI: Win95-style menu bar, toolbar and status bar.

pub mod win95;

use crate::clock::Speed;
use eframe::egui::{self, Color32, Painter, Rect, Stroke, Ui, pos2, vec2};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tool {
    #[default]
    Pointer,
    Road,
    RemoveRoad,
    Rail,
    Signal,
    Train,
    Station,
    RemoveRail,
    Bulldoze,
    Port,
    Boat,
    Airport,
    Plane,
    BusStop,
    Bus,
    Expressway,
}

impl Tool {
    pub fn name(self) -> &'static str {
        match self {
            Tool::Pointer => "Pointer",
            Tool::Road => "Build road",
            Tool::RemoveRoad => "Remove road",
            Tool::Rail => "Build railway",
            Tool::Signal => "Path signal",
            Tool::Train => "Place train",
            Tool::Station => "Build station",
            Tool::RemoveRail => "Remove railway",
            Tool::Bulldoze => "Bulldoze",
            Tool::Port => "Build port",
            Tool::Boat => "Place ferry",
            Tool::Airport => "Build airport",
            Tool::Plane => "Place plane",
            Tool::BusStop => "Build bus stop",
            Tool::Bus => "Place bus",
            Tool::Expressway => "Build hyper expressway",
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            Tool::Pointer => "Drag to pan, right-drag to rotate, scroll to zoom. Click a train or station to open it.",
            Tool::Road => "Drag along an axis or at 45° to draw road. Red means it can't be built there.",
            Tool::RemoveRoad => "Click or drag a rectangle to remove road",
            Tool::Rail => {
                "Drag from one point to another: it finds a way round obstacles and levels the ground (for a fee). Red means no way through."
            }
            Tool::Signal => {
                "Click track to place a path signal facing you; click again to turn it round, right-click to remove"
            }
            Tool::Train => "Click plain track to place a train heading away from you. Space pauses trains.",
            Tool::Station => {
                "Drag along straight track or flat ground to build platforms. Click trains to give them orders."
            }
            Tool::RemoveRail => "Click or drag a rectangle to remove track",
            Tool::Bulldoze => "Click or drag a rectangle to clear it to bare ground",
            Tool::Port => "Click flat coastal land to build a port. Ferries tie up in the water beside it.",
            Tool::Boat => "Click water to place a ferry, then click it to give it ports to visit.",
            Tool::Airport => "Click to build an airport (O turns the runway). It levels the ground for a price.",
            Tool::Plane => "Click an airport to park a plane there, then click the plane to give it airports to visit.",
            Tool::BusStop => "Click a road to put a bus stop on it. People within four tiles wait there.",
            Tool::Bus => "Click a bus stop to put a bus there, then click the bus to give it stops to visit.",
            Tool::Expressway => {
                "Drag to lay an elevated autonomous expressway (from 2035): straight over hills, towns and lines. Cities it reaches grow much faster."
            }
        }
    }

    fn icon(self, p: &Painter, r: Rect) {
        match self {
            Tool::Pointer => icon_pointer(p, r),
            Tool::Road => icon_road(p, r),
            Tool::RemoveRoad => {
                icon_road(p, r);
                icon_cross(p, r);
            }
            Tool::Rail => icon_rail(p, r),
            Tool::Signal => icon_signal(p, r),
            Tool::Train => icon_train(p, r),
            Tool::Station => icon_station(p, r),
            Tool::RemoveRail => {
                icon_rail(p, r);
                icon_cross(p, r);
            }
            Tool::Bulldoze => icon_bulldozer(p, r),
            Tool::Port => icon_port(p, r),
            Tool::Boat => icon_boat(p, r),
            Tool::Airport => icon_airport(p, r),
            Tool::Plane => icon_plane(p, r),
            Tool::BusStop => icon_bus_stop(p, r),
            Tool::Bus => icon_bus(p, r),
            Tool::Expressway => icon_expressway(p, r),
        }
    }
}

/// Things the menus and toolbar can change.
pub struct UiState {
    pub tool: Tool,
    pub wireframe: bool,
    pub grid: bool,
    pub ortho: bool,
    pub clouds: bool,
    /// Whether the light follows the time of day.
    pub day_night: bool,
    pub speed: Speed,
    pub show_debug: bool,
    pub show_about: bool,
    pub show_finance: bool,
    pub show_hq: bool,
    pub show_compute: bool,
    pub show_fund: bool,
    pub show_charter: bool,
    /// Force a look instead of the one the year brings.
    pub look_override: Option<win95::Look>,
    /// The year, so looks from later decades stay locked.
    pub year: i32,
    /// The Next Step panel (GAME-DESIGN.md §9.2); on by default.
    pub show_next_step: bool,
    /// The Assistant shows its whole checklist, not just a one-line pill.
    pub assistant_expanded: bool,
    /// Set for one frame when the player asks to look at the headquarters.
    pub go_hq: bool,
    /// Set for one frame when the player asks to centre the view on the island.
    pub go_home: bool,
}

#[derive(Default)]
pub struct UiActions {
    pub new_map: bool,
}

/// The Compute meter shown at the right of the menu bar: running total, rate, history (log axis), tooltip.
pub struct Meter<'a> {
    pub total: &'a str,
    pub rate: &'a str,
    pub tip: &'a str,
    pub spark: &'a [f32],
}

pub fn menu_bar(ui: &mut Ui, s: &mut UiState, meter: &Meter) -> UiActions {
    let mut actions = UiActions::default();
    egui::MenuBar::new().ui(ui, |ui| {
        ui.menu_button("File", |ui| {
            if ui.button("New map").clicked() {
                actions.new_map = true;
            }
            #[cfg(not(target_arch = "wasm32"))]
            {
                ui.separator();
                if ui.button("Exit").clicked() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        });
        ui.menu_button("View", |ui| {
            ui.checkbox(&mut s.wireframe, "Wireframe\tF1");
            ui.checkbox(&mut s.grid, "Tile grid\tG");
            ui.checkbox(&mut s.ortho, "Orthographic\tR");
            ui.checkbox(&mut s.clouds, "Clouds\tC");
            ui.checkbox(&mut s.day_night, "Day/night cycle\tN");
            ui.separator();
            if ui.button("Centre on island\tHome").clicked() {
                s.go_home = true;
                ui.close();
            }
            ui.separator();
            ui.checkbox(&mut s.show_debug, "Debug window\tF3");
            ui.separator();
            ui.label("Interface style");
            if ui.radio(s.look_override.is_none(), "Follows the years").clicked() {
                s.look_override = None;
            }
            for look in win95::Look::ALL {
                let unlocked = look.from_year() <= s.year;
                let label = if unlocked {
                    look.name().to_string()
                } else {
                    format!("{} (from {})", look.name(), look.from_year())
                };
                if ui
                    .add_enabled(unlocked, egui::RadioButton::new(s.look_override == Some(look), label))
                    .clicked()
                {
                    s.look_override = Some(look);
                }
            }
        });
        ui.menu_button("Company", |ui| {
            if ui.button("Finances").clicked() {
                s.show_finance = true;
                ui.close();
            }
            if ui.button("Headquarters").clicked() {
                s.show_hq = true;
                ui.close();
            }
            if ui.button("Compute").clicked() {
                s.show_compute = true;
                ui.close();
            }
            if ui.button("Fund an industry").clicked() {
                s.show_fund = true;
                ui.close();
            }
            if ui.button("Charter and ledgers").clicked() {
                s.show_charter = true;
                ui.close();
            }
        });
        ui.menu_button("Help", |ui| {
            ui.checkbox(&mut s.show_next_step, "Next step panel");
            if ui.button("About Starcity").clicked() {
                s.show_about = true;
            }
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if win95::compute_pane(ui, 270.0, meter.total, meter.rate, meter.spark)
                .on_hover_text(meter.tip)
                .clicked()
            {
                s.show_compute = true;
            }
        });
    });
    actions
}

/// What the toolbar needs to know about the company.
pub struct ToolbarInfo<'a> {
    pub clock: &'a str,
    pub daylight: f32,
    pub cash: &'a str,
    pub in_the_red: bool,
    /// Which tools the headquarters has unlocked.
    pub unlocked: &'a dyn Fn(Tool) -> bool,
    /// The tool the next step points at.
    pub suggested: Option<Tool>,
    /// The Assistant has something new to say: its button glows.
    pub assistant_attention: bool,
}

pub fn toolbar(ui: &mut Ui, s: &mut UiState, info: &ToolbarInfo) {
    const SIZE: f32 = 30.0;
    let groups: [(&str, &[Tool]); 6] = [
        ("", &[Tool::Pointer]),
        ("Land", &[Tool::Bulldoze]),
        ("Roads", &[Tool::Road, Tool::RemoveRoad, Tool::BusStop, Tool::Bus]),
        (
            "Rail",
            &[Tool::Rail, Tool::Signal, Tool::Station, Tool::Train, Tool::RemoveRail],
        ),
        ("Sea & air", &[Tool::Port, Tool::Boat, Tool::Airport, Tool::Plane]),
        ("Future", &[Tool::Expressway]),
    ];
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        let home = win95::tool_button(ui, SIZE, false, icon_island);
        if home.clicked() {
            s.go_home = true;
        }
        home.on_hover_text("Centre on island (Home)");
        let hq = win95::tool_button(ui, SIZE, false, icon_hq);
        if hq.clicked() {
            s.go_hq = true;
        }
        hq.on_hover_text("Centre on your headquarters");
        let assistant = win95::tool_button_ex(
            ui,
            SIZE,
            s.show_next_step && s.assistant_expanded,
            true,
            info.assistant_attention && !(s.show_next_step && s.assistant_expanded),
            icon_assistant,
        );
        if assistant.clicked() {
            if s.show_next_step {
                s.assistant_expanded = !s.assistant_expanded;
            } else {
                s.show_next_step = true;
                s.assistant_expanded = true;
            }
        }
        assistant.on_hover_text("Your Assistant: shows what to do next");
        win95::separator(ui, SIZE);
        for (i, (label, tools)) in groups.iter().enumerate() {
            if i > 0 {
                win95::separator(ui, SIZE);
            }
            // Group names only where there's room: on a laptop-sized window the buttons come first.
            if !label.is_empty() && ui.ctx().content_rect().width() > 1500.0 {
                ui.label(*label);
            }
            for &tool in *tools {
                let selected = s.tool == tool;
                let enabled = (info.unlocked)(tool);
                let suggested = info.suggested == Some(tool) && !selected;
                let response = win95::tool_button_ex(ui, SIZE, selected, enabled, suggested, |p, r| tool.icon(p, r));
                if response.clicked() && enabled {
                    s.tool = if selected { Tool::Pointer } else { tool };
                }
                if enabled {
                    response.on_hover_text(tool.name());
                } else {
                    response.on_hover_text(format!("{}: needs a Regional office", tool.name()));
                }
            }
        }
        // Game speed, at the right-hand end.
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            for speed in [Speed::X8, Speed::X4, Speed::X2, Speed::X1, Speed::Paused] {
                let selected = s.speed == speed;
                let response = win95::tool_button(ui, SIZE, selected, |p, r| icon_speed(p, r, speed));
                if response.clicked() {
                    s.speed = speed;
                }
                response.on_hover_text(match speed {
                    Speed::Paused => "Pause (Space)".to_string(),
                    _ => format!("Game speed {}", speed.label()),
                });
            }
            // The date and time, beside the controls that change how fast it runs.
            win95::clock_pane(ui, 160.0, info.clock, info.daylight);
            if win95::money_pane(ui, 110.0, info.cash, info.in_the_red)
                .on_hover_text("Your money. Click for the finance window")
                .clicked()
            {
                s.show_finance = true;
            }
        });
    });
}

pub fn status_bar(ui: &mut Ui, s: &UiState, right: &str) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        let right_width = 260.0;
        let left = (ui.available_width() - right_width - 2.0).max(50.0);
        win95::status_pane(ui, left, s.tool.hint());
        win95::status_pane(ui, right_width, right);
    });
}

pub fn about_window(ctx: &egui::Context, open: &mut bool) {
    win95::window(ctx, "About Starcity", open, pos2(200.0, 150.0), |ui| {
        ui.label("Starcity: a transport tycoon on voxel terrain.");
        ui.label("Textures from ambientCG (CC0). See CREDITS.md.");
    });
}

// Icons, drawn into a ~20px square.

fn icon_port(p: &Painter, r: Rect) {
    let at = |x: f32, y: f32| pos2(r.min.x + x * r.width(), r.min.y + y * r.height());
    // Water, a quay with a crane over it.
    p.rect_filled(
        Rect::from_min_max(at(0.0, 0.6), at(1.0, 1.0)),
        0.0,
        Color32::from_rgb(0x2a, 0x6a, 0x9c),
    );
    p.rect_filled(
        Rect::from_min_max(at(0.0, 0.5), at(0.55, 0.65)),
        0.0,
        Color32::from_gray(0x90),
    );
    p.rect_filled(
        Rect::from_min_max(at(0.1, 0.25), at(0.3, 0.5)),
        0.0,
        Color32::from_rgb(0xa8, 0x80, 0x5c),
    );
    p.rect_filled(
        Rect::from_min_max(at(0.42, 0.05), at(0.5, 0.5)),
        0.0,
        Color32::from_rgb(0xc8, 0x78, 0x28),
    );
    p.rect_filled(
        Rect::from_min_max(at(0.42, 0.05), at(0.95, 0.12)),
        0.0,
        Color32::from_rgb(0xc8, 0x78, 0x28),
    );
    p.line_segment([at(0.85, 0.12), at(0.85, 0.45)], Stroke::new(1.0, Color32::BLACK));
}

fn icon_boat(p: &Painter, r: Rect) {
    let at = |x: f32, y: f32| pos2(r.min.x + x * r.width(), r.min.y + y * r.height());
    p.rect_filled(
        Rect::from_min_max(at(0.0, 0.7), at(1.0, 1.0)),
        0.0,
        Color32::from_rgb(0x2a, 0x6a, 0x9c),
    );
    p.add(egui::Shape::convex_polygon(
        vec![at(0.05, 0.5), at(0.95, 0.5), at(0.78, 0.8), at(0.2, 0.8)],
        Color32::from_rgb(0x1e, 0x34, 0x5c),
        Stroke::NONE,
    ));
    p.rect_filled(
        Rect::from_min_max(at(0.3, 0.28), at(0.65, 0.5)),
        0.0,
        Color32::from_gray(0xee),
    );
    p.rect_filled(
        Rect::from_min_max(at(0.42, 0.08), at(0.52, 0.28)),
        0.0,
        Color32::from_rgb(0xbe, 0x34, 0x2a),
    );
}

fn icon_airport(p: &Painter, r: Rect) {
    let at = |x: f32, y: f32| pos2(r.min.x + x * r.width(), r.min.y + y * r.height());
    p.rect_filled(r, 0.0, Color32::from_rgb(0x3a, 0x8c, 0x2f));
    // A runway with a dashed centre line, and a tower.
    p.rect_filled(
        Rect::from_min_max(at(0.0, 0.55), at(1.0, 0.85)),
        0.0,
        Color32::from_gray(0x3e),
    );
    for x in [0.08, 0.34, 0.6, 0.86] {
        p.rect_filled(Rect::from_min_max(at(x, 0.69), at(x + 0.12, 0.72)), 0.0, Color32::WHITE);
    }
    p.rect_filled(
        Rect::from_min_max(at(0.6, 0.25), at(0.7, 0.55)),
        0.0,
        Color32::from_gray(0xc8),
    );
    p.rect_filled(
        Rect::from_min_max(at(0.55, 0.12), at(0.75, 0.25)),
        0.0,
        Color32::from_rgb(0x3c, 0x5a, 0x78),
    );
}

fn icon_plane(p: &Painter, r: Rect) {
    let at = |x: f32, y: f32| pos2(r.min.x + x * r.width(), r.min.y + y * r.height());
    p.rect_filled(r, 0.0, Color32::from_rgb(0x7f, 0xb2, 0xd9));
    let body = Color32::from_gray(0xf0);
    p.rect_filled(Rect::from_min_max(at(0.1, 0.44), at(0.92, 0.58)), 0.0, body);
    p.add(egui::Shape::convex_polygon(
        vec![at(0.42, 0.5), at(0.62, 0.5), at(0.48, 0.08), at(0.38, 0.08)],
        Color32::from_gray(0xc8),
        Stroke::NONE,
    ));
    p.add(egui::Shape::convex_polygon(
        vec![at(0.42, 0.52), at(0.62, 0.52), at(0.48, 0.92), at(0.38, 0.92)],
        Color32::from_gray(0xc8),
        Stroke::NONE,
    ));
    p.add(egui::Shape::convex_polygon(
        vec![at(0.1, 0.46), at(0.22, 0.46), at(0.12, 0.26), at(0.06, 0.26)],
        Color32::from_rgb(0xbe, 0x34, 0x2a),
        Stroke::NONE,
    ));
}

fn icon_expressway(p: &Painter, r: Rect) {
    let at = |x: f32, y: f32| pos2(r.min.x + x * r.width(), r.min.y + y * r.height());
    for x in [0.2, 0.75] {
        p.rect_filled(Rect::from_min_max(at(x, 0.45), at(x + 0.08, 1.0)), 0.0, Color32::from_gray(0x90));
    }
    p.rect_filled(Rect::from_min_max(at(0.0, 0.3), at(1.0, 0.48)), 0.0, Color32::from_rgb(0xc9, 0xce, 0xd8));
    p.rect_filled(Rect::from_min_max(at(0.0, 0.37), at(1.0, 0.41)), 0.0, Color32::from_rgb(0x2a, 0xc8, 0xf0));
    p.rect_filled(Rect::from_min_max(at(0.3, 0.2), at(0.5, 0.3)), 1.0, Color32::from_gray(0xf4));
}

fn icon_bus(p: &Painter, r: Rect) {
    let at = |x: f32, y: f32| pos2(r.min.x + x * r.width(), r.min.y + y * r.height());
    let body = Color32::from_rgb(0x28, 0x6e, 0xc8);
    p.rect_filled(Rect::from_min_max(at(0.08, 0.22), at(0.92, 0.78)), 2.0, body);
    for x in [0.16, 0.36, 0.56] {
        p.rect_filled(Rect::from_min_max(at(x, 0.3), at(x + 0.14, 0.5)), 0.0, Color32::from_rgb(0xcf, 0xe6, 0xf7));
    }
    p.rect_filled(Rect::from_min_max(at(0.76, 0.3), at(0.88, 0.7)), 0.0, Color32::from_rgb(0xcf, 0xe6, 0xf7));
    for x in [0.26, 0.72] {
        p.circle_filled(at(x, 0.8), r.width() * 0.09, Color32::from_gray(0x20));
    }
}

fn icon_bus_stop(p: &Painter, r: Rect) {
    let at = |x: f32, y: f32| pos2(r.min.x + x * r.width(), r.min.y + y * r.height());
    p.rect_filled(Rect::from_min_max(at(0.0, 0.78), at(1.0, 1.0)), 0.0, Color32::from_gray(0x50));
    p.rect_filled(Rect::from_min_max(at(0.45, 0.25), at(0.53, 0.8)), 0.0, Color32::from_gray(0x60));
    p.circle_filled(at(0.49, 0.24), r.width() * 0.2, Color32::from_rgb(0xf2, 0xc2, 0x30));
    p.circle_stroke(at(0.49, 0.24), r.width() * 0.2, Stroke::new(1.0, Color32::from_gray(0x30)));
    p.text(at(0.49, 0.25), egui::Align2::CENTER_CENTER, "B", egui::FontId::proportional(r.height() * 0.3), Color32::from_gray(0x20));
}

fn icon_speed(p: &Painter, r: Rect, speed: Speed) {
    if speed == Speed::Paused {
        for x in [0.22, 0.58] {
            p.rect_filled(
                Rect::from_min_max(
                    pos2(r.min.x + x * r.width(), r.min.y + 0.15 * r.height()),
                    pos2(r.min.x + (x + 0.2) * r.width(), r.min.y + 0.85 * r.height()),
                ),
                0.0,
                win95::text(),
            );
        }
    } else {
        p.text(
            r.center(),
            egui::Align2::CENTER_CENTER,
            speed.label(),
            egui::FontId::proportional(13.0),
            win95::text(),
        );
    }
}

fn icon_pointer(p: &Painter, r: Rect) {
    let at = |x: f32, y: f32| pos2(r.min.x + x * r.width(), r.min.y + y * r.height());
    let outline = Stroke::new(1.0, Color32::BLACK);
    p.line_segment([at(0.45, 0.6), at(0.65, 1.0)], Stroke::new(3.0, Color32::BLACK));
    p.add(egui::Shape::convex_polygon(
        vec![at(0.2, 0.0), at(0.2, 0.8), at(0.75, 0.62)],
        Color32::WHITE,
        outline,
    ));
}

fn icon_road(p: &Painter, r: Rect) {
    p.rect_filled(r, 0.0, Color32::from_rgb(0x3a, 0x8c, 0x2f));
    let road = Rect::from_center_size(r.center(), vec2(r.width() * 0.6, r.height()));
    p.rect_filled(road, 0.0, Color32::from_rgb(0x50, 0x50, 0x50));
    let mut y = r.min.y + 1.0;
    while y < r.max.y {
        p.rect_filled(
            Rect::from_min_size(pos2(r.center().x - 0.5, y), vec2(1.0, 3.0)),
            0.0,
            Color32::WHITE,
        );
        y += 5.0;
    }
}

fn icon_rail(p: &Painter, r: Rect) {
    p.rect_filled(r, 0.0, Color32::from_rgb(0x9a, 0x8a, 0x70));
    let sleeper = Color32::from_rgb(0x6b, 0x45, 0x23);
    let mut y = r.min.y + 1.0;
    while y < r.max.y {
        p.rect_filled(
            Rect::from_min_size(pos2(r.min.x + 2.0, y), vec2(r.width() - 4.0, 2.0)),
            0.0,
            sleeper,
        );
        y += 4.0;
    }
    for fx in [0.3, 0.7] {
        let x = r.min.x + r.width() * fx;
        p.rect_filled(
            Rect::from_min_max(pos2(x - 1.0, r.min.y), pos2(x + 1.0, r.max.y)),
            0.0,
            Color32::from_gray(60),
        );
    }
}

fn icon_signal(p: &Painter, r: Rect) {
    let cx = r.center().x;
    p.rect_filled(
        Rect::from_min_max(pos2(cx - 1.0, r.min.y + 8.0), pos2(cx + 1.0, r.max.y)),
        0.0,
        Color32::from_gray(60),
    );
    let head = Rect::from_min_max(pos2(cx - 4.0, r.min.y), pos2(cx + 4.0, r.min.y + 13.0));
    p.rect_filled(head, 0.0, Color32::BLACK);
    p.circle_filled(pos2(cx, r.min.y + 3.5), 2.2, Color32::from_rgb(0xff, 0x30, 0x30));
    p.circle_filled(pos2(cx, r.min.y + 9.0), 2.2, Color32::from_rgb(0x30, 0xd0, 0x40));
}

fn icon_bulldozer(p: &Painter, r: Rect) {
    let at = |x: f32, y: f32| pos2(r.min.x + x * r.width(), r.min.y + y * r.height());
    let yellow = Color32::from_rgb(0xf0, 0xc0, 0x10);
    let dark = Color32::from_gray(40);
    // Cab, body, blade and tracks.
    p.rect_filled(Rect::from_min_max(at(0.35, 0.15), at(0.7, 0.5)), 0.0, yellow);
    p.rect_filled(
        Rect::from_min_max(at(0.42, 0.22), at(0.63, 0.42)),
        0.0,
        Color32::from_rgb(0x90, 0xc8, 0xf0),
    );
    p.rect_filled(Rect::from_min_max(at(0.2, 0.45), at(0.85, 0.7)), 0.0, yellow);
    p.rect_filled(
        Rect::from_min_max(at(0.0, 0.35), at(0.12, 0.9)),
        0.0,
        Color32::from_gray(110),
    );
    p.rect_filled(Rect::from_min_max(at(0.12, 0.62), at(0.22, 0.7)), 0.0, dark);
    p.rect_filled(Rect::from_min_max(at(0.2, 0.72), at(0.95, 1.0)), 2.0, dark);
}

fn icon_station(p: &Painter, r: Rect) {
    let at = |x: f32, y: f32| pos2(r.min.x + x * r.width(), r.min.y + y * r.height());
    // Building with a red roof, on a platform beside the track.
    p.rect_filled(
        Rect::from_min_max(at(0.15, 0.3), at(0.85, 0.65)),
        0.0,
        Color32::from_rgb(0xde, 0xcd, 0xa5),
    );
    p.rect_filled(
        Rect::from_min_max(at(0.05, 0.18), at(0.95, 0.32)),
        0.0,
        Color32::from_rgb(0x96, 0x3e, 0x32),
    );
    p.rect_filled(
        Rect::from_min_max(at(0.42, 0.42), at(0.58, 0.65)),
        0.0,
        Color32::from_rgb(0x5a, 0x3c, 0x28),
    );
    p.rect_filled(
        Rect::from_min_max(at(0.0, 0.65), at(1.0, 0.78)),
        0.0,
        Color32::from_gray(0xb0),
    );
    p.rect_filled(
        Rect::from_min_max(at(0.0, 0.84), at(1.0, 0.9)),
        0.0,
        Color32::from_gray(0x50),
    );
}

fn icon_train(p: &Painter, r: Rect) {
    let at = |x: f32, y: f32| pos2(r.min.x + x * r.width(), r.min.y + y * r.height());
    let green = Color32::from_rgb(0x23, 0x69, 0x3c);
    p.rect_filled(Rect::from_min_max(at(0.05, 0.35), at(0.7, 0.75)), 0.0, green);
    p.rect_filled(
        Rect::from_min_max(at(0.55, 0.15), at(0.95, 0.75)),
        0.0,
        Color32::from_rgb(0x19, 0x4b, 0x2d),
    );
    p.rect_filled(
        Rect::from_min_max(at(0.62, 0.22), at(0.88, 0.42)),
        0.0,
        Color32::from_rgb(0x90, 0xc8, 0xf0),
    );
    p.rect_filled(
        Rect::from_min_max(at(0.12, 0.12), at(0.24, 0.35)),
        0.0,
        Color32::from_gray(40),
    );
    for x in [0.2, 0.45, 0.78] {
        p.circle_filled(at(x, 0.82), r.width() * 0.1, Color32::from_gray(30));
    }
}

/// The projection button: a globe (perspective) that unwraps into a flat square map (orthographic).
/// `flat` runs from 0 (globe) to 1 (map).
pub fn icon_projection(p: &Painter, r: Rect, flat: f32) {
    let c = r.center();
    let rad = r.width().min(r.height()) / 2.0;
    let at = |x: f32, y: f32| pos2(c.x + x * rad, c.y + y * rad);
    let ocean = Color32::from_rgb(0x2a, 0x6c, 0xb8);
    let sea_flat = Color32::from_rgb(0x4a, 0x84, 0xc4);
    let fill = Color32::from_rgb(
        (ocean.r() as f32 + (sea_flat.r() as f32 - ocean.r() as f32) * flat) as u8,
        (ocean.g() as f32 + (sea_flat.g() as f32 - ocean.g() as f32) * flat) as u8,
        (ocean.b() as f32 + (sea_flat.b() as f32 - ocean.b() as f32) * flat) as u8,
    );
    // Outline: a circle that squares off as it unwraps.
    let n = 48;
    let outline: Vec<_> = (0..n)
        .map(|i| {
            let a = i as f32 * std::f32::consts::TAU / n as f32;
            let (cx, sy) = (a.cos(), a.sin());
            let square = 1.0 / cx.abs().max(sy.abs());
            let k = 1.0 + (square - 1.0) * flat;
            at(cx * k * 0.95, sy * k * 0.95)
        })
        .collect();
    p.add(egui::Shape::convex_polygon(
        outline,
        fill,
        Stroke::new(1.0, Color32::BLACK),
    ));
    // Land: a couple of patches that stretch with the map.
    let land = Color32::from_rgb(0x4a, 0xa0, 0x3a);
    let width = 1.0 + 0.0 * flat;
    let stretch = 0.82 + 0.18 * flat;
    for (x, y, w, h) in [(-0.3, -0.25, 0.34, 0.26), (0.28, 0.2, 0.3, 0.3)] {
        let pts: Vec<_> = (0..14)
            .map(|i| {
                let a = i as f32 * std::f32::consts::TAU / 14.0;
                at((x + a.cos() * w * 0.8) * stretch * width, y + a.sin() * h * 0.8)
            })
            .collect();
        p.add(egui::Shape::convex_polygon(pts, land, Stroke::NONE));
    }
    // Meridians and parallels: curves on the globe, straight lines on the map.
    let line = Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 150));
    let lerp = |a: f32, b: f32| a + (b - a) * flat;
    for k in [-0.55f32, 0.0, 0.55] {
        let pts: Vec<_> = (0..=16)
            .map(|i| {
                let y = -0.95 + 1.9 * i as f32 / 16.0;
                at(k * lerp((1.0 - y * y).max(0.0).sqrt(), 1.0) * 0.95, y)
            })
            .collect();
        p.add(egui::Shape::line(pts, line));
        let w = lerp((1.0 - k * k).sqrt(), 1.0) * 0.95;
        p.line_segment([at(-w, k * 0.95), at(w, k * 0.95)], line);
    }
}

/// The headquarters: a little office with a flag.
fn icon_hq(p: &Painter, r: Rect) {
    let at = |x: f32, y: f32| pos2(r.min.x + x * r.width(), r.min.y + y * r.height());
    p.rect_filled(
        Rect::from_two_pos(at(0.15, 0.95), at(0.85, 0.35)),
        0.0,
        Color32::from_rgb(0xc8, 0xc0, 0xa8),
    );
    p.rect_filled(
        Rect::from_two_pos(at(0.1, 0.35), at(0.9, 0.25)),
        0.0,
        Color32::from_rgb(0x50, 0x54, 0x60),
    );
    for (x, y) in [(0.28, 0.5), (0.5, 0.5), (0.72, 0.5), (0.28, 0.72), (0.72, 0.72)] {
        p.rect_filled(
            Rect::from_center_size(at(x, y), vec2(r.width() * 0.13, r.height() * 0.13)),
            0.0,
            Color32::from_rgb(0x30, 0x58, 0x90),
        );
    }
    p.rect_filled(
        Rect::from_two_pos(at(0.44, 0.95), at(0.56, 0.78)),
        0.0,
        Color32::from_rgb(0x6b, 0x45, 0x23),
    );
    p.line_segment([at(0.5, 0.25), at(0.5, 0.02)], Stroke::new(1.5, Color32::BLACK));
    p.rect_filled(
        Rect::from_two_pos(at(0.5, 0.02), at(0.78, 0.14)),
        0.0,
        Color32::from_rgb(0xd0, 0x20, 0x20),
    );
}

/// The Assistant: a friendly face in a speech bubble.
pub fn icon_assistant(p: &Painter, r: Rect) {
    let at = |x: f32, y: f32| pos2(r.min.x + x * r.width(), r.min.y + y * r.height());
    let bubble = Rect::from_two_pos(at(0.02, 0.05), at(0.98, 0.78));
    p.rect_filled(bubble, r.width() * 0.2, Color32::from_rgb(0xff, 0xf4, 0xb0));
    p.rect_stroke(
        bubble,
        r.width() * 0.2,
        Stroke::new(1.0, Color32::BLACK),
        egui::StrokeKind::Inside,
    );
    p.add(egui::Shape::convex_polygon(
        vec![at(0.25, 0.76), at(0.2, 0.98), at(0.48, 0.76)],
        Color32::from_rgb(0xff, 0xf4, 0xb0),
        Stroke::new(1.0, Color32::BLACK),
    ));
    let eye = Color32::BLACK;
    p.circle_filled(at(0.36, 0.34), r.width() * 0.07, eye);
    p.circle_filled(at(0.64, 0.34), r.width() * 0.07, eye);
    p.add(egui::Shape::line(
        (0..=8)
            .map(|i| {
                let t = i as f32 / 8.0;
                at(0.32 + 0.36 * t, 0.5 + 0.1 * (std::f32::consts::PI * t).sin())
            })
            .collect(),
        Stroke::new(1.5, eye),
    ));
}

fn icon_island(p: &Painter, r: Rect) {
    let at = |x: f32, y: f32| pos2(r.min.x + x * r.width(), r.min.y + y * r.height());
    p.rect_filled(r, 0.0, Color32::from_rgb(0x30, 0x70, 0xb0));
    p.circle_filled(at(0.5, 0.62), r.width() * 0.36, Color32::from_rgb(0xd8, 0xc0, 0x80));
    p.circle_filled(at(0.5, 0.6), r.width() * 0.28, Color32::from_rgb(0x3a, 0x8c, 0x2f));
    // A palm tree.
    p.line_segment(
        [at(0.55, 0.62), at(0.5, 0.18)],
        Stroke::new(2.0, Color32::from_rgb(0x6b, 0x45, 0x23)),
    );
    let leaf = Stroke::new(2.0, Color32::from_rgb(0x20, 0x60, 0x20));
    for (x, y) in [(0.25, 0.25), (0.75, 0.22), (0.35, 0.05), (0.68, 0.06)] {
        p.line_segment([at(0.5, 0.18), at(x, y)], leaf);
    }
}

fn icon_cross(p: &Painter, r: Rect) {
    let s = Stroke::new(2.5, Color32::from_rgb(0xd0, 0x10, 0x10));
    let r = r.shrink(3.0);
    p.line_segment([r.left_top(), r.right_bottom()], s);
    p.line_segment([r.right_top(), r.left_bottom()], s);
}
