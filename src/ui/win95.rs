//! Windows 95 look for egui: grey surfaces, bevelled edges, navy selection.

use eframe::egui::{self, Color32, CornerRadius, Painter, Rect, Response, Sense, Shadow, Stroke, StrokeKind, Ui, Vec2, pos2, vec2};
use std::sync::atomic::{AtomicU8, Ordering};

pub const FACE: Color32 = Color32::from_rgb(0xc0, 0xc0, 0xc0);
pub const LIGHT: Color32 = Color32::from_rgb(0xdf, 0xdf, 0xdf);
pub const HIGHLIGHT: Color32 = Color32::WHITE;
pub const SHADOW: Color32 = Color32::from_rgb(0x80, 0x80, 0x80);
pub const DARK: Color32 = Color32::from_rgb(0x0a, 0x0a, 0x0a);
pub const NAVY: Color32 = Color32::from_rgb(0x00, 0x00, 0x80);

/// The look of the windows. The company's software upgrades about ten years behind the world
/// (GAME-DESIGN.md §10.3): layout never changes, only the paint.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Look {
    /// 2000–2009: grey bevels, navy title bars.
    Classic,
    /// 2010–2019: translucent, glossy, rounded.
    Glass,
    /// 2020–2029: flat, rounded, large and quiet.
    Sleek,
    /// 2030–2039: thin luminous edges on dark glass.
    Holo,
    /// 2040–2047: polished metal slabs.
    Chrome,
    /// 2048 on: the frames fade into the world.
    Ambient,
}

impl Look {
    pub const ALL: [Look; 6] = [Look::Classic, Look::Glass, Look::Sleek, Look::Holo, Look::Chrome, Look::Ambient];

    pub fn for_year(year: i32) -> Look {
        match year {
            ..=2009 => Look::Classic,
            2010..=2019 => Look::Glass,
            2020..=2029 => Look::Sleek,
            2030..=2039 => Look::Holo,
            2040..=2047 => Look::Chrome,
            _ => Look::Ambient,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Look::Classic => "Classic",
            Look::Glass => "Glass",
            Look::Sleek => "Sleek",
            Look::Holo => "Holo",
            Look::Chrome => "Liquid chrome",
            Look::Ambient => "Ambient",
        }
    }

    /// The first year the company's software can have this look: it is unlocked from then on.
    pub fn from_year(self) -> i32 {
        match self {
            Look::Classic => 2000,
            Look::Glass => 2010,
            Look::Sleek => 2020,
            Look::Holo => 2030,
            Look::Chrome => 2040,
            Look::Ambient => 2048,
        }
    }

    fn index(self) -> u8 {
        Look::ALL.iter().position(|&l| l == self).unwrap() as u8
    }
}

static LOOK: AtomicU8 = AtomicU8::new(0);

pub fn look() -> Look {
    Look::ALL[(LOOK.load(Ordering::Relaxed) as usize).min(5)]
}

/// Switches the look. Call `apply` afterwards so the stock widgets follow.
pub fn set_look(l: Look) {
    LOOK.store(l.index(), Ordering::Relaxed);
}

struct Palette {
    face: Color32,
    /// A step lighter and darker than the face, and the lit edge and the dark edge.
    light: Color32,
    highlight: Color32,
    shadow: Color32,
    dark: Color32,
    accent: Color32,
    accent_end: Color32,
    text: Color32,
    radius: u8,
    /// Dark glass with light text.
    dark_ui: bool,
}

fn rgba(r: u8, g: u8, b: u8, a: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(r, g, b, a)
}

/// The look being blended away from, and how far the blend has got (0 to 1, as f32 bits).
static BLEND_FROM: AtomicU8 = AtomicU8::new(0);
static BLEND: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0x3f80_0000);

