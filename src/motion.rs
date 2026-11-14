//! The vocabulary of the interface: easing curves, slanted shapes, pictures
//! cut to a polygon, the themes' background motifs, vector icons, titles
//! whose letters come in one after the other. Everything is drawn here, in
//! vector; nothing moves unless something changed.

use crate::theme::{Motif, Theme};
use eframe::egui::{self, Align2, Color32, FontFamily, FontId, Painter, Pos2, Rect, Shape, Stroke, TextureId, Vec2, pos2, vec2};
use std::f32::consts::TAU;

/// The slant of every diagonal (x offset per unit of height).
pub const SLANT: f32 = 0.26;

pub fn display(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("display".into()))
}
pub fn semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("semibold".into()))
}
pub fn editorial(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("editorial".into()))
}
pub fn body(size: f32) -> FontId {
    FontId::proportional(size)
}

/// Whether Windows asks for fewer animations ("Afficher les animations dans Windows" off).
#[cfg(windows)]
pub fn reduced_motion() -> bool {
    #[link(name = "user32")]
    unsafe extern "system" {
        fn SystemParametersInfoW(action: u32, param: u32, value: *mut core::ffi::c_void, ini: u32) -> i32;
    }
    const SPI_GETCLIENTAREAANIMATION: u32 = 0x1042;
    let mut enabled: i32 = 1;
    // SAFETY: the call writes one BOOL into `enabled`.
    let ok = unsafe { SystemParametersInfoW(SPI_GETCLIENTAREAANIMATION, 0, (&mut enabled as *mut i32).cast(), 0) };
    ok != 0 && enabled == 0
}
#[cfg(not(windows))]
pub fn reduced_motion() -> bool {
    false
}

pub fn clamp01(t: f32) -> f32 {
    t.clamp(0.0, 1.0)
}
pub fn ease_out(t: f32) -> f32 {
    1.0 - (1.0 - clamp01(t)).powi(3)
}
pub fn ease_out_quint(t: f32) -> f32 {
    1.0 - (1.0 - clamp01(t)).powi(5)
}
pub fn ease_in_out(t: f32) -> f32 {
    let t = clamp01(t);
    if t < 0.5 { 4.0 * t * t * t } else { 1.0 - (-2.0 * t + 2.0).powi(3) / 2.0 }
}
/// Overshoots a little before settling (titles, success mark).
pub fn ease_back(t: f32) -> f32 {
    let t = clamp01(t);
    let c = 1.70158;
    1.0 + (c + 1.0) * (t - 1.0).powi(3) + c * (t - 1.0).powi(2)
}

pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = clamp01(t);
    let channel = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgba_unmultiplied(channel(a.r(), b.r()), channel(a.g(), b.g()), channel(a.b(), b.b()), channel(a.a(), b.a()))
}
pub fn alpha(colour: Color32, a: f32) -> Color32 {
    let [r, g, b, _] = colour.to_srgba_unmultiplied();
    Color32::from_rgba_unmultiplied(r, g, b, (clamp01(a) * 255.0) as u8)
}
pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}
pub fn lerp_pos(a: Pos2, b: Pos2, t: f32) -> Pos2 {
    pos2(lerp(a.x, b.x, t), lerp(a.y, b.y, t))
}
pub fn lerp_rect(a: Rect, b: Rect, t: f32) -> Rect {
    Rect::from_min_max(lerp_pos(a.min, b.min, t), lerp_pos(a.max, b.max, t))
}

/// A blended theme, while one gives way to the other.
pub fn mix_theme(a: &Theme, b: &Theme, t: f32) -> Theme {
    Theme {
        bg: mix(a.bg, b.bg, t),
        accent: mix(a.accent, b.accent, t),
        accent2: mix(a.accent2, b.accent2, t),
        ink: mix(a.ink, b.ink, t),
        muted: mix(a.muted, b.muted, t),
        ..if t < 0.5 { *a } else { *b }
    }
}

/// A rectangle leaning right: its top edge shifted by `slant` × height.
pub fn slanted(rect: Rect, slant: f32) -> [Pos2; 4] {
    let shift = rect.height() * slant / 2.0;
    [
        pos2(rect.min.x + shift, rect.min.y),
        pos2(rect.max.x + shift, rect.min.y),
        pos2(rect.max.x - shift, rect.max.y),
        pos2(rect.min.x - shift, rect.max.y),
    ]
}

pub fn fill(painter: &Painter, points: &[Pos2], colour: Color32) {
    painter.add(Shape::convex_polygon(points.to_vec(), colour, Stroke::NONE));
}

