//! The vocabulary of the interface: easing curves, slanted shapes, pictures
//! cut to a polygon, the themes' background motifs, vector icons, titles
//! whose letters come in one after the other. Everything is drawn here, in
//! vector; nothing moves unless something changed.

use crate::theme::{Motif, Theme};
use eframe::egui::{self, Align2, Color32, FontFamily, FontId, Painter, Pos2, Rect, Shape, Stroke, TextureId, Vec2, pos2, vec2};
use std::f32::consts::TAU;

/// The slant of every diagonal (x offset per unit of height).
pub const SLANT: f32 = 0.26;

/// Titles (Anton). Sized as Impact was: Anton's capitals are taller (860 against 791 per
/// 1000), so the size shrinks by 0.92 and the glyphs rise (app.rs) to sit where Impact's did.
pub fn display(size: f32) -> FontId {
    FontId::new(size * 0.92, FontFamily::Name("display".into()))
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
/// convex polygon `points`: cut, never stretched. Where the polygon goes past the picture, nothing
/// is drawn (no smeared edge pixels), and its edges fade over about a pixel, so a slanted cut is
/// as smooth as egui's own shapes (a textured mesh gets no anti-aliasing of its own).
pub fn image_in(painter: &Painter, texture: TextureId, placed: Rect, points: &[Pos2], tint: Color32) {
    let polygon = clip_to_rect(points, placed);
    if polygon.len() < 3 {
        return;
    }
    // Winding, so the normals point outwards.
    let area: f32 = (0..polygon.len()).map(|i| {
        let (a, b) = (polygon[i], polygon[(i + 1) % polygon.len()]);
        a.x * b.y - b.x * a.y
    }).sum();
    let outward = if area > 0.0 { 1.0 } else { -1.0 };
    let normal = |a: Pos2, b: Pos2| {
        let d = (b - a).normalized();
        vec2(d.y, -d.x) * outward
    };
    let feather = 1.0 / painter.ctx().pixels_per_point();
    let uv = |point: Pos2| pos2(((point.x - placed.min.x) / placed.width()).clamp(0.0, 1.0), ((point.y - placed.min.y) / placed.height()).clamp(0.0, 1.0));
    let clear = Color32::from_rgba_premultiplied(0, 0, 0, 0);
    let mut mesh = egui::Mesh::with_texture(texture);
    let count = polygon.len();
    for i in 0..count {
        let (previous, point, next) = (polygon[(i + count - 1) % count], polygon[i], polygon[(i + 1) % count]);
        let (n1, n2) = (normal(previous, point), normal(point, next));
        let mut miter = (n1 + n2).normalized();
        let cos = miter.dot(n1).max(0.3);
        miter /= cos;
        let inner = point - miter * feather * 0.5;
        let outer = point + miter * feather * 0.5;
        mesh.vertices.push(egui::epaint::Vertex { pos: inner, uv: uv(inner), color: tint });
        mesh.vertices.push(egui::epaint::Vertex { pos: outer, uv: uv(point), color: clear });
    }
    let count = count as u32;
    for i in 1..count - 1 {
        mesh.add_triangle(0, 2 * i, 2 * (i + 1));
    }
    for i in 0..count {
        let j = (i + 1) % count;
        mesh.add_triangle(2 * i, 2 * i + 1, 2 * j + 1);
        mesh.add_triangle(2 * i, 2 * j + 1, 2 * j);
    }
    painter.add(Shape::mesh(mesh));
}

/// How strong the glows are (motion::glow): 0 turns every one of them off.
pub const GLOW: f32 = 1.0;

/// A soft light around a convex shape: a ring from `colour` at its edge to nothing `radius`
/// points out (drawn before the shape, which covers its inside).
pub fn glow(painter: &Painter, points: &[Pos2], colour: Color32, radius: f32) {
    if GLOW <= 0.0 || colour.a() == 0 || points.len() < 3 {
        return;
    }
    let colour = Color32::from_rgba_premultiplied(
        (colour.r() as f32 * GLOW).min(255.0) as u8,
        (colour.g() as f32 * GLOW).min(255.0) as u8,
        (colour.b() as f32 * GLOW).min(255.0) as u8,
        (colour.a() as f32 * GLOW).min(255.0) as u8,
    );
    let area: f32 = (0..points.len()).map(|i| {
        let (a, b) = (points[i], points[(i + 1) % points.len()]);
        a.x * b.y - b.x * a.y
    }).sum();
    let outward = if area > 0.0 { 1.0 } else { -1.0 };
    let normal = |a: Pos2, b: Pos2| {
        let d = (b - a).normalized();
        vec2(d.y, -d.x) * outward
    };
    let clear = Color32::from_rgba_premultiplied(0, 0, 0, 0);
    let mut mesh = egui::Mesh::default();
    let count = points.len();
    for i in 0..count {
        let (previous, point, next) = (points[(i + count - 1) % count], points[i], points[(i + 1) % count]);
        let (n1, n2) = (normal(previous, point), normal(point, next));
        let mut miter = (n1 + n2).normalized();
        miter /= miter.dot(n1).max(0.5);
        mesh.colored_vertex(point, colour);
        mesh.colored_vertex(point + miter * radius, clear);
    }
    let count = count as u32;
    for i in 0..count {
        let j = (i + 1) % count;
        mesh.add_triangle(2 * i, 2 * i + 1, 2 * j + 1);
        mesh.add_triangle(2 * i, 2 * j + 1, 2 * j);
    }
    painter.add(Shape::mesh(mesh));
}

/// A glow around a point (a small disc of light fading out).
pub fn glow_dot(painter: &Painter, c: Pos2, size: f32, colour: Color32, radius: f32) {
    let points: Vec<Pos2> = (0..12).map(|k| c + Vec2::angled(k as f32 * std::f32::consts::TAU / 12.0) * size).collect();
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(c, colour);
    for point in &points {
        mesh.colored_vertex(*point, colour);
    }
    for k in 0..12u32 {
        mesh.add_triangle(0, 1 + k, 1 + (k + 1) % 12);
    }
    if GLOW > 0.0 {
        painter.add(Shape::mesh(mesh));
    }
    glow(painter, &points, colour, radius);
}

/// A picture in a slanted frame without cutting it: the frame is filled with the picture's
/// blurred copy (`blur`), and the whole picture lies on it in `upright`, its edges fading into the
/// blur. `crop`: how much larger than whole it may be shown (1: never cut; a very wide picture in a
/// tall frame is better a little cut than lost in blur). `zoom` breathes the backdrop (hover).
#[allow(clippy::too_many_arguments)]
pub fn picture_in(painter: &Painter, texture: TextureId, blur: Option<TextureId>, size: Vec2, shape: &[Pos2], upright: Rect, crop: f32, zoom: f32, tint: Color32) {
    let bounds = Rect::from_points(shape);
    if let Some(blur) = blur {
        let dim = Color32::from_rgba_premultiplied((tint.r() as f32 * 0.85) as u8, (tint.g() as f32 * 0.85) as u8, (tint.b() as f32 * 0.85) as u8, tint.a());
        image_in(painter, blur, cover(size, bounds, zoom * 1.08, Vec2::ZERO), shape, dim);
    }
    let whole = (upright.width() / size.x).min(upright.height() / size.y);
    let full = (upright.width() / size.x).max(upright.height() / size.y);
    let placed = Rect::from_center_size(upright.center(), size * full.min(whole * crop));
    let visible = placed.intersect(upright);
    let fade = (visible.width().min(visible.height()) * 0.07).clamp(2.0, 12.0);
    if blur.is_some() { image_soft(painter, texture, placed, visible, fade, tint) } else { image_in(painter, texture, placed, shape, tint) }
}

/// The part `visible` of a picture laid on `placed`, its edges fading out over `fade` points.
pub fn image_soft(painter: &Painter, texture: TextureId, placed: Rect, visible: Rect, fade: f32, tint: Color32) {
    let xs = [visible.min.x, visible.min.x + fade, visible.max.x - fade, visible.max.x];
    let ys = [visible.min.y, visible.min.y + fade, visible.max.y - fade, visible.max.y];
    let clear = Color32::from_rgba_premultiplied(0, 0, 0, 0);
    let mut mesh = egui::Mesh::with_texture(texture);
    for (j, y) in ys.iter().enumerate() {
        for (i, x) in xs.iter().enumerate() {
            let inside = (1..=2).contains(&i) && (1..=2).contains(&j);
            let uv = pos2((x - placed.min.x) / placed.width(), (y - placed.min.y) / placed.height());
            mesh.vertices.push(egui::epaint::Vertex { pos: pos2(*x, *y), uv, color: if inside { tint } else { clear } });
        }
    }
    for j in 0..3u32 {
        for i in 0..3u32 {
            let a = j * 4 + i;
            mesh.add_triangle(a, a + 1, a + 5);
            mesh.add_triangle(a, a + 5, a + 4);
        }
    }
    painter.add(Shape::mesh(mesh));
}

/// The upright part of a slanted rectangle (`slanted(rect, slant)`): a thumbnail covering it is
/// never cut by the slant, which leaves the frame's colour in its two corners instead.
pub fn upright(rect: Rect, slant: f32) -> Rect {
    rect.shrink2(vec2(rect.height() * slant / 2.0, 0.0))
}

/// A convex polygon cut by a rectangle (Sutherland–Hodgman), without repeated points.
fn clip_to_rect(points: &[Pos2], rect: Rect) -> Vec<Pos2> {
    let mut polygon = points.to_vec();
    let edges: [(fn(Pos2, Rect) -> f32, ); 4] = [
        (|p, r| p.x - r.min.x,),
        (|p, r| r.max.x - p.x,),
        (|p, r| p.y - r.min.y,),
        (|p, r| r.max.y - p.y,),
    ];
    for (inside,) in edges {
        if polygon.is_empty() {
            break;
        }
        let input = std::mem::take(&mut polygon);
        for i in 0..input.len() {
            let (a, b) = (input[i], input[(i + 1) % input.len()]);
            let (da, db) = (inside(a, rect), inside(b, rect));
            if da >= 0.0 {
                polygon.push(a);
            }
            if (da >= 0.0) != (db >= 0.0) {
                polygon.push(a + (b - a) * (da / (da - db)));
            }
        }
    }
    polygon.dedup_by(|a, b| a.distance(*b) < 0.05);
    while polygon.len() > 1 && polygon[0].distance(polygon[polygon.len() - 1]) < 0.05 {
        polygon.pop();
    }
    polygon
}

/// The theme's motif, faint, over `rect`. `shift`: parallax during transitions.
pub fn motif(painter: &Painter, rect: Rect, theme: &Theme, strength: f32, shift: f32) {
    let ink = alpha(theme.accent, 0.07 * strength);
    let faint = alpha(theme.accent, 0.035 * strength);
    // The motif drifts slowly with the clock (AMBIENT), its slanted stripes behind it.
    let clock = ambient_clock(painter);
    drift(painter, rect, theme, strength, clock);
    let spin = clock * 0.04;
    let shift = shift + clock * 10.0;
    match theme.motif {
        Motif::Orbits => {
            // The orbment's rings, off to the right, and its seven slots.
            let center = pos2(rect.max.x - rect.width() * 0.18 + (shift - clock * 10.0), rect.min.y + rect.height() * 0.42);
            for (index, radius) in [0.22, 0.34, 0.47, 0.62].iter().enumerate() {
                let r = rect.height() * radius;
                painter.circle_stroke(center, r, Stroke::new(if index == 1 { 2.0 } else { 1.0 }, if index % 2 == 0 { ink } else { faint }));
            }
            let r = rect.height() * 0.34;
            for slot in 0..7 {
                let a = slot as f32 * TAU / 7.0 - TAU / 4.0 + spin;
                painter.circle_filled(center + vec2(a.cos(), a.sin()) * r, 5.0, ink);
            }
        }
        Motif::Rays => {
            let origin = pos2(rect.max.x + 80.0 + (shift - clock * 10.0), rect.min.y - 60.0);
            for ray in 0..14 {
                let a = TAU * 0.25 + ray as f32 * 0.075 + (spin * 2.0).sin() * 0.03;
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
                let mut x = rect.min.x + rect.width() * 0.45 + if row % 2 == 0 { 0.0 } else { step / 2.0 } + (shift % step);
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

/// The ambient motion (drifting motifs, passing shines): 0 stills them all.
pub const AMBIENT: f32 = 1.0;

/// Seconds for the ambient motion, asking for the next frame (about 30 a second) while it runs.
fn ambient_clock(painter: &Painter) -> f32 {
    if AMBIENT <= 0.0 {
        return 0.0;
    }
    painter.ctx().request_repaint_after(std::time::Duration::from_millis(33));
    painter.ctx().input(|input| input.time) as f32 * AMBIENT
}

/// Thin slanted stripes across the ground, very faint, sliding slowly to the right.
fn drift(painter: &Painter, rect: Rect, theme: &Theme, strength: f32, clock: f32) {
    if AMBIENT <= 0.0 {
        return;
    }
    let colour = alpha(theme.accent, 0.022 * strength);
    let step = 34.0;
    let height = rect.height();
    let s = height * SLANT * 2.0;
    let mut x = rect.min.x - s - step + (clock * 14.0) % step;
    while x < rect.max.x + step {
        painter.add(Shape::convex_polygon(vec![pos2(x + s, rect.min.y), pos2(x + s + 7.0, rect.min.y), pos2(x + 7.0, rect.max.y), pos2(x, rect.max.y)], colour, Stroke::NONE));
        x += step;
    }
}

/// A convex polygon cut by another convex polygon (Sutherland–Hodgman).
fn clip_convex(subject: &[Pos2], clip: &[Pos2]) -> Vec<Pos2> {
    let area: f32 = (0..clip.len()).map(|i| {
        let (a, b) = (clip[i], clip[(i + 1) % clip.len()]);
        a.x * b.y - b.x * a.y
    }).sum();
    let sign = if area > 0.0 { 1.0 } else { -1.0 };
    let mut polygon = subject.to_vec();
    for i in 0..clip.len() {
        if polygon.is_empty() {
            break;
        }
        let (e0, e1) = (clip[i], clip[(i + 1) % clip.len()]);
        let inside = |p: Pos2| sign * ((e1.x - e0.x) * (p.y - e0.y) - (e1.y - e0.y) * (p.x - e0.x));
        let input = std::mem::take(&mut polygon);
        for j in 0..input.len() {
            let (a, b) = (input[j], input[(j + 1) % input.len()]);
            let (da, db) = (inside(a), inside(b));
            if da >= 0.0 {
                polygon.push(a);
            }
            if (da >= 0.0) != (db >= 0.0) {
                polygon.push(a + (b - a) * (da / (da - db)));
            }
        }
    }
    polygon
}

/// A shine running across a coloured shape now and then: a soft slanted band of light, once
/// every `period` seconds, `phase` setting when (so shapes do not all shine together).
pub fn shine(painter: &Painter, shape: &[Pos2], period: f32, phase: f32, strength: f32) {
    if AMBIENT <= 0.0 || strength <= 0.0 || shape.len() < 3 {
        return;
    }
    let clock = ambient_clock(painter);
    let t = ((clock + phase) % period) / 0.9;
    if t >= 1.0 {
        return;
    }
    let bounds = Rect::from_points(shape);
    let lean = bounds.height() * 0.6;
    let centre = lerp(bounds.min.x - 50.0 - lean, bounds.max.x + 50.0, ease_in_out(t));
    for (half, light) in [(26.0, 0.05), (16.0, 0.07), (9.0, 0.09), (3.5, 0.12)] {
        let band = [pos2(centre - half + lean, bounds.min.y), pos2(centre + half + lean, bounds.min.y), pos2(centre + half, bounds.max.y), pos2(centre - half, bounds.max.y)];
        let cut = clip_convex(&band, shape);
        if cut.len() >= 3 {
            painter.add(Shape::convex_polygon(cut, alpha(Color32::WHITE, light * strength), Stroke::NONE));
        }
    }
}

/// A word whose letters rise one after the other from under a mask.
/// `t` is the time since the reveal began (seconds), `out` makes them leave upwards.
pub fn letters(painter: &Painter, at: Pos2, text: &str, font: FontId, colour: Color32, t: f32, stagger: f32, out: bool) -> f32 {
    letters_ringed(painter, at, text, font, colour, None, t, stagger, out)
}

/// Titles in white with a coloured outline (an essay): false keeps them in the ink colour.
pub const OUTLINED_TITLES: bool = true;

/// How thick a title's outline is for its size.
pub fn outline_width(font: &FontId) -> f32 {
    (font.size * 0.04).clamp(2.0, 4.0)
}

/// A title's letters (letters) with the essay's colours: white in a ring of `ring`, or `ink`.
pub fn title_letters(painter: &Painter, at: Pos2, text: &str, font: FontId, ink: Color32, ring: Color32, shadow: Color32, t: f32, stagger: f32) {
    if OUTLINED_TITLES {
        let width = outline_width(&font);
        letters_ringed(painter, at + vec2(4.0, 4.0) + vec2(width, width) * 0.5, text, font.clone(), shadow, Some((shadow, width)), t, stagger, false);
        letters_ringed(painter, at, text, font, Color32::WHITE, Some((ring, width)), t, stagger, false);
    } else {
        letters(painter, at + vec2(4.0, 4.0), text, font.clone(), shadow, t, stagger, false);
        letters(painter, at, text, font, ink, t, stagger, false);
    }
}

/// letters, each letter in a ring of (colour, width) drawn under it when one is given.
pub fn letters_ringed(painter: &Painter, at: Pos2, text: &str, font: FontId, colour: Color32, ring: Option<(Color32, f32)>, t: f32, stagger: f32, out: bool) -> f32 {
    let height = font.size * 1.1;
    let pad = ring.map_or(0.0, |(_, width)| width);
    let clip = Rect::from_min_size(pos2(at.x - 4.0 - pad, at.y - height * 0.08 - pad), vec2(4000.0, height * 1.12 + 2.0 * pad));
    let painter = painter.with_clip_rect(painter.clip_rect().intersect(clip));
    // Each letter where the whole line puts it (kerning included), so the line drawn whole
    // once the letters are in place does not shift by a hair.
    let line = painter.layout_no_wrap(text.to_string(), font.clone(), colour);
    let glyphs = line.rows.first().map(|row| row.glyphs.clone()).unwrap_or_default();
    for (index, glyph) in glyphs.iter().enumerate() {
        let local = if out { clamp01(t / 0.18) } else { clamp01((t - index as f32 * stagger) / 0.42) };
        let offset = if out { -ease_in_out(local) * height * 1.4 } else { (1.0 - ease_out_quint(local)) * height };
        // The last pixels of the ease are not crawled: the letter lands, whole pixels only.
        let offset = if out { offset.round() } else if offset < 2.5 { 0.0 } else { offset.round() };
        if (out || local > 0.0) && !glyph.chr.is_whitespace() {
            let place = pos2(at.x + glyph.pos.x, at.y + offset);
            if let Some((ring_colour, width)) = ring {
                let galley = painter.layout_no_wrap(glyph.chr.to_string(), font.clone(), ring_colour);
                for k in 0..16 {
                    let around = Vec2::angled(k as f32 * std::f32::consts::TAU / 16.0) * width;
                    painter.galley(place + around, galley.clone(), ring_colour);
                }
            }
            let galley = painter.layout_no_wrap(glyph.chr.to_string(), font.clone(), colour);
            painter.galley(place, galley, colour);
        }
    }
    line.size().x
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
    Help,
}

/// A filled outline seen whole from its centre (fan of triangles), with a thin stroke of the same
/// colour around it for a smooth edge.
fn silhouette(painter: &Painter, c: Pos2, points: &[Pos2], colour: Color32) {
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(c, colour);
    for point in points {
        mesh.colored_vertex(*point, colour);
    }
    let n = points.len() as u32;
    for i in 0..n {
        mesh.add_triangle(0, 1 + i, 1 + (i + 1) % n);
    }
    painter.add(Shape::mesh(mesh));
    painter.add(Shape::closed_line(points.to_vec(), Stroke::new(0.8, colour)));
}

/// An icon on a background of colour `back` (the logos cut holes in themselves with it).
pub fn icon_on(painter: &Painter, icon: Icon, c: Pos2, s: f32, colour: Color32, back: Color32) {
    let h = s / 2.0;
    match icon {
        Icon::Discord => {
            // Clyde: the horns, the round cheeks, the feet, and the eyes.
            const OUTLINE: [(f32, f32); 26] = [
                (-0.62, -0.74), (-0.38, -0.63), (-0.14, -0.57), (0.14, -0.57), (0.38, -0.63), (0.62, -0.74),
                (0.80, -0.50), (0.93, -0.15), (1.0, 0.22), (0.98, 0.50), (0.86, 0.62), (0.70, 0.74),
                (0.58, 0.56), (0.48, 0.44), (0.22, 0.50), (0.0, 0.51), (-0.22, 0.50), (-0.48, 0.44),
                (-0.58, 0.56), (-0.70, 0.74), (-0.86, 0.62), (-0.98, 0.50), (-1.0, 0.22), (-0.93, -0.15),
                (-0.80, -0.50), (-0.70, -0.64),
            ];
            let points: Vec<Pos2> = OUTLINE.iter().map(|(x, y)| c + vec2(x * h, y * h)).collect();
            silhouette(painter, c, &points, colour);
            for side in [-1.0, 1.0] {
                painter.add(Shape::ellipse_filled(c + vec2(side * 0.36 * h, 0.06 * h), vec2(0.16 * h, 0.19 * h), back));
            }
        }
        Icon::X => {
            // The thick stroke is hollow, the thin one passes under it.
            let thin = Stroke::new((s / 9.0).max(1.4), colour);
            painter.line_segment([c + vec2(0.80 * h, -0.92 * h), c + vec2(-0.80 * h, 0.92 * h)], thin);
            let band = vec![c + vec2(-0.92 * h, -0.92 * h), c + vec2(-0.40 * h, -0.92 * h), c + vec2(0.92 * h, 0.92 * h), c + vec2(0.40 * h, 0.92 * h)];
            painter.add(Shape::convex_polygon(band.clone(), back, Stroke::new((s / 11.0).max(1.2), colour)));
        }
        _ => self::icon(painter, icon, c, s, colour),
    }
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
        Icon::Help => {
            painter.circle_stroke(c, h, stroke);
            painter.text(c + vec2(0.0, s * 0.03), Align2::CENTER_CENTER, "?", semibold(s * 0.78), colour);
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