/// For the first months of a new look, its colours ease in from the old one's (the shapes change at
/// once). `t` is 0 at the boundary and 1 when the new look is complete.
pub fn set_blend(from: Look, t: f32) {
    BLEND_FROM.store(from.index(), Ordering::Relaxed);
    BLEND.store(t.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
}

pub fn blend() -> f32 {
    f32::from_bits(BLEND.load(Ordering::Relaxed))
}

fn pal() -> Palette {
    let new = palette_of(look());
    let t = blend();
    if t >= 1.0 {
        return new;
    }
    let old = palette_of(Look::ALL[(BLEND_FROM.load(Ordering::Relaxed) as usize).min(5)]);
    let mix = |a: Color32, b: Color32| a.lerp_to_gamma(b, t);
    Palette {
        face: mix(old.face, new.face),
        light: mix(old.light, new.light),
        highlight: mix(old.highlight, new.highlight),
        shadow: mix(old.shadow, new.shadow),
        dark: mix(old.dark, new.dark),
        accent: mix(old.accent, new.accent),
        accent_end: mix(old.accent_end, new.accent_end),
        text: if t < 0.5 && old.dark_ui == new.dark_ui { mix(old.text, new.text) } else { new.text },
        radius: new.radius,
        dark_ui: new.dark_ui,
    }
}

fn palette_of(l: Look) -> Palette {
    match l {
        Look::Classic => Palette {
            face: FACE,
            light: LIGHT,
            highlight: HIGHLIGHT,
            shadow: SHADOW,
            dark: DARK,
            accent: NAVY,
            accent_end: Color32::from_rgb(0x10, 0x84, 0xd0),
            text: Color32::BLACK,
            radius: 0,
            dark_ui: false,
        },
        Look::Glass => Palette {
            face: Color32::from_rgb(0xdd, 0xe6, 0xf1),
            light: Color32::from_rgb(0xf3, 0xf7, 0xfc),
            highlight: Color32::WHITE,
            shadow: Color32::from_rgb(0x8f, 0xa2, 0xbb),
            dark: Color32::from_rgb(0x4b, 0x5d, 0x78),
            accent: Color32::from_rgb(0x2b, 0x78, 0xc8),
            accent_end: Color32::from_rgb(0x8c, 0xc4, 0xf4),
            text: Color32::from_rgb(0x10, 0x18, 0x24),
            radius: 6,
            dark_ui: false,
        },
        Look::Sleek => Palette {
            face: Color32::from_rgb(0xf3, 0xf3, 0xf6),
            light: Color32::WHITE,
            highlight: Color32::WHITE,
            shadow: Color32::from_rgb(0xcf, 0xcf, 0xd6),
            dark: Color32::from_rgb(0xa4, 0xa4, 0xae),
            accent: Color32::from_rgb(0x0a, 0x84, 0xff),
            accent_end: Color32::from_rgb(0x0a, 0x84, 0xff),
            text: Color32::from_rgb(0x1c, 0x1c, 0x1e),
            radius: 10,
            dark_ui: false,
        },
        Look::Holo => Palette {
            face: rgba(0x0b, 0x17, 0x26, 0xdc),
            light: rgba(0x16, 0x2c, 0x44, 0xe6),
            highlight: Color32::from_rgb(0x5f, 0xe3, 0xff),
            shadow: Color32::from_rgb(0x1d, 0x3d, 0x5c),
            dark: Color32::from_rgb(0x2f, 0x78, 0x9c),
            accent: Color32::from_rgb(0x19, 0xb8, 0xe6),
            accent_end: Color32::from_rgb(0x0a, 0x4a, 0x70),
            text: Color32::from_rgb(0xd8, 0xf6, 0xff),
            radius: 2,
            dark_ui: true,
        },
        Look::Chrome => Palette {
            face: Color32::from_rgb(0xb9, 0xbf, 0xc9),
            light: Color32::from_rgb(0xe6, 0xea, 0xf1),
            highlight: Color32::WHITE,
            shadow: Color32::from_rgb(0x6b, 0x72, 0x7f),
            dark: Color32::from_rgb(0x2a, 0x2f, 0x38),
            accent: Color32::from_rgb(0x4d, 0x5d, 0x7a),
            accent_end: Color32::from_rgb(0xd0, 0xda, 0xea),
            text: Color32::from_rgb(0x12, 0x16, 0x1d),
            radius: 8,
            dark_ui: false,
        },
        Look::Ambient => Palette {
            face: rgba(0x08, 0x10, 0x1a, 0x9a),
            light: rgba(0x10, 0x1c, 0x2c, 0xb4),
            highlight: Color32::from_rgb(0xa8, 0xde, 0xf2),
            shadow: Color32::from_rgb(0x20, 0x34, 0x48),
            dark: Color32::from_rgb(0x3a, 0x5c, 0x76),
            accent: Color32::from_rgb(0x7f, 0xd3, 0xee),
            accent_end: Color32::from_rgb(0x7f, 0xd3, 0xee),
            text: Color32::from_rgb(0xe8, 0xf4, 0xfa),
            radius: 4,
            dark_ui: true,
        },
    }
}

/// The colour of surfaces, text and the things text warns about, in the current look.
pub fn face() -> Color32 {
    pal().face
}

pub fn text() -> Color32 {
    pal().text
}

/// Accent colour for numbers that matter (the compute meter).
pub fn ink() -> Color32 {
    let p = pal();
    match look() {
        Look::Classic => Color32::from_rgb(0x10, 0x40, 0x90),
        Look::Holo | Look::Ambient => p.highlight,
        Look::Chrome => p.dark,
        _ => p.accent,
    }
}

pub fn warn() -> Color32 {
    if pal().dark_ui {
        Color32::from_rgb(0xff, 0xb0, 0x70)
    } else {
        Color32::from_rgb(0x90, 0x30, 0x10)
    }
}

pub fn bad() -> Color32 {
    if pal().dark_ui {
        Color32::from_rgb(0xff, 0x70, 0x70)
    } else {
        Color32::from_rgb(0xc0, 0x10, 0x10)
    }
}

fn corner(radius: u8) -> CornerRadius {
    CornerRadius::same(radius)
}

pub fn apply(ctx: &egui::Context) {
    let p = pal();
    let classic = look() == Look::Classic;
    ctx.all_styles_mut(|style| {
        let v = &mut style.visuals;
        *v = if p.dark_ui { egui::Visuals::dark() } else { egui::Visuals::light() };
        v.override_text_color = None;
        v.panel_fill = p.face;
        v.window_fill = p.face;
        v.window_stroke = Stroke::new(1.0, if classic { DARK } else { p.shadow });
        v.window_shadow = if classic || look() == Look::Ambient {
            Shadow::NONE
        } else {
            Shadow { offset: [0, 6], blur: 18, spread: 0, color: Color32::from_black_alpha(70) }
        };
        v.popup_shadow = v.window_shadow;
        v.window_corner_radius = corner(p.radius);
        v.menu_corner_radius = corner(p.radius.min(6));
        v.extreme_bg_color = if p.dark_ui { p.light } else { Color32::WHITE };
        v.faint_bg_color = p.light;
        v.selection.bg_fill = p.accent;
        v.selection.stroke = Stroke::new(1.0, if p.dark_ui { Color32::BLACK } else { Color32::WHITE });
        v.hyperlink_color = p.accent;
        v.slider_trailing_fill = !classic;
        v.handle_shape = if classic {
            egui::style::HandleShape::Rect { aspect_ratio: 0.5 }
        } else {
            egui::style::HandleShape::Circle
        };

        let w = &mut v.widgets;
        let edge_colour = if classic { DARK } else { p.dark };
        for (wv, stroke) in [
            (&mut w.noninteractive, p.shadow),
            (&mut w.inactive, edge_colour),
            (&mut w.hovered, if classic { DARK } else { p.accent }),
            (&mut w.active, if classic { DARK } else { p.accent }),
            (&mut w.open, edge_colour),
        ] {
            wv.corner_radius = corner(p.radius.min(8));
            wv.bg_fill = p.face;
            wv.weak_bg_fill = p.face;
            wv.bg_stroke = Stroke::new(1.0, stroke);
            wv.fg_stroke = Stroke::new(1.0, p.text);
            wv.expansion = 0.0;
        }
        w.noninteractive.bg_stroke = Stroke::new(1.0, p.shadow);
        // bg_fill is used for slider grooves and checkbox boxes, which are white in Win95; buttons
        // use weak_bg_fill and stay grey.
        for wv in [&mut w.inactive, &mut w.hovered, &mut w.active] {
            wv.bg_fill = if p.dark_ui { p.light } else if classic { Color32::WHITE } else { p.light };
        }
        w.hovered.weak_bg_fill = if classic { LIGHT } else { p.light };
        w.active.weak_bg_fill = if classic { SHADOW } else { p.shadow };
        // Open menus and selected items: white on navy, like Win95 menus.
        w.open.weak_bg_fill = p.accent;
        w.open.bg_fill = p.accent;
        w.open.fg_stroke = Stroke::new(1.0, if p.dark_ui { Color32::BLACK } else { Color32::WHITE });
        v.weak_text_color = Some(if classic { SHADOW } else { p.dark });

        style.spacing.window_margin = egui::Margin::same(if classic { 6 } else { 8 });
        style.spacing.button_padding = if classic { vec2(8.0, 3.0) } else { vec2(10.0, 4.0) };
        style.spacing.menu_margin = egui::Margin::same(2);
    });
}

/// A window in the current look: a frame, a title bar and a close button. Drag the title bar (or
/// any empty part of the window) to move it.
pub fn window<R>(
    ctx: &egui::Context,
    title: &str,
    open: &mut bool,
    default_pos: egui::Pos2,
    add_contents: impl FnOnce(&mut Ui) -> R,
) -> Option<R> {
    if !*open {
        return None;
    }
    let p = pal();
    let mut close = false;
    let frame = egui::Frame::NONE
        .fill(p.face)
        .corner_radius(corner(p.radius))
        .shadow(ctx.global_style().visuals.window_shadow)
        .inner_margin(egui::Margin::same(if look() == Look::Classic { 4 } else { 6 }));
    let response = egui::Window::new(title)
        .title_bar(false)
        .frame(frame)
        .fade_in(false)
        .fade_out(false)
        .default_pos(default_pos)
        .show(ctx, |ui| {
            close = title_bar(ui, title);
            ui.add_space(4.0);
            egui::Frame::NONE
                .inner_margin(egui::Margin::symmetric(4, 0))
                .show(ui, add_contents)
                .inner
        });
    let response = response?;
    bevel(
        &ctx.layer_painter(response.response.layer_id),
        response.response.rect,
        true,
    );
    if close {
        *open = false;
    }
    response.inner
}

/// A vertical gradient filling `rect` (rounded at the top corners if `radius`).
fn gradient(p: &Painter, rect: Rect, top: Color32, bottom: Color32, radius: u8) {
    let r = radius as f32;
    if r <= 0.0 {
        let mut mesh = egui::Mesh::default();
        mesh.colored_vertex(rect.left_top(), top);
        mesh.colored_vertex(rect.right_top(), top);
        mesh.colored_vertex(rect.right_bottom(), bottom);
        mesh.colored_vertex(rect.left_bottom(), bottom);
        mesh.add_triangle(0, 1, 2);
        mesh.add_triangle(0, 2, 3);
        p.add(egui::Shape::mesh(mesh));
        return;
    }
    // Rounded: a flat fill in the middle colour, with the gradient laid over the straight part.
    let mid = top.lerp_to_gamma(bottom, 0.5);
    p.rect_filled(rect, r, mid);
    let inner = rect.shrink2(vec2(r * 0.5, 0.0));
    gradient(p, Rect::from_min_max(inner.min + vec2(0.0, r * 0.4), inner.max - vec2(0.0, r * 0.4)), top, bottom, 0);
}

/// Paints a title bar across the top of a window. Returns true if the close button was clicked.
fn title_bar(ui: &mut Ui, title: &str) -> bool {
    let pl = pal();
    let height = if look() == Look::Classic { 20.0 } else { 24.0 };
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width().max(160.0), height), Sense::hover());
    let p = ui.painter();
    let bold = look() == Look::Classic;
    match look() {
        Look::Classic => gradient(p, rect, pl.accent, pl.accent_end, 0),
        Look::Glass => {
            p.rect_filled(rect, corner(pl.radius.saturating_sub(1)), pl.accent);
            gradient(p, rect.shrink2(vec2(2.0, 1.0)), pl.accent_end, pl.accent, 0);
            p.rect_filled(
                Rect::from_min_max(rect.min + vec2(3.0, 1.0), pos2(rect.max.x - 3.0, rect.center().y)),
                corner(4),
                Color32::from_white_alpha(46),
            );
        }
        Look::Sleek => {
            p.line_segment(
                [pos2(rect.min.x, rect.max.y), pos2(rect.max.x, rect.max.y)],
                Stroke::new(1.0, pl.shadow),
            );
        }
        Look::Holo | Look::Ambient => {
            p.line_segment(
                [pos2(rect.min.x, rect.max.y), pos2(rect.max.x, rect.max.y)],
                Stroke::new(1.0, pl.highlight.gamma_multiply(if look() == Look::Holo { 0.8 } else { 0.35 })),
            );
            p.line_segment(
                [pos2(rect.min.x, rect.max.y), pos2(rect.min.x + 36.0, rect.max.y)],
                Stroke::new(2.0, pl.highlight),
            );
        }
        Look::Chrome => {
            gradient(p, rect, Color32::WHITE, pl.shadow, 0);
            p.line_segment(
                [pos2(rect.min.x, rect.center().y), pos2(rect.max.x, rect.center().y)],
                Stroke::new(1.0, Color32::from_white_alpha(120)),
            );
        }
    }
    let title_colour = match look() {
        Look::Classic | Look::Glass => Color32::WHITE,
        _ => pl.text,
    };
    let at = rect.min.x + if look() == Look::Classic { 5.0 } else { 8.0 };
    let font = egui::FontId::proportional(if look() == Look::Classic { 13.0 } else { 14.0 });
    p.text(pos2(at, rect.center().y), egui::Align2::LEFT_CENTER, title, font.clone(), title_colour);
    if bold {
        // Bold by overdrawing one pixel to the right.
        p.text(pos2(at + 1.0, rect.center().y), egui::Align2::LEFT_CENTER, title, font, title_colour);
    }

    let button = Rect::from_min_size(pos2(rect.max.x - 20.0, rect.center().y - 8.0), vec2(16.0, 16.0));
    let response = ui.interact(button, ui.id().with("close"), Sense::click());
    let pressed = response.is_pointer_button_down_on();
    let cross = if matches!(look(), Look::Classic | Look::Glass | Look::Sleek | Look::Chrome) {
        Color32::BLACK
    } else {
        pl.highlight
    };
    match look() {
        Look::Classic => {
            let b = Rect::from_min_size(pos2(rect.max.x - 18.0, rect.min.y + 2.0), vec2(16.0, 14.0));
            p.rect_filled(b, 0.0, pl.face);
            bevel(p, b, !pressed);
        }
        Look::Sleek => {
            p.circle_filled(button.center(), 7.0, if response.hovered() { Color32::from_rgb(0xff, 0x5f, 0x57) } else { Color32::from_rgb(0xdd, 0xdd, 0xe2) });
        }
        Look::Glass => {
            p.circle_filled(button.center(), 8.0, if response.hovered() { Color32::from_rgb(0xe0, 0x50, 0x40) } else { Color32::from_white_alpha(170) });
        }
        Look::Chrome => {
            p.circle_filled(button.center(), 8.0, Color32::from_rgb(0xdd, 0xe2, 0xea));
            p.circle_stroke(button.center(), 8.0, Stroke::new(1.0, pl.dark));
        }
        Look::Holo | Look::Ambient => {
            if response.hovered() {
                p.circle_filled(button.center(), 8.0, pl.highlight.gamma_multiply(0.25));
            }
        }
    }
    let c = button.center() + if pressed && look() == Look::Classic { vec2(1.0, 1.0) } else { Vec2::ZERO };
    let s = Stroke::new(1.5, cross);
    p.line_segment([c + vec2(-3.5, -3.0), c + vec2(3.5, 3.0)], s);
    p.line_segment([c + vec2(3.5, -3.0), c + vec2(-3.5, 3.0)], s);
    response.clicked()
}