/// A polygon filled with a gradient from `from` (first edge side) to `to`, horizontally.
pub fn gradient_rect(painter: &Painter, rect: Rect, left: Color32, right: Color32, vertical: bool) {
    let mut mesh = egui::Mesh::default();
    let (a, b, c, d) = if vertical { (left, left, right, right) } else { (left, right, right, left) };
    mesh.colored_vertex(rect.left_top(), a);
    mesh.colored_vertex(rect.right_top(), b);
    mesh.colored_vertex(rect.right_bottom(), c);
    mesh.colored_vertex(rect.left_bottom(), d);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    painter.add(Shape::mesh(mesh));
}

/// The part of a picture of `size` that covers `area` (no distortion), as a screen rectangle,
/// zoomed by `zoom` and moved by `pan` (fractions of the area).
pub fn cover(size: Vec2, area: Rect, zoom: f32, pan: Vec2) -> Rect {
    let scale = (area.width() / size.x).max(area.height() / size.y) * zoom;
    let shown = size * scale;
    Rect::from_center_size(area.center() + vec2(pan.x * area.width(), pan.y * area.height()), shown)
}

/// The picture laid on `placed` (screen rectangle of the whole picture), shown only inside the
/// convex polygon `points`: cut, never stretched.
pub fn image_in(painter: &Painter, texture: TextureId, placed: Rect, points: &[Pos2], tint: Color32) {
    let mut mesh = egui::Mesh::with_texture(texture);
    for point in points {
        let uv = pos2((point.x - placed.min.x) / placed.width(), (point.y - placed.min.y) / placed.height());
        mesh.vertices.push(egui::epaint::Vertex { pos: *point, uv, color: tint });
    }
    for index in 1..points.len().saturating_sub(1) as u32 {
        mesh.add_triangle(0, index, index + 1);
    }
    painter.add(Shape::mesh(mesh));
}

/// The theme's motif, faint, over `rect`. `shift`: parallax during transitions.
pub fn motif(painter: &Painter, rect: Rect, theme: &Theme, strength: f32, shift: f32) {
    let ink = alpha(theme.accent, 0.07 * strength);
    let faint = alpha(theme.accent, 0.035 * strength);
    match theme.motif {
        Motif::Orbits => {
            // The orbment's rings, off to the right, and its seven slots.
            let center = pos2(rect.max.x - rect.width() * 0.18 + shift, rect.min.y + rect.height() * 0.42);
            for (index, radius) in [0.22, 0.34, 0.47, 0.62].iter().enumerate() {
                let r = rect.height() * radius;
                painter.circle_stroke(center, r, Stroke::new(if index == 1 { 2.0 } else { 1.0 }, if index % 2 == 0 { ink } else { faint }));
            }
            let r = rect.height() * 0.34;
            for slot in 0..7 {
                let a = slot as f32 * TAU / 7.0 - TAU / 4.0 + shift * 0.002;
                painter.circle_filled(center + vec2(a.cos(), a.sin()) * r, 5.0, ink);
            }
        }
        Motif::Rays => {
            let origin = pos2(rect.max.x + 80.0 + shift, rect.min.y - 60.0);
            for ray in 0..14 {
                let a = TAU * 0.25 + ray as f32 * 0.075;
                let end = origin + vec2(a.cos(), a.sin()) * rect.width() * 1.6;
                painter.line_segment([origin, end], Stroke::new(if ray % 3 == 0 { 2.0 } else { 1.0 }, if ray % 2 == 0 { ink } else { faint }));
            }
        }
        Motif::Grid => {
            let step = 46.0;
            let height = rect.height();
            let mut x = rect.min.x - height * SLANT - step + (shift % step);
            while x < rect.max.x + step {
                painter.line_segment([pos2(x + height * SLANT, rect.min.y), pos2(x, rect.max.y)], Stroke::new(1.0, faint));
                x += step;
            }
            let mut y = rect.min.y + step;
            while y < rect.max.y {
                painter.line_segment([pos2(rect.min.x, y), pos2(rect.max.x, y)], Stroke::new(1.0, faint));
                y += step;
            }
        }
        Motif::Scanlines => {
            let line = alpha(Color32::BLACK, 0.22 * strength);
            let mut y = rect.min.y;
            while y < rect.max.y {
                painter.line_segment([pos2(rect.min.x, y), pos2(rect.max.x, y)], Stroke::new(1.0, line));
                y += 3.0;
            }
            painter.rect_filled(Rect::from_min_size(pos2(rect.min.x, rect.min.y + (shift.abs() * 3.0) % rect.height()), vec2(rect.width(), 2.0)), 0.0, faint);
        }
        Motif::Dots => {
            // A band of halftone dots, growing towards the right.
            let step = 18.0;
            let mut y = rect.min.y + step;
            let mut row = 0;
            while y < rect.max.y {
                let mut x = rect.min.x + rect.width() * 0.45 + if row % 2 == 0 { 0.0 } else { step / 2.0 } + shift;
                while x < rect.max.x + step {
                    let t = clamp01((x - rect.min.x - rect.width() * 0.45) / (rect.width() * 0.55));
                    painter.circle_filled(pos2(x, y), 0.6 + 3.2 * t, ink);
                    x += step;
                }
                y += step;
                row += 1;
            }
        }
    }
}

