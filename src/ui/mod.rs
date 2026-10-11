//! Game UI: Win95-style menu bar, toolbar and status bar.

pub mod win95;

use eframe::egui::{self, Color32, Painter, Rect, Stroke, Ui, pos2, vec2};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tool {
    #[default]
    Pointer,
    Road,
    RemoveRoad,
    Rail,
    Signal,
    RemoveRail,
    Bulldoze,
}

impl Tool {
    pub fn name(self) -> &'static str {
        match self {
            Tool::Pointer => "Pointer",
            Tool::Road => "Build road",
            Tool::RemoveRoad => "Remove road",
            Tool::Rail => "Build railway",
            Tool::Signal => "Place signal",
            Tool::RemoveRail => "Remove railway",
            Tool::Bulldoze => "Bulldoze",
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            Tool::Pointer => "Drag to pan, right-drag to rotate, scroll to zoom",
            Tool::Road => "Drag along an axis or at 45° to draw road. Red means it can't be built there.",
            Tool::RemoveRoad => "Click or drag a rectangle to remove road",
            Tool::Rail => "Drag along an axis or at 45° to lay track. Red means it can't be built there.",
            Tool::Signal => "Click track to place a signal (coming soon)",
            Tool::RemoveRail => "Click or drag a rectangle to remove track",
            Tool::Bulldoze => "Click or drag a rectangle to clear it to bare ground",
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
            Tool::RemoveRail => {
                icon_rail(p, r);
                icon_cross(p, r);
            }
            Tool::Bulldoze => icon_bulldozer(p, r),
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
    pub show_debug: bool,
    pub show_about: bool,
    /// Set for one frame when the player asks to centre the view on the island.
    pub go_home: bool,
}

#[derive(Default)]
pub struct UiActions {
    pub new_map: bool,
}

pub fn menu_bar(ui: &mut Ui, s: &mut UiState) -> UiActions {
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
            ui.separator();
            if ui.button("Centre on island\tHome").clicked() {
                s.go_home = true;
                ui.close();
            }
            ui.separator();
            ui.checkbox(&mut s.show_debug, "Debug window\tF3");
        });
        ui.menu_button("Help", |ui| {
            if ui.button("About Starcity").clicked() {
                s.show_about = true;
            }
        });
    });
    actions
}

pub fn toolbar(ui: &mut Ui, s: &mut UiState) {
    const SIZE: f32 = 30.0;
    let groups: [(&str, &[Tool]); 4] = [
        ("", &[Tool::Pointer]),
        ("Land", &[Tool::Bulldoze]),
        ("Roads", &[Tool::Road, Tool::RemoveRoad]),
        ("Rail", &[Tool::Rail, Tool::Signal, Tool::RemoveRail]),
    ];
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        let home = win95::tool_button(ui, SIZE, false, icon_island);
        if home.clicked() {
            s.go_home = true;
        }
        home.on_hover_text("Centre on island (Home)");
        win95::separator(ui, SIZE);
        for (i, (label, tools)) in groups.iter().enumerate() {
            if i > 0 {
                win95::separator(ui, SIZE);
            }
            if !label.is_empty() {
                ui.label(*label);
            }
            for &tool in *tools {
                let selected = s.tool == tool;
                let response = win95::tool_button(ui, SIZE, selected, |p, r| tool.icon(p, r));
                if response.clicked() {
                    s.tool = if selected { Tool::Pointer } else { tool };
                }
                response.on_hover_text(tool.name());
            }
        }
    });
}

pub fn status_bar(ui: &mut Ui, s: &UiState, right: &str) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        let right_width = 180.0;
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