/// Draws the border of a raised or sunken thing inside `rect`: a two-pixel bevel in Classic, and a
/// soft, glowing or metallic edge in the other looks.
pub fn bevel(painter: &Painter, rect: Rect, raised: bool) {
    let p = pal();
    match look() {
        Look::Classic => {
            let (outer_tl, outer_br, inner_tl, inner_br) = if raised {
                (p.highlight, p.dark, p.light, p.shadow)
            } else {
                (p.shadow, p.highlight, p.dark, p.light)
            };
            edge(painter, rect, outer_tl, outer_br);
            edge(painter, rect.shrink(1.0), inner_tl, inner_br);
        }
        Look::Glass | Look::Chrome => {
            let r = corner(p.radius.min(8));
            painter.rect_stroke(rect, r, Stroke::new(1.0, if raised { p.dark.gamma_multiply(0.7) } else { p.shadow }), StrokeKind::Inside);
            if raised {
                painter.rect_stroke(rect.shrink(1.0), r, Stroke::new(1.0, Color32::from_white_alpha(150)), StrokeKind::Inside);
            }
        }
        Look::Sleek => {
            painter.rect_stroke(
                rect,
                corner(p.radius.min(8)),
                Stroke::new(1.0, if raised { p.shadow } else { p.dark }),
                StrokeKind::Inside,
            );
        }
        Look::Holo | Look::Ambient => {
            let strength = match (look(), raised) {
                (Look::Holo, true) => 0.85,
                (Look::Holo, false) => 0.45,
                (_, true) => 0.4,
                (_, false) => 0.22,
            };
            let colour = p.highlight.gamma_multiply(strength);
            painter.rect_stroke(rect, corner(p.radius), Stroke::new(1.0, colour), StrokeKind::Inside);
            if look() == Look::Holo && raised {
                // Brighter corners, like the brackets of a projected panel.
                let t = 5.0;
                for (c, dx, dy) in [(rect.left_top(), 1.0, 1.0), (rect.right_bottom(), -1.0, -1.0)] {
                    let s = Stroke::new(2.0, p.highlight);
                    painter.line_segment([c, c + vec2(t * dx, 0.0)], s);
                    painter.line_segment([c, c + vec2(0.0, t * dy)], s);
                }
            }
        }
    }
}