/// A word whose letters rise one after the other from under a mask.
/// `t` is the time since the reveal began (seconds), `out` makes them leave upwards.
pub fn letters(painter: &Painter, at: Pos2, text: &str, font: FontId, colour: Color32, t: f32, stagger: f32, out: bool) -> f32 {
    let height = font.size * 1.1;
    let clip = Rect::from_min_size(pos2(at.x - 4.0, at.y - height * 0.08), vec2(4000.0, height * 1.12));
    let painter = painter.with_clip_rect(painter.clip_rect().intersect(clip));
    let mut x = at.x;
    for (index, character) in text.chars().enumerate() {
        let galley = painter.layout_no_wrap(character.to_string(), font.clone(), colour);
        let local = if out { clamp01(t / 0.18) } else { clamp01((t - index as f32 * stagger) / 0.42) };
        let offset = if out { -ease_in_out(local) * height } else { (1.0 - ease_out_quint(local)) * height };
        if out || local > 0.0 {
            painter.galley(pos2(x, at.y + offset), galley.clone(), colour);
        }
        x += galley.size().x;
    }
    x - at.x
}

/// How wide a text is in a font.
pub fn width(painter: &Painter, text: &str, font: &FontId) -> f32 {
    painter.layout_no_wrap(text.to_string(), font.clone(), Color32::WHITE).size().x
}

/// Words broken into lines no wider than `max`.
pub fn wrap(painter: &Painter, text: &str, font: &FontId, max: f32) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for word in text.split_whitespace() {
        match lines.last_mut() {
            Some(line) if width(painter, &format!("{line} {word}"), font) <= max => {
                line.push(' ');
                line.push_str(word);
            }
            _ => lines.push(word.to_string()),
        }
    }
    lines
}

/// Text with a hard offset shadow, Persona-like.
pub fn shadowed(painter: &Painter, at: Pos2, align: Align2, text: &str, font: FontId, colour: Color32, shadow: Color32, offset: f32) -> Rect {
    painter.text(at + vec2(offset, offset), align, text, font.clone(), shadow);
    painter.text(at, align, text, font, colour)
}

#[derive(Clone, Copy)]
pub enum Icon {
    Globe,
    Discord,
    X,
    Folder,
    FolderEdit,
    Trash,
    Play,
    Grid,
    Wheel,
    Search,
    Left,
    Right,
    Close,
}