/// Fills a button or pane face in the current look, before its border is drawn.
fn fill_face(painter: &Painter, rect: Rect, raised: bool) {
    let p = pal();
    let r = p.radius.min(8);
    match look() {
        Look::Classic => {
            painter.rect_filled(rect, 0.0, p.face);
        }
        Look::Glass if raised => {
            painter.rect_filled(rect, corner(r), p.face);
            gradient(painter, rect.shrink2(vec2(1.0, 1.0)), p.highlight, p.face, 0);
            painter.rect_filled(
                Rect::from_min_max(rect.min + vec2(2.0, 1.0), pos2(rect.max.x - 2.0, rect.center().y)),
                corner(r.saturating_sub(2)),
                Color32::from_white_alpha(70),
            );
        }
        Look::Chrome if raised => {
            painter.rect_filled(rect, corner(r), p.face);
            gradient(painter, rect.shrink(1.0), Color32::WHITE, p.shadow, 0);
            gradient(painter, Rect::from_min_max(rect.left_center() + vec2(1.0, 0.0), rect.right_bottom() - vec2(1.0, 1.0)), p.shadow.gamma_multiply(0.9), p.light, 0);
        }
        _ => {
            painter.rect_filled(rect, corner(r), if raised { p.face } else { p.light });
        }
    }
}

fn edge(painter: &Painter, r: Rect, tl: Color32, br: Color32) {
    let px = 1.0;
    painter.rect_filled(Rect::from_min_max(r.min, pos2(r.max.x, r.min.y + px)), 0.0, tl);
    painter.rect_filled(Rect::from_min_max(r.min, pos2(r.min.x + px, r.max.y)), 0.0, tl);
    painter.rect_filled(Rect::from_min_max(pos2(r.min.x, r.max.y - px), r.max), 0.0, br);
    painter.rect_filled(Rect::from_min_max(pos2(r.max.x - px, r.min.y), r.max), 0.0, br);
}

/// A sunken pane (cash, date, status bar): a bevel in Classic, a recessed rounded field otherwise.
fn sunken(painter: &Painter, rect: Rect) {
    let p = pal();
    if look() == Look::Classic {
        edge(painter, rect, p.shadow, p.highlight);
    } else {
        painter.rect_filled(rect, corner(p.radius.min(8)), p.light);
        bevel(painter, rect, false);
    }
}

/// Etched vertical separator, as between toolbar groups.
pub fn separator(ui: &mut Ui, height: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(6.0, height), Sense::hover());
    let x = rect.center().x.round();
    let p = ui.painter();
    let pl = pal();
    p.rect_filled(
        Rect::from_min_max(pos2(x - 1.0, rect.min.y), pos2(x, rect.max.y)),
        0.0,
        pl.shadow,
    );
    if look() == Look::Classic {
        p.rect_filled(
            Rect::from_min_max(pos2(x, rect.min.y), pos2(x + 1.0, rect.max.y)),
            0.0,
            pl.highlight,
        );
    }
}