/// A small vector icon centred on `c`, `s` pixels wide.
pub fn icon(painter: &Painter, icon: Icon, c: Pos2, s: f32, colour: Color32) {
    let stroke = Stroke::new((s / 11.0).max(1.4), colour);
    let h = s / 2.0;
    match icon {
        Icon::Globe => {
            painter.circle_stroke(c, h, stroke);
            painter.line_segment([c - vec2(h, 0.0), c + vec2(h, 0.0)], stroke);
            painter.add(Shape::ellipse_stroke(c, vec2(h * 0.45, h), stroke));
        }
        Icon::Discord => {
            let body = Rect::from_center_size(c + vec2(0.0, h * 0.05), vec2(s, s * 0.72));
            painter.rect_stroke(body, h * 0.45, stroke, egui::StrokeKind::Middle);
            painter.circle_filled(c + vec2(-h * 0.36, h * 0.05), s * 0.09, colour);
            painter.circle_filled(c + vec2(h * 0.36, h * 0.05), s * 0.09, colour);
        }
        Icon::X => {
            painter.line_segment([c + vec2(-h * 0.8, -h), c + vec2(h * 0.8, h)], Stroke::new(stroke.width * 1.4, colour));
            painter.line_segment([c + vec2(h * 0.8, -h), c + vec2(-h * 0.8, h)], stroke);
        }
        Icon::Folder | Icon::FolderEdit => {
            let points = vec![
                c + vec2(-h, -h * 0.7),
                c + vec2(-h * 0.2, -h * 0.7),
                c + vec2(0.0, -h * 0.45),
                c + vec2(h, -h * 0.45),
                c + vec2(h, h * 0.75),
                c + vec2(-h, h * 0.75),
            ];
            painter.add(Shape::closed_line(points, stroke));
            if matches!(icon, Icon::FolderEdit) {
                painter.line_segment([c + vec2(-h * 0.35, h * 0.45), c + vec2(h * 0.4, -h * 0.25)], Stroke::new(stroke.width * 1.3, colour));
            }
        }
        Icon::Trash => {
            painter.line_segment([c + vec2(-h, -h * 0.6), c + vec2(h, -h * 0.6)], stroke);
            painter.line_segment([c + vec2(-h * 0.3, -h * 0.85), c + vec2(h * 0.3, -h * 0.85)], stroke);
            painter.add(Shape::closed_line(vec![c + vec2(-h * 0.75, -h * 0.6), c + vec2(h * 0.75, -h * 0.6), c + vec2(h * 0.6, h), c + vec2(-h * 0.6, h)], stroke));
            for x in [-0.25, 0.25] {
                painter.line_segment([c + vec2(h * x, -h * 0.25), c + vec2(h * x, h * 0.65)], stroke);
            }
        }
        Icon::Play => fill(painter, &[c + vec2(-h * 0.6, -h * 0.8), c + vec2(h * 0.85, 0.0), c + vec2(-h * 0.6, h * 0.8)], colour),
        Icon::Grid => {
            for (dx, dy) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
                painter.rect_filled(Rect::from_center_size(c + vec2(dx * h * 0.5, dy * h * 0.5), vec2(h * 0.75, h * 0.75)), 1.5, colour);
            }
        }
        Icon::Wheel => {
            fill(painter, &slanted(Rect::from_center_size(c, vec2(s * 0.5, s * 0.85)), SLANT), colour);
            fill(painter, &slanted(Rect::from_center_size(c + vec2(-h * 0.85, 0.0), vec2(s * 0.18, s * 0.6)), SLANT), alpha(colour, 0.6));
            fill(painter, &slanted(Rect::from_center_size(c + vec2(h * 0.85, 0.0), vec2(s * 0.18, s * 0.6)), SLANT), alpha(colour, 0.6));
        }
        Icon::Search => {
            painter.circle_stroke(c + vec2(-h * 0.15, -h * 0.15), h * 0.6, stroke);
            painter.line_segment([c + vec2(h * 0.3, h * 0.3), c + vec2(h * 0.9, h * 0.9)], Stroke::new(stroke.width * 1.3, colour));
        }
        Icon::Left | Icon::Right => {
            let d = if matches!(icon, Icon::Left) { -1.0 } else { 1.0 };
            painter.add(Shape::line(vec![c + vec2(-d * h * 0.35, -h * 0.8), c + vec2(d * h * 0.45, 0.0), c + vec2(-d * h * 0.35, h * 0.8)], Stroke::new(stroke.width * 1.5, colour)));
        }
        Icon::Close => {
            painter.line_segment([c + vec2(-h * 0.7, -h * 0.7), c + vec2(h * 0.7, h * 0.7)], stroke);
            painter.line_segment([c + vec2(h * 0.7, -h * 0.7), c + vec2(-h * 0.7, h * 0.7)], stroke);
        }
    }
}

/// A check mark drawn progressively (t from 0 to 1).
pub fn check_mark(painter: &Painter, c: Pos2, h: f32, t: f32, stroke: Stroke) {
    let a = c + vec2(-h * 0.7, 0.0);
    let b = c + vec2(-h * 0.15, h * 0.55);
    let d = c + vec2(h * 0.8, -h * 0.6);
    let first = clamp01(t / 0.4);
    painter.line_segment([a, lerp_pos(a, b, first)], stroke);
    if t > 0.4 {
        painter.line_segment([b, lerp_pos(b, d, clamp01((t - 0.4) / 0.6))], stroke);
    }
}

/// Diagonal stripes moving inside `rect` (progress bars).
pub fn stripes(painter: &Painter, rect: Rect, colour: Color32, phase: f32) {
    let painter = painter.with_clip_rect(painter.clip_rect().intersect(rect));
    let step = 14.0;
    let mut x = rect.min.x - rect.height() - step + (phase * step) % step;
    while x < rect.max.x + step {
        fill(&painter, &[pos2(x, rect.max.y), pos2(x + step * 0.5, rect.max.y), pos2(x + step * 0.5 + rect.height(), rect.min.y), pos2(x + rect.height(), rect.min.y)], colour);
        x += step;
    }
}