/// A square toolbar button with a custom-painted icon. Stays pressed while `selected`.
pub fn tool_button(ui: &mut Ui, size: f32, selected: bool, icon: impl FnOnce(&Painter, Rect)) -> Response {
    tool_button_ex(ui, size, selected, true, false, icon)
}

/// A toolbar button that can be greyed out (`enabled` false: the headquarters
/// hasn't unlocked it) or outlined to point the player at it (`suggested`).
pub fn tool_button_ex(
    ui: &mut Ui,
    size: f32,
    selected: bool,
    enabled: bool,
    suggested: bool,
    icon: impl FnOnce(&Painter, Rect),
) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    let pressed = enabled && (selected || response.is_pointer_button_down_on());
    let p = ui.painter();
    let pl = pal();
    fill_face(p, rect, !pressed);
    if selected && look() != Look::Classic {
        p.rect_filled(rect.shrink(2.0), corner(pl.radius.min(8)), pl.accent.gamma_multiply(0.35));
    }
    if selected && look() == Look::Classic {
        // Win95 draws latched buttons with a light dither; a checkerboard of 1px dots.
        let mut y = rect.min.y + 2.0;
        while y < rect.max.y - 2.0 {
            let mut x = rect.min.x + 2.0 + ((y - rect.min.y) as i32 % 2) as f32;
            while x < rect.max.x - 2.0 {
                p.rect_filled(Rect::from_min_size(pos2(x, y), Vec2::splat(1.0)), 0.0, pl.highlight);
                x += 2.0;
            }
            y += 1.0;
        }
    }
    bevel(p, rect, !pressed);
    let offset = if pressed { vec2(1.0, 1.0) } else { Vec2::ZERO };
    icon(p, rect.shrink(5.0).translate(offset));
    if !enabled && look() != Look::Classic {
        p.rect_filled(rect.shrink(1.0), corner(pl.radius.min(8)), pl.face.gamma_multiply(0.7));
    }
    if !enabled && look() == Look::Classic {
        // Win95 greys things out with a dither of white over the icon.
        let mut y = rect.min.y + 3.0;
        while y < rect.max.y - 3.0 {
            let mut x = rect.min.x + 3.0 + ((y - rect.min.y) as i32 % 2) as f32;
            while x < rect.max.x - 3.0 {
                p.rect_filled(Rect::from_min_size(pos2(x, y), Vec2::splat(1.0)), 0.0, pl.face);
                x += 2.0;
            }
            y += 1.0;
        }
    }
    if suggested {
        p.rect_stroke(
            rect.shrink(1.0),
            corner(pl.radius.min(8)),
            Stroke::new(2.0, Color32::from_rgb(0xf0, 0xb0, 0x00)),
            egui::StrokeKind::Inside,
        );
    }
    response
}

/// A sunken pane showing the company's cash; click it for the finance window.
pub fn money_pane(ui: &mut Ui, width: f32, text: &str, in_the_red: bool) -> Response {
    let height = ui.spacing().interact_size.y;
    let (rect, response) = ui.allocate_exact_size(vec2(width, height), Sense::click());
    let p = ui.painter();
    sunken(p, rect);
    p.text(
        pos2(rect.max.x - 6.0, rect.center().y),
        egui::Align2::RIGHT_CENTER,
        text,
        egui::FontId::proportional(13.0),
        if in_the_red { bad() } else { pal().text },
    );
    response
}

/// The Compute meter: the running total, the rate, and a log-axis sparkline of the last months.
pub fn compute_pane(ui: &mut Ui, width: f32, total: &str, rate: &str, spark: &[f32]) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(width, 20.0), Sense::click());
    let p = ui.painter();
    sunken(p, rect);
    let ink = ink();
    p.text(
        pos2(rect.min.x + 6.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        total,
        egui::FontId::proportional(13.0),
        ink,
    );
    // The sparkline: a straight line means exponential growth.
    let area = Rect::from_min_max(pos2(rect.max.x - 50.0, rect.min.y + 4.0), pos2(rect.max.x - 5.0, rect.max.y - 4.0));
    p.text(
        pos2(area.min.x - 6.0, rect.center().y),
        egui::Align2::RIGHT_CENTER,
        rate,
        egui::FontId::proportional(11.0),
        pal().dark,
    );
    if spark.len() >= 2 {
        let top = spark.iter().copied().fold(3.0f32, f32::max);
        let points: Vec<_> = spark
            .iter()
            .enumerate()
            .map(|(i, &v)| {
                pos2(
                    area.min.x + area.width() * i as f32 / (spark.len() - 1) as f32,
                    area.max.y - area.height() * (v / top).clamp(0.0, 1.0),
                )
            })
            .collect();
        p.add(egui::Shape::line(points, Stroke::new(1.5, ink)));
    }
    response
}

/// A ticked or empty box with a line of text beside it: a Win95 check box in Classic, a rounded
/// accent box in the later looks. For checklists that report rather than take input.
pub fn check_row(ui: &mut Ui, done: bool, text: &str) {
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        let (rect, _) = ui.allocate_exact_size(vec2(13.0, 13.0), Sense::hover());
        let rect = rect.translate(vec2(0.0, 2.0));
        let p = ui.painter();
        let pl = pal();
        let tick = |colour: Color32| {
            let at = |x: f32, y: f32| pos2(rect.min.x + x * rect.width(), rect.min.y + y * rect.height());
            p.add(egui::Shape::line(
                vec![at(0.22, 0.52), at(0.42, 0.72), at(0.8, 0.28)],
                Stroke::new(2.0, colour),
            ));
        };
        match look() {
            Look::Classic => {
                p.rect_filled(rect, 0.0, Color32::WHITE);
                edge(p, rect, pl.shadow, pl.highlight);
                edge(p, rect.shrink(1.0), pl.dark, pl.light);
                if done {
                    tick(Color32::BLACK);
                }
            }
            Look::Holo | Look::Ambient => {
                p.rect_stroke(rect, corner(2), Stroke::new(1.0, pl.highlight.gamma_multiply(if done { 1.0 } else { 0.5 })), StrokeKind::Inside);
                if done {
                    p.rect_filled(rect.shrink(2.0), corner(1), pl.highlight.gamma_multiply(0.25));
                    tick(pl.highlight);
                }
            }
            _ => {
                if done {
                    p.rect_filled(rect, corner(4), pl.accent);
                    tick(Color32::WHITE);
                } else {
                    p.rect_filled(rect, corner(4), pl.light);
                    p.rect_stroke(rect, corner(4), Stroke::new(1.0, pl.dark), StrokeKind::Inside);
                }
            }
        }
        let mut label = egui::RichText::new(text);
        if done {
            label = label.color(pl.text.gamma_multiply(0.65));
        }
        ui.add(egui::Label::new(label).wrap());
    });
}

/// A sunken status bar pane holding some text.
pub fn status_pane(ui: &mut Ui, width: f32, text: &str) {
    let height = ui.spacing().interact_size.y;
    let (rect, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    let p = ui.painter();
    sunken(p, rect);
    p.text(
        pos2(rect.min.x + 4.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        text,
        egui::FontId::proportional(12.0),
        pal().text,
    );
}

/// A sunken pane with the date, and a sun or moon showing the time of day.
pub fn clock_pane(ui: &mut Ui, width: f32, text: &str, daylight: f32) {
    let height = ui.spacing().interact_size.y;
    let (rect, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    let p = ui.painter();
    sunken(p, rect);
    let c = pos2(rect.min.x + 12.0, rect.center().y);
    if daylight > 0.5 {
        // A sun: a disc with rays.
        let sun = Color32::from_rgb(0xf2, 0xb8, 0x10);
        p.circle_filled(c, 4.0, sun);
        for i in 0..8 {
            let a = i as f32 * std::f32::consts::FRAC_PI_4;
            let d = vec2(a.cos(), a.sin());
            p.line_segment([c + d * 6.0, c + d * 8.0], egui::Stroke::new(1.0, sun));
        }
    } else {
        // A crescent moon: a pale disc with a bite taken out of it.
        p.circle_filled(c, 6.0, Color32::from_rgb(0x30, 0x40, 0x80));
        p.circle_filled(c + vec2(2.5, -1.5), 5.0, pal().light);
    }
    p.text(
        pos2(rect.min.x + 26.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        text,
        egui::FontId::proportional(12.0),
        pal().text,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_look_follows_the_decade_and_never_goes_backwards() {
        let years = [1999, 2000, 2009, 2010, 2019, 2020, 2029, 2030, 2039, 2040, 2047, 2048, 2050];
        let looks: Vec<Look> = years.iter().map(|&y| Look::for_year(y)).collect();
        assert_eq!(looks[0], Look::Classic);
        assert_eq!(looks[3], Look::Glass);
        assert_eq!(looks[5], Look::Sleek);
        assert_eq!(looks[7], Look::Holo);
        assert_eq!(looks[9], Look::Chrome);
        assert_eq!(*looks.last().unwrap(), Look::Ambient);
        assert!(looks.windows(2).all(|w| w[0].index() <= w[1].index()));
    }

    #[test]
    fn every_look_has_readable_text_on_its_surface() {
        for l in Look::ALL {
            set_look(l);
            let (f, t) = (pal().face, pal().text);
            let lum = |c: Color32| c.r() as i32 * 3 + c.g() as i32 * 6 + c.b() as i32;
            assert!((lum(f) - lum(t)).abs() > 1500, "{} has low contrast", l.name());
        }
        set_look(Look::Classic);
    }
}
