//! A project's page. The picture flown from the home becomes the hero (cut by
//! a diagonal, it shrinks as the page scrolls); under it, two tabs. The
//! overview reveals its parts one after the other: the description, the
//! release notes, the gallery, the reviews; on the right, the patch: its
//! version and the button that turns into the progress bar, then into the
//! success mark, then back into a button.

#![allow(deprecated)]

use crate::app::{self, App, OPEN_TIME, Tab, open_url, shorten, slant_button, spaced, spaced_width};
use crate::catalog::{self, Project};
use crate::detect::{self, Store};
use crate::games::Credit;
use crate::motion::{self, Icon, SLANT, alpha, clamp01, ease_back, ease_in_out, ease_out, ease_out_quint, fill, lerp, mix, slanted};
use crate::patch::{self, Installed};
use crate::reviews::Reviews;
use crate::theme::Theme;
use eframe::egui::{self, Align2, Color32, CursorIcon, Id, Order, Pos2, Rect, RichText, Sense, Stroke, UiBuilder, Vec2, pos2, vec2};

const HERO: f32 = 380.0;
const HERO_SMALL: f32 = 200.0;
const TABS: f32 = 54.0;
/// The scroll where the hero has finished shrinking: the gallery then sits right under the tabs.
/// What the strip of thumbnails under the big view takes, and the air kept under it.
const STRIP: f32 = 96.0 + 20.0;
const SIDE: f32 = 420.0;
const MARGIN: f32 = 56.0;
const RED: Color32 = Color32::from_rgb(0xff, 0x6b, 0x5e);

enum Media {
    Video(String),
    Image(String),
}

fn media(project: &Project) -> Vec<Media> {
    project.youtube.iter().map(|id| Media::Video(id.clone())).chain(project.images.iter().map(|url| Media::Image(url.clone()))).collect()
}

impl Media {
    fn picture(&self) -> String {
        match self {
            Media::Video(id) => format!("https://i.ytimg.com/vi/{id}/hqdefault.jpg"),
            Media::Image(url) => url.clone(),
        }
    }
}

fn hero_height(app: &App) -> f32 {
    let scroll = app.page.as_ref().map(|page| page.scroll).unwrap_or(0.0);
    // Follows the scroll exactly: the content never leaves a gap under the tabs.
    (HERO - scroll).max(HERO_SMALL)
}

/// The hero's polygon (its bottom edge slanted) and where its whole picture lies.
pub fn hero_geometry(app: &App, index: usize) -> ([Pos2; 4], Rect) {
    let screen = app.screen;
    let height = hero_height(app);
    let area = Rect::from_min_size(screen.min, vec2(screen.width(), height));
    let poly = [area.min, pos2(area.max.x, area.min.y), pos2(area.max.x, area.max.y - 58.0), pos2(area.min.x, area.max.y)];
    let project = &app.catalog.projects[index];
    let size = project.key_art().and_then(|url| app.images.size(&url)).or_else(|| project.thumbnail().and_then(|url| app.images.size(&url))).unwrap_or(vec2(1920.0, 620.0));
    // Covers the tallest hero, so the picture does not jump when the hero shrinks.
    let full = Rect::from_min_size(screen.min, vec2(screen.width(), HERO));
    (poly, motion::cover(size, full, 1.0, vec2(0.0, -0.08)))
}

pub fn show(app: &mut App, ui: &mut egui::Ui, screen: Rect, theme: &Theme) {
    let index = app.page.as_ref().map(|page| page.project).unwrap_or(0);
    let project = app.catalog.projects[index].clone();
    let opened = app.page.as_ref().map(|page| page.opened).unwrap_or(0.0);
    let t = app.since(opened, 1.8);

    // The content scrolls under the hero.
    let area = Rect::from_min_max(screen.min, screen.max);
    let mut child = ui.new_child(UiBuilder::new().max_rect(area));
    let (tab, tab_changed, tab_direction) = app.page.as_ref().map(|page| (page.tab, page.tab_changed, page.tab_direction)).unwrap_or((Tab::Overview, 0.0, 1.0));
    let switch = ease_out_quint(app.progress(tab_changed, 0.35));
    // The page scrolls itself: the wheel counts wherever the mouse is (hero, top bar,
    // content), a little faster than egui's, eased towards where it is going.
    let blocked = app.page.as_ref().is_none_or(|page| page.lightbox.is_some() || page.confirm.is_some() || page.floppies.is_some());
    let wheel: f32 = app.player.take_wheel() + if blocked {
        0.0
    } else {
        ui.ctx().input(|input| {
            input
                .raw
                .events
                .iter()
                .map(|event| match event {
                    egui::Event::MouseWheel { unit, delta, .. } => match unit {
                        egui::MouseWheelUnit::Point => delta.y,
                        egui::MouseWheelUnit::Line => delta.y * 60.0,
                        egui::MouseWheelUnit::Page => delta.y * screen.height(),
                    },
                    _ => 0.0,
                })
                .sum()
        })
    };
    let (now, dt, reduced) = (app.now, app.dt, app.reduced);
    let mut scroll = 0.0;
    if let Some(page) = &mut app.page {
        if wheel != 0.0 {
            page.scroll_target = (page.scroll_target - wheel * 1.6).clamp(0.0, page.scroll_max);
            page.scrolled_at = now;
        }
        let gap = page.scroll_target - page.scroll;
        page.scroll = if reduced || gap.abs() < 0.5 { page.scroll_target } else { page.scroll + gap * (1.0 - (-dt * 14.0).exp()) };
        scroll = page.scroll;
    }
    if (app.page.as_ref().map(|page| page.scroll_target - page.scroll).unwrap_or(0.0)).abs() >= 0.5 {
        app.moving = true;
    }
    let area = egui::ScrollArea::vertical()
        .id_salt(("page", &project.id))
        .auto_shrink([false, false])
        .scroll_source(egui::containers::scroll_area::ScrollSource { scroll_bar: true, drag: egui::containers::scroll_area::DragScroll::Never, mouse_wheel: false })
        .vertical_scroll_offset(scroll);
    let output = area.show(&mut child, |ui| {
        ui.add_space(HERO + TABS + 26.0);
        let left = screen.min.x + MARGIN + tab_direction * (1.0 - switch) * 36.0;
        let width = screen.width() - MARGIN * 2.0;
        ui.set_opacity(switch);
        match tab {
            Tab::Overview => overview(app, ui, &project, theme, left, width, t),
            Tab::Credits => credits(app, ui, &project, theme, left, width),
        }
        ui.add_space(60.0);
    });
    if let Some(page) = &mut app.page {
        page.scroll_max = (output.content_size.y - output.inner_rect.height()).max(0.0);
        let offset = output.state.offset().y;
        // The scroll bar dragged: the page follows it.
        if (offset - page.scroll).abs() > 1.0 {
            page.scroll = offset;
            page.scroll_target = offset;
        }
    }
    hero(app, ui.ctx(), screen, &project, theme, t);
    confirm_dialog(app, ui.ctx(), screen, &project, theme);
    floppies_dialog(app, ui.ctx(), screen, &project, theme);
    lightbox(app, ui.ctx(), screen, &project, theme);
}

fn hero(app: &mut App, context: &egui::Context, screen: Rect, project: &Project, theme: &Theme, t: f32) {
    let index = app.page.as_ref().map(|page| page.project).unwrap_or(0);
    let height = hero_height(app);
    let (poly, placed) = hero_geometry(app, index);
    egui::Area::new(Id::new("hero")).order(Order::Middle).fixed_pos(screen.min).constrain(false).show(context, |ui| {
        let (rect, _) = ui.allocate_exact_size(vec2(screen.width(), height + TABS), Sense::hover());
        let painter = ui.painter().with_clip_rect(screen);
        // Opaque: the content passes under it.
        fill(&painter, &[rect.min, pos2(rect.max.x, rect.min.y), pos2(rect.max.x, rect.max.y), pos2(rect.min.x, rect.max.y)], theme.bg);
        motion::motif(&painter, Rect::from_min_size(rect.min, vec2(rect.width(), height + TABS)), theme, 1.0, 0.0);
        let edge_a = pos2(rect.min.x, rect.min.y + height);
        // The edge rises to the right, but never into the title: it passes under the last
        // line's end (its baseline, plus its shadow and a margin) at any width.
        let collapse = clamp01((HERO - height) / (HERO - HERO_SMALL));
        let size = lerp(66.0, 40.0, collapse);
        let font = motion::display(size);
        let title = project.title.to_uppercase();
        let lines = motion::wrap(&painter, &title, &font, (screen.width() - MARGIN * 2.0) * 0.7);
        let lines: Vec<String> = if lines.len() > 2 { vec![title.clone()] } else { lines };
        let drop = lines.last().map_or(58.0, |line| {
            let galley = painter.layout_no_wrap(line.clone(), font.clone(), theme.ink);
            let baseline = galley.rows.first().and_then(|row| row.glyphs.first()).map_or(size, |glyph| glyph.pos.y);
            let bottom = rect.min.y + height - 30.0 - size + baseline + 4.0 + 8.0;
            let end = MARGIN + galley.size().x + 4.0;
            ((edge_a.y - 4.0 - bottom).max(0.0) * rect.width() / end.max(1.0)).min(58.0)
        });
        // The picture's outline follows the same edge.
        let poly = { let mut poly = poly; poly[2].y = poly[3].y - drop; poly };
        let flight = app.flight.as_ref().filter(|flight| !flight.closing).map(|flight| (flight.from, flight.from_placed, flight.started));
        let (shape, placed) = match flight {
            Some((from, from_placed, started)) => {
                let e = ease_out_quint(app.progress(started, OPEN_TIME));
                ((0..4).map(|k| motion::lerp_pos(from[k], poly[k], e)).collect::<Vec<_>>(), motion::lerp_rect(from_placed, placed, e))
            }
            None => (poly.to_vec(), placed),
        };
        if let Some(url) = project.key_art().filter(|url| !app.images.failed(url)).or_else(|| project.thumbnail()) {
            let pixel = app.themes.of(&project.id).pixel;
            if let Some((texture, _, _)) = app.images.get(context, &url, pixel) {
                motion::image_in(&painter, texture.id(), placed, &shape, Color32::WHITE);
            }
        }
        let landed = ease_out(clamp01((t - 0.2) / 0.5));
        // Shade for the title, then the slanted edge with its blade.
        motion::gradient_rect(&painter, Rect::from_min_max(pos2(rect.min.x, rect.min.y + height * 0.35), pos2(rect.max.x, rect.min.y + height)), alpha(theme.bg, 0.0), alpha(theme.bg, 0.92 * landed), true);
        motion::gradient_rect(&painter, Rect::from_min_max(rect.min, pos2(rect.min.x + rect.width() * 0.55, rect.min.y + height)), alpha(theme.bg, 0.75 * landed), alpha(theme.bg, 0.0), false);
        let edge_b = pos2(rect.max.x, rect.min.y + height - drop);
        let reach = ease_out_quint(clamp01((t - 0.3) / 0.5));
        let tip = motion::lerp_pos(edge_a, edge_b, reach);
        fill(&painter, &[edge_a + vec2(0.0, 2.0), tip + vec2(0.0, 2.0), tip + vec2(0.0, 9.0), edge_a + vec2(0.0, 9.0)], theme.accent2);
        motion::glow(&painter, &[edge_a - vec2(0.0, 3.0), tip - vec2(0.0, 3.0), tip + vec2(0.0, 3.0), edge_a + vec2(0.0, 3.0)], alpha(theme.accent, 0.35 * reach), 14.0);
        fill(&painter, &[edge_a - vec2(0.0, 3.0), tip - vec2(0.0, 3.0), tip + vec2(0.0, 3.0), edge_a + vec2(0.0, 3.0)], theme.accent);
        if let Some((.., started)) = flight {
            if app.progress(started, OPEN_TIME) >= 1.0 {
                app.flight = None;
            }
        }

        // The title over the picture.
        let mut y = rect.min.y + height - 30.0 - lines.len() as f32 * size;
        let kicker = format!("{}  ·  {}  ·  {}", project.series.label().to_uppercase(), project.year, project.developer.to_uppercase());
        spaced(&painter, pos2(rect.min.x + MARGIN, y - 12.0), &kicker, &motion::semibold(12.0), alpha(theme.accent, landed), 2.2);
        // The series opens the library on its shelf (as on the home).
        let series_label = project.series.label().to_uppercase();
        let series_width = spaced_width(&painter, &series_label, &motion::semibold(12.0), 2.2);
        let series_rect = Rect::from_min_size(pos2(rect.min.x + MARGIN - 4.0, y - 24.0), vec2(series_width + 8.0, 24.0));
        let series = ui.interact(series_rect, Id::new("page-series"), Sense::click());
        if series.hovered() {
            ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
            painter.line_segment([pos2(rect.min.x + MARGIN, y - 3.0), pos2(rect.min.x + MARGIN + series_width, y - 3.0)], Stroke::new(1.2, theme.accent));
        }
        if series.on_hover_text("Voir la série dans la bibliothèque").clicked() {
            app.filter = app::Filter::Series(project.series);
            app.query.clear();
            app.close(app::Mode::Library);
        }
        for (row, line) in lines.iter().enumerate() {
            let local = t - 0.28 - row as f32 * 0.08;
            // Letter by letter even once landed (app.rs, the home's title: the same reason).
            let local = if app.reduced { 99.0 } else { local };
            motion::letters(&painter, pos2(rect.min.x + MARGIN + 4.0, y + 4.0), line, font.clone(), alpha(theme.accent2, 0.6), local, 0.02, false);
            motion::letters(&painter, pos2(rect.min.x + MARGIN, y), line, font.clone(), theme.ink, local, 0.02, false);
            y += size;
        }

        // The tabs, a slanted marker sliding under the open one.
        let tabs_y = rect.min.y + height + 12.0;
        let current = app.page.as_ref().map(|page| page.tab).unwrap_or(Tab::Overview);
        let mut x = rect.min.x + MARGIN;
        let mut spots = Vec::new();
        for (tab, label) in [(Tab::Overview, "PRÉSENTATION"), (Tab::Credits, "CRÉDITS")] {
            let width = spaced_width(&painter, label, &motion::semibold(14.0), 2.4) + 8.0;
            let spot = Rect::from_min_size(pos2(x, tabs_y), vec2(width, 32.0));
            let response = ui.interact(spot, Id::new(("tab", label)), Sense::click());
            let hover = ui.ctx().animate_bool_with_time(response.id, response.hovered(), 0.12);
            let colour = if current == tab { theme.ink } else { mix(theme.muted, theme.ink, hover) };
            spaced(&painter, pos2(spot.min.x, spot.center().y), label, &motion::semibold(14.0), alpha(colour, landed), 2.4);
            if response.hovered() {
                ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
            }
            if response.clicked() && current != tab {
                let now = app.now;
                if let Some(page) = &mut app.page {
                    page.tab_direction = if tab == Tab::Credits { 1.0 } else { -1.0 };
                    page.tab = tab;
                    page.tab_changed = now;
                }
            }
            spots.push(spot);
            x = spot.max.x + 34.0;
        }
        let target = if current == Tab::Overview { spots[0] } else { spots[1] };
        let bar_x = ui.ctx().animate_value_with_time(Id::new("tab-x"), target.min.x, 0.3);
        let bar_w = ui.ctx().animate_value_with_time(Id::new("tab-w"), target.width(), 0.3);
        fill(&painter, &slanted(Rect::from_min_size(pos2(bar_x, target.max.y), vec2(bar_w * landed, 5.0)), SLANT * 3.0), theme.accent);
    });
}

/// A part of the page that comes in after the ones before it.
fn reveal(t: f32, order: usize) -> f32 {
    ease_out_quint(clamp01((t - 0.3 - order as f32 * 0.07) / 0.5))
}

/// Lays `add` at (x, y) in a column `width` wide, shifted down while it comes in; returns its height.
fn place(ui: &mut egui::Ui, x: f32, y: f32, width: f32, appear: f32, add: impl FnOnce(&mut egui::Ui)) -> f32 {
    let rect = Rect::from_min_size(pos2(x, y + (1.0 - appear) * 22.0), vec2(width, 10_000.0));
    let mut child = ui.new_child(UiBuilder::new().max_rect(rect));
    child.set_width(width);
    child.set_opacity(appear * ui.opacity());
    add(&mut child);
    child.min_rect().height()
}

fn heading(ui: &mut egui::Ui, text: &str, theme: &Theme) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::hover());
    let painter = ui.painter();
    fill(painter, &slanted(Rect::from_min_size(pos2(rect.min.x, rect.min.y + 6.0), vec2(8.0, 18.0)), SLANT), theme.accent);
    spaced(painter, pos2(rect.min.x + 18.0, rect.center().y), text, &motion::semibold(13.0), theme.ink, 2.6);
}

fn overview(app: &mut App, ui: &mut egui::Ui, project: &Project, theme: &Theme, left: f32, width: f32, t: f32) {
    let side = SIDE.min(width * 0.4);
    let main = (width - side - 56.0).min(860.0);
    let top = ui.cursor().min.y;
    let side_x = left + width - side;

    let mut y = top;
    let mut order = 0;
    let items = media(project);
    if !items.is_empty() {
        let appear = reveal(t, order);
        y += place(ui, left, y, main, appear, |ui| {
            gallery(app, ui, project, &items, theme);
        }) + 40.0;
        order += 1;
    }
    let warning = project.sheet.warning.clone();
    let description = project.sheet.description.clone();
    if !description.is_empty() || !warning.is_empty() {
        y += place(ui, left, y, main, reveal(t, order), |ui| {
            if !warning.is_empty() {
                egui::Frame::new().fill(alpha(theme.accent, 0.12)).inner_margin(egui::Margin::same(12)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new(&warning).color(theme.accent).size(14.0));
                });
                ui.add_space(14.0);
            }
            if !description.is_empty() {
                ui.label(RichText::new(&description).size(16.0).color(mix(theme.ink, theme.muted, 0.25)));
            }
        }) + 36.0;
        order += 1;
    }
    let release = app.release(project);
    if let Some(Ok(Some(release))) = &release {
        let notes = release.notes.trim().to_string();
        let version = release.version.clone();
        y += place(ui, left, y, main, reveal(t, order), |ui| {
            heading(ui, "NOTES DE VERSION", theme);
            ui.add_space(6.0);
            let start = ui.cursor().min;
            ui.horizontal(|ui| {
                ui.add_space(22.0);
                ui.vertical(|ui| {
                    ui.label(RichText::new(format!("Version {version}")).font(motion::display(26.0)).color(theme.accent));
                    ui.add_space(4.0);
                    if notes.is_empty() {
                        ui.label(RichText::new("Pas de notes pour cette version.").color(theme.muted));
                    }
                    for line in notes.lines().filter(|line| !line.trim().is_empty()) {
                        ui.label(RichText::new(format!("·  {}", line.trim().trim_start_matches(['-', '*', '•']).trim())).size(15.0));
                    }
                });
            });
            let end = ui.min_rect().max.y;
            ui.painter().rect_filled(Rect::from_min_max(start + vec2(4.0, 4.0), pos2(start.x + 8.0, end)), 0.0, alpha(theme.accent, 0.6));
        }) + 40.0;
        order += 1;
    }
    y += place(ui, left, y, main, reveal(t, order), |ui| {
        heading(ui, "AVIS DES JOUEURS", theme);
        ui.add_space(8.0);
        reviews(app, ui, project, theme);
    }) + 20.0;

    // The patch's column follows the page while the hero shrinks, then stays under the tabs
    // (it holds what matters most), as long as the page is longer than it.
    let scroll = app.page.as_ref().map(|page| page.scroll).unwrap_or(0.0);
    // A column taller than the window first scrolls with the page until its end shows, then
    // stays there; it never slides past the end of the page, and the page is at least as tall
    // as the column, so all of it can always be reached.
    let visible_part = (app.screen.height() - (HERO_SMALL + TABS + 26.0 + 60.0)).max(0.0);
    let id = egui::Id::new(("side-height", &project.id));
    let last = ui.ctx().memory(|memory| memory.data.get_temp::<f32>(id)).unwrap_or(0.0);
    let overflow = (last - visible_part).max(0.0);
    let room = (y - top - last).max(0.0);
    let sticky = (scroll - (HERO - HERO_SMALL) - overflow).max(0.0).min(room);
    let side_height = place(ui, side_x, top + sticky, side, reveal(t, 0), |ui| action_panel(app, ui, project, theme));
    if (side_height - last).abs() > 0.5 {
        ui.ctx().memory_mut(|memory| memory.data.insert_temp(id, side_height));
        ui.ctx().request_repaint();
    }
    let bottom = y.max(top + side_height);
    ui.allocate_exact_size(vec2(1.0, bottom - top), Sense::hover());
}

fn action_panel(app: &mut App, ui: &mut egui::Ui, project: &Project, theme: &Theme) {
    let Some(game) = project.game else {
        // The site has this one.
        ui.label(RichText::new("STATUT").font(motion::semibold(11.0)).color(theme.muted));
        ui.label(RichText::new(if project.status_label.is_empty() { "—".to_string() } else { project.status_label.to_uppercase() }).font(motion::display(40.0)).color(theme.accent));
        ui.add_space(4.0);
        ui.label(RichText::new(format!("{} · {}", project.platform, project.year)).color(theme.muted));
        ui.add_space(18.0);
        let painter = ui.painter().clone();
        let recipe = app.recipe(project).filter(crate::retro::Recipe::local);
        if let Some(version) = recipe.as_ref().and_then(|recipe| recipe.version.clone()) {
            ui.label(RichText::new(format!("Patch {version}")).color(theme.muted));
            ui.add_space(8.0);
        }
        // The button goes under what was just written (never over it).
        let at = ui.cursor().min;
        let ours = app.job.as_ref().is_some_and(|job| job.project == project.id);
        if let Some(link) = project.link.clone().filter(|_| project.series == crate::catalog::Series::Other) {
            // A creation: played in the browser.
            ui.allocate_exact_size(vec2(10.0, 56.0), Sense::hover());
            if slant_button(ui, &painter, Id::new(("play", &project.id)), at, "JOUER", theme, true, 1.0, app.now) {
                open_url(&link);
            }
            ui.add_space(8.0);
            ui.label(RichText::new("Le jeu s'ouvre dans votre navigateur.").size(13.0).color(theme.muted));
        } else if recipe.is_some() {
            // A retro game: its floppies are translated here.
            if ours {
                job_progress(app, ui, theme);
            } else {
                ui.allocate_exact_size(vec2(10.0, 56.0), Sense::hover());
                if slant_button(ui, &painter, Id::new(("floppies", &project.id)), at, "PATCHER MON JEU", theme, app.job.is_none(), 1.0, app.now) {
                    pick_floppies(app, ui.ctx(), project);
                }
            }
            ui.add_space(8.0);
            ui.label(
                RichText::new("Choisissez vos images disque (.d88, .t88) : une copie traduite « [FR] » est créée à côté, l'original reste intact.")
                    .size(13.0)
                    .color(theme.muted),
            );
        } else {
            ui.allocate_exact_size(vec2(10.0, 56.0), Sense::hover());
            if slant_button(ui, &painter, Id::new(("site", &project.id)), at, "VOIR SUR LE SITE", theme, true, 1.0, app.now) {
                open_url(&project.page);
            }
            ui.add_space(8.0);
            ui.label(RichText::new("Le patch et ses instructions sont sur la page du projet.").size(13.0).color(theme.muted));
        }
        if let Some(outcome) = app.outcomes.get(&project.id).filter(|_| !ours) {
            ui.add_space(6.0);
            ui.label(RichText::new(&outcome.text).size(13.0).color(if outcome.ok { mix(theme.accent, theme.ink, 0.4) } else { RED }));
        }
        related(app, ui, project, theme);
        return;
    };
    let release = app.release(project);
    let state = app.games.get(game.project);
    let installed = state.map(|state| state.installed.clone()).unwrap_or(Installed::Nothing);
    let folder = state.and_then(|state| state.folder.clone());

    install_control(app, ui, project, theme);
    ui.add_space(12.0);

    // Secondary actions, as icons.
    let busy = app.job.is_some();
    let patched = matches!(installed, Installed::Version(_));
    let mut open_folder = false;
    let mut change_folder = false;
    let mut uninstall = false;
    ui.horizontal(|ui| {
        open_folder = icon_button(ui, Icon::Folder, "Ouvrir le dossier du jeu", folder.is_some(), theme);
        change_folder = icon_button(ui, Icon::FolderEdit, "Choisir le dossier du jeu", !busy, theme);
        uninstall = icon_button(ui, Icon::Trash, "Désinstaller le patch (remettre le jeu en anglais)", patched && !busy, theme);
    });
    if let Some((path, _)) = &folder {
        ui.label(RichText::new(shorten(&path.display().to_string(), 60)).size(12.0).color(alpha(theme.muted, 0.8)));
    }
    if game.voices {
        voices_row(app, ui, project, folder.as_ref().map(|(path, _)| path.clone()), theme);
    }
    if open_folder {
        if let Some((path, _)) = &folder {
            app::open_folder(path);
        }
    }
    if change_folder {
        if let Some(chosen) = rfd::FileDialog::new().set_title(format!("Dossier de {}", game.name)).pick_folder() {
            if detect::is_game(game, &chosen) {
                if let Some(state) = app.games.get_mut(game.project) {
                    state.installed = patch::installed(game, &chosen);
                    state.folder = Some((chosen, Store::Manual));
                }
                app.outcomes.remove(&project.id);
            } else {
                app.outcomes.insert(
                    project.id.clone(),
                    app::Outcome { ok: false, text: format!("Ce dossier n'est pas celui de {} (il n'y a pas de {}).", game.name, game.marker), at: app.now - 10.0, kind: app::JobKind::Install },
                );
            }
        }
    }
    if uninstall {
        let now = app.now;
        if let Some(page) = &mut app.page {
            page.confirm = Some(now);
        }
    }
    ui.add_space(26.0);
    ui.label(RichText::new("DERNIÈRE VERSION").font(motion::semibold(13.0)).color(theme.muted));
    let latest = match &release {
        Some(Ok(Some(release))) => release.version.clone(),
        Some(Ok(None)) => "—".into(),
        Some(Err(_)) => "?".into(),
        None => "…".into(),
    };
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 62.0), Sense::hover());
    // As big as it fits: a short public number is huge, a long build name shrinks.
    let mut size = 68.0;
    while size > 22.0 && motion::width(ui.painter(), &latest, &motion::display(size)) > rect.width() - 8.0 {
        size -= 2.0;
    }
    motion::shadowed(ui.painter(), rect.left_center(), Align2::LEFT_CENTER, &latest, motion::display(size), theme.accent, alpha(theme.accent2, 0.7), (size / 14.0).max(2.0));
    let installed_text = match &installed {
        Installed::Nothing => "aucune (jeu en anglais)".to_string(),
        Installed::OldPatch => "ancien patch (mis de côté à l'installation)".to_string(),
        Installed::Version(version) => app.installed_label(project).unwrap_or_else(|| version.clone()),
    };
    ui.style_mut().text_styles.insert(egui::TextStyle::Body, motion::body(17.0));
    egui::Grid::new(("facts", &project.id)).num_columns(2).spacing([18.0, 7.0]).show(ui, |ui| {
        ui.label(RichText::new("Installée").color(theme.muted));
        ui.label(installed_text);
        ui.end_row();
        ui.label(RichText::new("Jeu").color(theme.muted));
        ui.label(match &folder {
            Some((_, Store::Steam)) => "trouvé (Steam)",
            Some((_, Store::Gog)) => "trouvé (GOG)",
            Some((_, Store::Manual)) => "dossier choisi",
            None => "introuvable",
        });
        ui.end_row();
        if !project.sheet.languages.is_empty() {
            ui.label(RichText::new("Langues").color(theme.muted));
            ui.label(&project.sheet.languages);
            ui.end_row();
        }
        ui.label(RichText::new("Compatible").color(theme.muted));
        ui.label(if game.gog_id != 0 { "PC · Steam, GOG" } else { "PC · Steam" });
        ui.end_row();
        if let Some(Ok(Some(release))) = &release {
            ui.label(RichText::new("Taille").color(theme.muted));
            ui.label(format!("{:.0} Mo", release.size as f64 / 1_048_576.0));
            ui.end_row();
        }
    });
    related(app, ui, project, theme);
}

/// The Japanese voices: installed, to install, or not yet available (SC and the 3rd for now).
fn voices_row(app: &mut App, ui: &mut egui::Ui, project: &Project, folder: Option<std::path::PathBuf>, theme: &Theme) {
    use crate::voices::VoiceState;
    ui.add_space(22.0);
    heading(ui, "VOIX JAPONAISES", theme);
    // The disk is asked at most every two seconds (and at once after a job), not every frame.
    let key = Id::new(("voice-state", &project.id));
    let (now, busy_now) = (app.now, app.job.is_some());
    let cached: Option<(VoiceState, f64, bool)> = ui.ctx().memory(|memory| memory.data.get_temp(key));
    let state = match cached {
        Some((state, at, was_busy)) if now - at < 2.0 && was_busy == busy_now => state,
        _ => {
            let state = folder.as_deref().map(crate::voices::state).unwrap_or(VoiceState::Missing);
            ui.ctx().memory_mut(|memory| memory.data.insert_temp(key, (state.clone(), now, busy_now)));
            state
        }
    };
    let busy = app.job.is_some();
    let index = app.page.as_ref().map(|page| page.project).unwrap_or(0);
    match app.voice_release(project) {
        None => {
            ui.label(RichText::new("Recherche des voix…").color(theme.muted));
        }
        Some(Err(error)) => {
            ui.label(RichText::new(format!("Serveur injoignable ({error}).")).color(theme.muted));
        }
        Some(Ok(None)) => {
            ui.label(RichText::new("Pas encore disponibles pour ce jeu.").color(theme.muted));
        }
        Some(Ok(Some(voices))) => {
            let size = format!("{:.1} Go", voices.size() as f64 / 1_073_741_824.0).replace('.', ",");
            match &state {
                VoiceState::Installed(version) if *version == voices.version => {
                    ui.label(RichText::new(format!("Installées ({version}).")).color(theme.ink));
                    if outline_button(ui, "RETIRER LES VOIX", folder.is_some() && !busy, theme) {
                        app.start(ui.ctx(), index, app::JobKind::RemoveVoices);
                    }
                }
                _ => {
                    let note = match &state {
                        VoiceState::Incomplete => "Votre ancien mod des voix n'a plus de chargeur (dsound.dll) : installez-les de nouveau.".to_string(),
                        VoiceState::Installed(version) => format!("Version {version} installée, {} disponible.", voices.version),
                        VoiceState::Missing => format!("Les voix japonaises des scènes doublées ({size})."),
                    };
                    ui.label(RichText::new(note).size(13.0).color(theme.muted));
                    if outline_button(ui, &format!("INSTALLER LES VOIX · {size}"), folder.is_some() && !busy, theme) {
                        app.start(ui.ctx(), index, app::JobKind::Voices);
                    }
                }
            }
        }
    }
}

/// A secondary action: a slanted outline that fills under the mouse.
fn outline_button(ui: &mut egui::Ui, label: &str, enabled: bool, theme: &Theme) -> bool {
    let font = motion::semibold(13.0);
    let width = spaced_width(ui.painter(), label, &font, 1.8) + 36.0;
    ui.add_space(6.0);
    let (rect, response) = ui.allocate_exact_size(vec2(width, 36.0), if enabled { Sense::click() } else { Sense::hover() });
    let hover = ui.ctx().animate_bool_with_time(response.id, enabled && response.hovered(), 0.14);
    let painter = ui.painter();
    let shape = slanted(rect, SLANT);
    let colour = if enabled { theme.accent } else { alpha(theme.muted, 0.5) };
    if hover > 0.0 {
        let grown = Rect::from_min_size(rect.min, vec2(rect.width() * ease_out(hover), rect.height()));
        fill(painter, &slanted(grown, SLANT), colour);
    }
    painter.add(egui::Shape::closed_line(shape.to_vec(), Stroke::new(2.0, colour)));
    spaced(painter, pos2(rect.min.x + 18.0, rect.center().y), label, &font, mix(colour, theme.bg, hover), 1.8);
    if enabled && response.hovered() {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }
    response.clicked() && enabled
}

/// The running job, for pages without the patch button (retro games).
fn job_progress(app: &mut App, ui: &mut egui::Ui, theme: &Theme) {
    let Some(job) = &app.job else { return };
    let (step, detail, fraction) = {
        let progress = job.shared.lock().unwrap();
        (progress.step.clone(), progress.detail.clone(), progress.fraction())
    };
    let kind = job.kind;
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 56.0), Sense::hover());
    let painter = ui.painter();
    let track = Rect::from_min_max(pos2(rect.min.x, rect.max.y - 12.0), rect.max);
    painter.text(pos2(rect.min.x, track.min.y - 8.0), Align2::LEFT_BOTTOM, phase_of(&step, kind), motion::display(26.0), theme.ink);
    painter.text(pos2(rect.max.x, track.min.y - 8.0), Align2::RIGHT_BOTTOM, format!("{:.0} %", fraction * 100.0), motion::display(26.0), theme.accent);
    painter.rect_filled(track, 0.0, alpha(theme.ink, 0.12));
    let filled = Rect::from_min_size(track.min, vec2(track.width() * fraction.max(0.02), track.height()));
    painter.rect_filled(filled, 0.0, theme.accent);
    motion::stripes(painter, filled, alpha(theme.accent2, 0.45), app.now as f32 * 1.6);
    ui.label(RichText::new(shorten(&detail, 48)).size(12.0).color(theme.muted));
    app.moving = true;
}

/// Asks for the floppies, then translates them.
/// Opens the floppies panel (floppies_dialog): the expected floppies, ticked as files are chosen.
pub fn pick_floppies(app: &mut App, _context: &egui::Context, _project: &Project) {
    let now = app.now;
    if let Some(page) = &mut app.page {
        page.floppies = Some((now, Vec::new()));
    }
}

/// The floppies panel: each floppy the patch expects, ticked when a chosen file holds it (by
/// its SHA-1, or Dragon Slayer's own recognition), so what is missing shows at once. Files come
/// from the picker or are dropped on the window; the patch runs once every floppy is there.
fn floppies_dialog(app: &mut App, context: &egui::Context, screen: Rect, project: &Project, theme: &Theme) {
    let Some(opened) = app.page.as_ref().and_then(|page| page.floppies.as_ref()).map(|(opened, _)| *opened) else { return };
    let Some(recipe) = app.recipe(project) else { return };
    let expected = crate::retro::expected(&recipe);
    let mut add: Vec<std::path::PathBuf> = context.input(|input| input.raw.dropped_files.iter().map(|file| file.path().to_path_buf()).collect());
    let files = app.page.as_ref().and_then(|page| page.floppies.as_ref()).map(|(_, files)| files.clone()).unwrap_or_default();
    let found_by = |index: usize| files.iter().find(|file| file.found.contains(&index));
    let complete = (0..expected.len()).all(|index| found_by(index).is_some());
    let strangers: Vec<&crate::retro::Checked> = files.iter().filter(|file| !file.unknown.is_empty()).collect();
    let p = ease_out_quint(app.progress(opened, 0.3));
    let text = clamp01((p - 0.5) * 2.0);
    let (mut close, mut patch, mut pick) = (false, false, false);
    egui::Area::new(Id::new("floppies")).order(Order::Tooltip).fixed_pos(screen.min).constrain(false).show(context, |ui| {
        let (rect, backdrop) = ui.allocate_exact_size(screen.size(), Sense::click());
        let painter = ui.painter().clone();
        painter.rect_filled(rect, 0.0, alpha(Color32::BLACK, 0.7 * p));
        let rows = expected.len() as f32 * 52.0 + strangers.len() as f32 * 40.0 + if strangers.is_empty() { 0.0 } else { 34.0 };
        let panel = Rect::from_center_size(rect.center(), vec2(620.0, 226.0 + rows));
        let shown = Rect::from_min_size(panel.min, vec2(panel.width() * p, panel.height()));
        fill(&painter, &slanted(shown.translate(vec2(8.0, 8.0)), SLANT * 0.2), theme.accent2);
        fill(&painter, &slanted(shown, SLANT * 0.2), theme.surface());
        let clip = painter.with_clip_rect(shown);
        let left = panel.min.x + 40.0;
        let mut y = panel.min.y + 34.0;
        spaced(&clip, pos2(left, y + 10.0), "PATCHER MON JEU", &motion::display(28.0), alpha(theme.ink, text), 1.4);
        y += 44.0;
        // The expected floppies.
        for (index, label) in expected.iter().enumerate() {
            let row = Rect::from_min_size(pos2(left, y), vec2(panel.width() - 80.0, 44.0));
            let file = found_by(index);
            let ok = file.is_some();
            fill(&clip, &slanted(row, SLANT * 0.2), alpha(if ok { theme.accent } else { theme.ink }, if ok { 0.12 } else { 0.05 } * text));
            // On the right, bare: a tick when it is there, a red cross when it is missing.
            let c = pos2(row.max.x - 30.0, row.center().y);
            if ok {
                motion::glow_dot(&clip, c, 4.0, alpha(theme.accent, 0.18 * text), 16.0);
                clip.add(egui::Shape::line(vec![c + vec2(-8.0, 0.0), c + vec2(-3.0, 6.0), c + vec2(9.0, -7.0)], Stroke::new(3.0, alpha(theme.accent, text))));
            } else {
                motion::icon(&clip, Icon::Close, c, 16.0, alpha(RED, text));
            }
            clip.text(pos2(row.min.x + 22.0, row.min.y + 7.0), Align2::LEFT_TOP, label, motion::semibold(15.0), alpha(theme.ink, text));
            let detail = match file {
                Some(file) => file.name.clone(),
                None => "manquante".to_string(),
            };
            clip.text(pos2(row.min.x + 22.0, row.min.y + 26.0), Align2::LEFT_TOP, detail, motion::body(12.5), alpha(if ok { theme.accent } else { theme.muted }, text));
            y += 52.0;
        }
        // The files that are none of them.
        if !strangers.is_empty() {
            y += 6.0;
            clip.text(pos2(left, y), Align2::LEFT_TOP, "NON RECONNUS", motion::semibold(11.5), alpha(RED, text));
            y += 28.0;
            for file in &strangers {
                let c = pos2(left + 26.0, y + 12.0);
                motion::icon(&clip, Icon::Close, c, 14.0, alpha(RED, text));
                clip.text(pos2(left + 52.0, y), Align2::LEFT_TOP, &file.name, motion::semibold(13.5), alpha(theme.ink, text));
                let why = if file.found.is_empty() { format!("version non prise en charge · {}", file.unknown.join(", ")) } else { format!("en partie seulement · {}", file.unknown.join(", ")) };
                clip.text(pos2(left + 52.0, y + 18.0), Align2::LEFT_TOP, why, motion::body(11.5), alpha(theme.muted, text));
                y += 40.0;
            }
        }
        y += 8.0;
        // Choose files (or drop them).
        let choose = Rect::from_min_size(pos2(left, y), vec2(motion::width(&painter, "CHOISIR DES FICHIERS…", &motion::semibold(13.0)) + 34.0, 34.0));
        let response = ui.interact(choose, Id::new("floppies-choose"), Sense::click());
        let hover = ui.ctx().animate_bool_with_time(response.id, response.hovered(), 0.12);
        clip.add(egui::Shape::closed_line(slanted(choose, SLANT).to_vec(), Stroke::new(1.5, alpha(theme.accent, text))));
        fill(&clip, &slanted(choose, SLANT), alpha(theme.accent, 0.15 * hover * text));
        clip.text(choose.center(), Align2::CENTER_CENTER, "CHOISIR DES FICHIERS…", motion::semibold(13.0), alpha(theme.accent, text));
        if response.hovered() {
            ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
        }
        if response.clicked() {
            pick = true;
        }
        clip.text(pos2(choose.max.x + 16.0, choose.center().y), Align2::LEFT_CENTER, "ou glissez-les sur cette fenêtre", motion::body(13.0), alpha(theme.muted, text));
        y += 58.0;
        let at = pos2(left, y);
        if slant_button(ui, &painter, Id::new("floppies-patch"), at, "PATCHER", theme, complete && app.job.is_none(), text, app.now) {
            patch = true;
        }
        let cancel = Rect::from_min_size(at + vec2(230.0, 6.0), vec2(120.0, 36.0));
        let response = ui.interact(cancel, Id::new("floppies-cancel"), Sense::click());
        let hover = ui.ctx().animate_bool_with_time(response.id, response.hovered(), 0.12);
        spaced(&painter, pos2(cancel.min.x + 10.0, cancel.center().y), "ANNULER", &motion::semibold(14.0), alpha(mix(theme.muted, theme.ink, hover), text), 2.0);
        if response.hovered() {
            ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
        }
        if response.clicked() {
            close = true;
        }
        if !complete {
            let missing = (0..expected.len()).filter(|index| found_by(*index).is_none()).count();
            clip.text(pos2(left, y + 54.0), Align2::LEFT_TOP, if missing == 1 { "Il manque 1 disquette.".to_string() } else { format!("Il manque {missing} disquettes.") }, motion::body(12.5), alpha(theme.muted, text));
        }
        if backdrop.clicked() && !panel.contains(ui.ctx().pointer_interact_pos().unwrap_or_default()) {
            close = true;
        }
    });
    if pick {
        if let Some(chosen) = rfd::FileDialog::new().set_title(format!("Disquettes de {}", project.title)).add_filter("Images disque PC-88", &["d88", "D88", "t88", "T88"]).pick_files() {
            add.extend(chosen);
        }
    }
    if !add.is_empty() {
        if let Some((_, files)) = app.page.as_mut().and_then(|page| page.floppies.as_mut()) {
            for path in add {
                if !files.iter().any(|file| file.path == path) {
                    files.push(crate::retro::check(&recipe, &path));
                }
            }
        }
    }
    if patch {
        let disks: Vec<std::path::PathBuf> = files.iter().filter(|file| !file.found.is_empty()).map(|file| file.path.clone()).collect();
        let index = app.page.as_ref().map(|page| page.project).unwrap_or(0);
        if let Some(page) = &mut app.page {
            page.floppies = None;
        }
        app.start_floppies(context, index, disks);
    } else if close {
        if let Some(page) = &mut app.page {
            page.floppies = None;
        }
    }
}

/// The latest article about the project.
fn related(app: &mut App, ui: &mut egui::Ui, project: &Project, theme: &Theme) {
    let Some(article) = catalog::article_about(&app.catalog.articles, project).cloned() else { return };
    ui.add_space(28.0);
    heading(ui, "À LA UNE", theme);
    ui.add_space(6.0);
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 92.0), Sense::click());
    let hover = ui.ctx().animate_bool_with_time(response.id, response.hovered(), 0.16);
    let painter = ui.painter().clone();
    let picture = Rect::from_min_size(rect.min, vec2(118.0, rect.height()));
    let shape = slanted(picture, SLANT * 0.6);
    fill(&painter, &shape.map(|p| p + vec2(4.0, 4.0)), alpha(theme.accent2, 0.5 + 0.4 * hover));
    fill(&painter, &shape, theme.shade(0.3));
    if let Some(url) = &article.illustration {
        let context = ui.ctx().clone();
        if let Some((texture, size, _)) = app.images.get(&context, url, false) {
            let texture = texture.id();
            motion::picture_in(&painter, texture, app.images.blur(url), size, &shape, motion::upright(picture, SLANT * 0.6), 1.3, 1.0 + 0.05 * hover, Color32::WHITE);
        }
    }
    let x = picture.max.x + 16.0 + 4.0 * hover;
    painter.text(pos2(x, rect.min.y + 6.0), Align2::LEFT_TOP, format!("{}  ·  {}", article.short_date(), article.author.to_uppercase()), motion::semibold(11.0), theme.accent);
    let galley = painter.layout(article.title.clone(), motion::editorial(17.0), mix(theme.ink, Color32::WHITE, hover), rect.max.x - x);
    painter.galley(pos2(x, rect.min.y + 24.0), galley, theme.ink);
    if response.hovered() {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }
    if response.on_hover_text("Lire sur le site").clicked() {
        open_url(&article.url);
    }
}

fn icon_button(ui: &mut egui::Ui, icon: Icon, tip: &str, enabled: bool, theme: &Theme) -> bool {
    let (rect, response) = ui.allocate_exact_size(vec2(54.0, 48.0), if enabled { Sense::click() } else { Sense::hover() });
    let hover = ui.ctx().animate_bool_with_time(response.id, enabled && response.hovered(), 0.12);
    let painter = ui.painter();
    fill(painter, &slanted(rect.shrink(2.0), SLANT), alpha(if enabled { theme.accent } else { theme.muted }, 0.1 + 0.25 * hover));
    motion::icon(painter, icon, rect.center(), 20.0, alpha(mix(theme.ink, Color32::WHITE, hover), if enabled { 1.0 } else { 0.3 }));
    if enabled && response.hovered() {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }
    response.on_hover_text(tip).clicked() && enabled
}

/// The patch's button. Idle: a slanted plate saying what it will do. Working:
/// the plate flattens into the progress bar, the step above it. Done: the bar
/// closes into a mark, then the plate comes back. Failed: it shakes, the reason stays under it.
fn install_control(app: &mut App, ui: &mut egui::Ui, project: &Project, theme: &Theme) {
    let Some(game) = project.game else { return };
    let id = Id::new(("install", &project.id));
    let release = app.release(project);
    let state = app.games.get(game.project);
    let installed = state.map(|state| state.installed.clone()).unwrap_or(Installed::Nothing);
    let has_folder = state.is_some_and(|state| state.folder.is_some());
    let ours = app.job.as_ref().filter(|job| job.project == project.id).map(|job| {
        let progress = job.shared.lock().unwrap();
        (progress.step.clone(), progress.detail.clone(), progress.fraction(), job.kind)
    });
    let other = app.job.is_some() && ours.is_none();
    let outcome = app.outcomes.get(&project.id).map(|outcome| (outcome.ok, outcome.text.clone(), outcome.at, outcome.kind));
    let since_outcome = outcome.as_ref().map(|(_, _, at, _)| (app.now - at) as f32).unwrap_or(99.0);
    let celebrating = matches!(outcome, Some((true, ..))) && since_outcome < 1.9 && !app.reduced;
    if celebrating {
        app.moving = true;
    }

    // Up to date, or installed while no public release exists yet (a test version): it plays.
    let up_to_date = match (&installed, &release) {
        (Installed::Version(version), Some(Ok(Some(release)))) => *version == release.build,
        (Installed::Version(_), Some(Ok(None))) | (Installed::Version(_), Some(Err(_))) => true,
        _ => false,
    };
    let failed = matches!(outcome, Some((false, ..)));
    let (label, enabled, hint): (&str, bool, Option<String>) = if other {
        ("INSTALLATION EN COURS", false, Some("Une autre installation est en cours.".into()))
    } else if !has_folder {
        ("JEU INTROUVABLE", false, Some("Indiquez le dossier du jeu avec l'icône du dossier.".into()))
    } else if up_to_date {
        ("JOUER", true, None)
    } else {
        match &release {
            None => ("RECHERCHE…", false, None),
            Some(Err(error)) => ("SERVEUR INJOIGNABLE", false, Some(error.clone())),
            Some(Ok(None)) => ("PAS ENCORE PUBLIÉ", false, Some("Ce patch n'a pas encore de version publique.".into())),
            Some(Ok(Some(_))) if failed => ("RÉESSAYER", true, None),
            Some(Ok(Some(_))) if matches!(installed, Installed::Version(_)) => ("METTRE À JOUR", true, None),
            Some(Ok(Some(_))) => ("INSTALLER LE PATCH", true, None),
        }
    };

    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(vec2(width, 84.0), if enabled && ours.is_none() { Sense::click() } else { Sense::hover() });
    let morph_target = if ours.is_some() || celebrating { 1.0 } else { 0.0 };
    let m = if app.reduced { morph_target } else { ui.ctx().animate_value_with_time(id.with("morph"), morph_target, 0.38) };
    let hover = ui.ctx().animate_bool_with_time(id.with("hover"), enabled && response.hovered() && ours.is_none(), 0.14);
    let press = ui.ctx().animate_bool_with_time(id.with("press"), enabled && response.is_pointer_button_down_on(), 0.06);
    let shake = if failed && since_outcome < 0.5 && !app.reduced {
        app.moving = true;
        (since_outcome * 48.0).sin() * 9.0 * (1.0 - since_outcome / 0.5)
    } else {
        0.0
    };
    let painter = ui.painter().clone();
    let base = rect.translate(vec2(shake, 0.0));
    let track = Rect::from_min_max(pos2(base.min.x, base.max.y - 12.0), base.max);

    if m < 1.0 {
        // The plate, flattening into the track.
        let height = lerp(66.0, 12.0, ease_in_out(m));
        let plate = Rect::from_min_max(pos2(base.min.x, base.max.y - height), base.max).translate(vec2(3.0, 3.0) * press);
        let slant = SLANT * (1.0 - m);
        let shape = slanted(plate.translate(vec2(3.0 * hover, 0.0)), slant);
        fill(&painter, &shape.map(|p| p + vec2(7.0, 7.0) * (1.0 - m) - vec2(3.0, 3.0) * press), if enabled { theme.accent2 } else { alpha(theme.muted, 0.2) });
        fill(&painter, &shape, if enabled { mix(theme.accent, Color32::WHITE, 0.12 * hover) } else { alpha(theme.muted, 0.3) });
        let label_alpha = clamp01(1.0 - m * 2.5);
        let font = motion::display(26.0);
        if label == "JOUER" {
            motion::icon(&painter, Icon::Play, pos2(plate.min.x + 30.0 + 3.0 * hover, plate.center().y), 16.0, alpha(theme.bg, label_alpha));
            spaced(&painter, pos2(plate.min.x + 50.0 + 3.0 * hover, plate.center().y), label, &font, alpha(theme.bg, label_alpha), 1.6);
        } else {
            spaced(&painter, pos2(plate.min.x + 26.0 + 3.0 * hover, plate.center().y), label, &font, alpha(if enabled { theme.bg } else { theme.ink }, label_alpha * if enabled { 1.0 } else { 0.55 }), 1.6);
        }
    }
    if m > 0.0 {
        let fraction = match &ours {
            Some((.., fraction, _)) => *fraction,
            None => 1.0,
        };
        let shown = if app.reduced { fraction } else { ui.ctx().animate_value_with_time(id.with("fraction"), fraction, 0.3) };
        painter.rect_filled(track, 0.0, alpha(theme.ink, 0.12 * m));
        let filled = Rect::from_min_size(track.min, vec2(track.width() * shown.max(0.02), track.height()));
        painter.rect_filled(filled, 0.0, alpha(theme.accent, m));
        if ours.is_some() {
            motion::stripes(&painter, filled, alpha(theme.accent2, 0.45 * m), app.now as f32 * 1.6);
        }
        let text_alpha = clamp01((m - 0.4) / 0.6);
        let (phase, detail) = match &ours {
            Some((step, detail, _, kind)) => (phase_of(step, *kind), detail.clone()),
            None => (
                match outcome.as_ref().map(|(.., kind)| *kind) {
                    Some(app::JobKind::Uninstall) => "JEU REMIS EN ANGLAIS",
                    Some(app::JobKind::Voices) => "VOIX INSTALLÉES",
                    Some(app::JobKind::RemoveVoices) => "VOIX RETIRÉES",
                    Some(app::JobKind::Floppies) => "DISQUETTES TRADUITES",
                    _ => "PATCH INSTALLÉ",
                }
                .to_string(),
                String::new(),
            ),
        };
        // The phase slides in when it changes.
        let phase_since = ui.ctx().memory_mut(|memory| {
            let entry = memory.data.get_temp_mut_or_insert_with(id.with("phase"), || (phase.clone(), app.now));
            if entry.0 != phase {
                *entry = (phase.clone(), app.now);
            }
            entry.1
        });
        let slide = ease_out_quint(clamp01((app.now - phase_since) as f32 / 0.35));
        if slide < 1.0 {
            app.moving = true;
        }
        let clip = painter.with_clip_rect(Rect::from_min_max(pos2(base.min.x, base.min.y - 4.0), pos2(base.max.x, track.min.y - 4.0)));
        clip.text(pos2(base.min.x, track.min.y - 8.0 + (1.0 - slide) * 30.0), Align2::LEFT_BOTTOM, &phase, motion::display(26.0), alpha(theme.ink, text_alpha));
        if ours.is_some() {
            clip.text(pos2(base.max.x, track.min.y - 8.0), Align2::RIGHT_BOTTOM, format!("{:.0} %", shown * 100.0), motion::display(26.0), alpha(theme.accent, text_alpha));
            painter.text(pos2(base.min.x, track.max.y + 6.0), Align2::LEFT_TOP, shorten(&detail, 48), motion::body(12.0), alpha(theme.muted, text_alpha));
        }
    }
    if celebrating {
        // The bar closes into a mark; rays burst around it.
        let t = since_outcome;
        let center = pos2(base.max.x - 30.0, base.min.y + 28.0);
        let grow = ease_back(clamp01(t / 0.4));
        let fade = 1.0 - clamp01((t - 1.5) / 0.4);
        for ray in 0..8 {
            let a = ray as f32 * std::f32::consts::TAU / 8.0 + 0.2;
            let k = ease_out(clamp01((t - 0.1) / 0.6));
            let inner = center + Vec2::angled(a) * (30.0 + 18.0 * k);
            let outer = center + Vec2::angled(a) * (36.0 + 34.0 * k);
            painter.line_segment([inner, outer], Stroke::new(3.0, alpha(theme.accent, (1.0 - k) * fade)));
        }
        painter.circle_filled(center, 26.0 * grow, alpha(theme.accent, fade));
        motion::check_mark(&painter, center, 13.0, clamp01((t - 0.25) / 0.4), Stroke::new(4.0, alpha(theme.bg, fade)));
    }
    if response.hovered() && enabled && ours.is_none() {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }
    if response.clicked() && enabled && ours.is_none() {
        let index = app.page.as_ref().map(|page| page.project).unwrap_or(0);
        if label == "JOUER" {
            play(app, project);
        } else {
            app.start(ui.ctx(), index, app::JobKind::Install);
        }
    }
    ui.add_space(18.0);
    if let Some(hint) = hint.filter(|_| ours.is_none() && !celebrating) {
        ui.label(RichText::new(hint).size(13.0).color(theme.muted));
    }
    if let Some((ok, text, ..)) = outcome.filter(|_| ours.is_none()) {
        ui.label(RichText::new(text).size(13.0).color(if ok { mix(theme.accent, theme.ink, 0.4) } else { RED }));
    }
}

/// What the job is doing, in one word for the eye.
fn phase_of(step: &str, kind: app::JobKind) -> String {
    if kind == app::JobKind::Uninstall {
        return "DÉSINSTALLATION".into();
    }
    if kind == app::JobKind::RemoveVoices {
        return "RETRAIT DES VOIX".into();
    }
    let step = step.to_lowercase();
    if step.starts_with("extraction") {
        return "EXTRACTION DES VOIX".into();
    }
    if step.starts_with("traduction") {
        return "TRADUCTION".into();
    }
    if step.starts_with("téléchargement") || step.starts_with("lecture") {
        "TÉLÉCHARGEMENT".into()
    } else if step.starts_with("vérification") {
        "VÉRIFICATION".into()
    } else {
        "INSTALLATION".into()
    }
}

pub fn play(app: &App, project: &Project) {
    let Some(game) = project.game else { return };
    let Some((folder, store)) = app.games.get(game.project).and_then(|state| state.folder.clone()) else { return };
    // Through Steam when it is Steam's (and always under Linux: the game runs in Proton).
    if store == Store::Steam || !cfg!(windows) {
        open_url(&format!("steam://rungameid/{}", game.steam_id));
    } else {
        let _ = std::process::Command::new(folder.join(game.exe)).current_dir(&folder).spawn();
    }
}

/// The media: one big view (a picture or the trailer's poster) and the strip of all of them.
fn gallery(app: &mut App, ui: &mut egui::Ui, project: &Project, items: &[Media], theme: &Theme) {
    let pixel = app.themes.of(&project.id).pixel;
    let (current, previous, changed) = app.page.as_ref().map(|page| (page.gallery.min(items.len() - 1), page.gallery_previous.min(items.len() - 1), page.gallery_changed)).unwrap_or((0, 0, 0.0));
    // As big as the column allows, but never taller than what the window shows under the
    // tabs with the strip: the whole gallery can be seen at once.
    let room = (app.screen.height() - (HERO_SMALL + TABS + 26.0) - STRIP).max(200.0);
    let height = (ui.available_width() * 9.0 / 16.0).min(room);
    let (view, response) = ui.allocate_exact_size(vec2(height * 16.0 / 9.0, height), Sense::click());
    let painter = ui.painter().clone();
    painter.rect_filled(view, 0.0, alpha(Color32::BLACK, 0.55));
    let context = ui.ctx().clone();
    let p = app.progress(changed, 0.42);
    let contain = |size: Vec2| {
        let scale = (view.width() / size.x).min(view.height() / size.y);
        Rect::from_center_size(view.center(), size * scale)
    };
    let corners = |r: Rect| [r.min, pos2(r.max.x, r.min.y), r.max, pos2(r.min.x, r.max.y)];
    // Under the trailer's view: black only, never its poster.
    let live = matches!(items[current], Media::Video(_)) && app.player.exists() && p >= 1.0;
    if live {
        painter.rect_filled(view, 0.0, Color32::BLACK);
    }
    if p < 1.0 && previous != current {
        if let Some((texture, size, _)) = app.images.get(&context, &items[previous].picture(), pixel) {
            let placed = contain(size);
            motion::image_in(&painter, texture.id(), placed, &corners(placed), Color32::WHITE);
        }
    }
    if live {
    } else if let Some((texture, size, since)) = app.images.get(&context, &items[current].picture(), pixel) {
        let placed = contain(size);
        let arrived = ease_out(clamp01((app.now - since) as f32 / 0.3));
        // A slanted wipe brings the new picture.
        let e = ease_out_quint(p);
        let s = view.height() * SLANT;
        let edge = lerp(view.min.x - s, view.max.x + s, e);
        let cut = placed.intersect(view);
        let wipe = [pos2(cut.min.x - s, cut.min.y), pos2(edge + s, cut.min.y), pos2(edge, cut.max.y), pos2(cut.min.x - s, cut.max.y)];
        let region: Vec<Pos2> = if p >= 1.0 { corners(cut).to_vec() } else { wipe.iter().map(|point| pos2(point.x.clamp(cut.min.x, cut.max.x), point.y)).collect() };
        motion::image_in(&painter, texture.id(), placed, &region, Color32::from_white_alpha((255.0 * arrived) as u8));
        if p < 1.0 {
            let blade = [pos2(edge + s - 6.0, cut.min.y), pos2(edge + s + 4.0, cut.min.y), pos2(edge + 4.0, cut.max.y), pos2(edge - 6.0, cut.max.y)];
            fill(&painter.with_clip_rect(cut), &blade, theme.accent);
        }
    } else {
        painter.text(view.center(), Align2::CENTER_CENTER, "…", motion::display(30.0), theme.muted);
    }
    let hover = ui.ctx().animate_bool_with_time(response.id, response.hovered(), 0.15);
    if let Media::Video(id) = &items[current] {
        // The trailer plays here (video.rs), over its poster; seen only when nothing covers it.
        let page = app.page.as_ref();
        let settled = page.is_some_and(|page| page.lightbox.is_none() && page.confirm.is_none() && page.floppies.is_none() && app.now - page.opened > 0.9 && app.now - page.tab_changed > 0.35)
            && app.flight.is_none()
            && p >= 1.0;
        // The window's edge cuts the view by itself; only the hero must not cover it.
        // It follows the page as it scrolls, cut under the hero and the tabs and by the window's
        // bottom; out of sight (or nearly), it pauses.
        let under = app.screen.min.y + hero_height(app) + TABS;
        let clip = Rect::from_min_max(pos2(view.min.x, view.min.y.max(under)), pos2(view.max.x, view.max.y.min(app.screen.max.y)));
        let gone = clip.height() < view.height() * 0.25;
        app.trailer = Some(crate::video::Request { id: id.clone(), rect: view, clip, visible: settled && clip.height() >= 2.0, pause: !settled || gone });
        if !app.player.failed {
            if !live {
                painter.text(view.center(), Align2::CENTER_CENTER, "BANDE-ANNONCE", motion::display(30.0), alpha(Color32::WHITE, 0.6));
            }
            return_strip(app, ui, project, items, theme, current, pixel, &context, view.width());
            return;
        }
        let play = Rect::from_center_size(view.center(), vec2(108.0 + 12.0 * hover, 64.0 + 6.0 * hover));
        fill(&painter, &slanted(play.translate(vec2(6.0, 6.0)), SLANT), theme.accent2);
        fill(&painter, &slanted(play, SLANT), theme.accent);
        motion::icon(&painter, Icon::Play, play.center(), 26.0, theme.bg);
        painter.text(view.left_bottom() + vec2(16.0, -14.0), Align2::LEFT_BOTTOM, "BANDE-ANNONCE · s'ouvre sur YouTube", motion::semibold(12.0), theme.ink);
        if response.hovered() {
            ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
        }
        if response.clicked() {
            open_url(&format!("https://www.youtube.com/watch?v={id}"));
        }
    } else {
        if response.hovered() {
            ui.ctx().set_cursor_icon(CursorIcon::ZoomIn);
        }
        if response.on_hover_text("Agrandir").clicked() {
            let now = app.now;
            if let Some(page) = &mut app.page {
                page.lightbox = Some(now);
                page.lightbox_from = view;
            }
        }
    }

    return_strip(app, ui, project, items, theme, current, pixel, &context, view.width());
}

/// The strip of every medium under the big view.
#[allow(clippy::too_many_arguments)]
#[allow(clippy::too_many_arguments)]
fn return_strip(app: &mut App, ui: &mut egui::Ui, project: &Project, items: &[Media], theme: &Theme, current: usize, pixel: bool, context: &egui::Context, width: f32) {
    let corners = |r: Rect| [r.min, pos2(r.max.x, r.min.y), r.max, pos2(r.min.x, r.max.y)];
    // The strip: exactly as wide as the big view above, its thumbnails filling it (or, too
    // many to fit, scrolling inside it).
    ui.add_space(10.0);
    let mut chosen = None;
    let gap = 8.0;
    let count = items.len().max(1) as f32;
    let fill = (width - gap * (count - 1.0)) / count;
    // Too many to fit: they scroll; few: no wider than 160, the row centred.
    let thumb = if fill < 110.0 { 128.0 } else { fill.min(160.0) };
    let total = count * thumb + gap * (count - 1.0);
    ui.allocate_ui(vec2(width, 90.0), |ui| {
        ui.set_max_width(width);
        egui::ScrollArea::horizontal().id_salt(("strip", &project.id)).max_width(width).auto_shrink([false, true]).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                if total < width {
                    ui.add_space(((width - total) / 2.0 - gap).max(0.0));
                }
                for (index, item) in items.iter().enumerate() {
                    let (rect, response) = ui.allocate_exact_size(vec2(thumb, 72.0), Sense::click());
                if !ui.is_rect_visible(rect) {
                    continue;
                }
                let on = ui.ctx().animate_bool_with_time(response.id, index == current, 0.2);
                let hover = ui.ctx().animate_bool_with_time(response.id.with("hover"), response.hovered(), 0.12);
                let painter = ui.painter();
                let lifted = rect.translate(vec2(0.0, -3.0 * on));
                painter.rect_filled(lifted, 0.0, alpha(Color32::BLACK, 0.5));
                if let Some((texture, size, _)) = app.images.get(context, &item.picture(), pixel) {
                    let shade = (130.0 + 125.0 * on.max(hover)) as u8;
                    motion::image_in(painter, texture.id(), motion::cover(size, lifted, 1.0, Vec2::ZERO), &corners(lifted), Color32::from_rgb(shade, shade, shade));
                }
                if matches!(item, Media::Video(_)) {
                    motion::icon(painter, Icon::Play, lifted.center(), 18.0, theme.ink);
                }
                if on > 0.0 {
                    painter.rect_filled(Rect::from_min_size(pos2(lifted.min.x, lifted.max.y + 2.0), vec2(lifted.width() * ease_out(on), 4.0)), 0.0, theme.accent);
                }
                if response.hovered() {
                    ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                }
                if response.clicked() {
                    chosen = Some(index);
                }
            }
        });
        });
    });
    if let Some(index) = chosen {
        show_media(app, index);
    }
}

fn show_media(app: &mut App, index: usize) {
    let now = app.now;
    if let Some(page) = &mut app.page {
        if page.gallery != index {
            page.gallery_previous = page.gallery;
            page.gallery = index;
            page.gallery_changed = now;
        }
    }
}

fn lightbox(app: &mut App, context: &egui::Context, screen: Rect, project: &Project, theme: &Theme) {
    let Some((opened, from, current)) = app.page.as_ref().and_then(|page| page.lightbox.map(|opened| (opened, page.lightbox_from, page.gallery))) else { return };
    let items = media(project);
    let Some(item) = items.get(current) else { return };
    let pixel = app.themes.of(&project.id).pixel;
    let p = ease_out_quint(app.progress(opened, 0.32));
    let mut close = false;
    let mut step = 0i32;
    egui::Area::new(Id::new("lightbox")).order(Order::Tooltip).fixed_pos(screen.min).constrain(false).show(context, |ui| {
        let (rect, backdrop) = ui.allocate_exact_size(screen.size(), Sense::click());
        let painter = ui.painter().clone();
        painter.rect_filled(rect, 0.0, alpha(Color32::BLACK, 0.92 * p));
        if let Some((texture, size, _)) = app.images.get(context, &item.picture(), pixel) {
            let space = rect.shrink2(vec2(90.0, 60.0));
            let scale = (space.width() / size.x).min(space.height() / size.y);
            let target = Rect::from_center_size(space.center(), size * scale);
            let start = Rect::from_center_size(from.center(), size * (from.width() / size.x).min(from.height() / size.y));
            let shown = motion::lerp_rect(start, target, p);
            painter.image(texture.id(), shown, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
        }
        for (icon, delta, at, tip) in [
            (Icon::Left, -1, pos2(rect.min.x + 40.0, rect.center().y), "Précédente (←)"),
            (Icon::Right, 1, pos2(rect.max.x - 40.0, rect.center().y), "Suivante (→)"),
            (Icon::Close, 0, pos2(rect.max.x - 40.0, rect.min.y + 40.0), "Fermer (Échap)"),
        ] {
            let spot = Rect::from_center_size(at, vec2(48.0, 48.0));
            let possible = delta == 0 || (current as i32 + delta >= 0 && ((current as i32 + delta) as usize) < items.len());
            let response = ui.interact(spot, Id::new(("lightbox", delta)), Sense::click());
            let hover = ui.ctx().animate_bool_with_time(response.id, response.hovered() && possible, 0.12);
            if hover > 0.0 {
                fill(&painter, &slanted(spot, SLANT), alpha(theme.accent, 0.3 * hover));
            }
            motion::icon(&painter, icon, spot.center(), 20.0, alpha(crate::theme::TEXT, p * if possible { 1.0 } else { 0.25 }));
            if response.on_hover_text(tip).clicked() && possible {
                if delta == 0 {
                    close = true;
                } else {
                    step = delta;
                }
            }
        }
        painter.text(rect.center_bottom() - vec2(0.0, 24.0), Align2::CENTER_BOTTOM, format!("{} / {}", current + 1, items.len()), motion::semibold(13.0), alpha(crate::theme::DIM, p));
        if backdrop.clicked() {
            close = true;
        }
        let (left, right) = ui.input(|input| (input.key_pressed(egui::Key::ArrowLeft), input.key_pressed(egui::Key::ArrowRight)));
        if left && current > 0 {
            step = -1;
        }
        if right && current + 1 < items.len() {
            step = 1;
        }
    });
    if step != 0 {
        show_media(app, (current as i32 + step) as usize);
    }
    if close {
        if let Some(page) = &mut app.page {
            page.lightbox = None;
        }
    }
}

fn confirm_dialog(app: &mut App, context: &egui::Context, screen: Rect, project: &Project, theme: &Theme) {
    let Some(opened) = app.page.as_ref().and_then(|page| page.confirm) else { return };
    let p = ease_out_quint(app.progress(opened, 0.3));
    let mut answer = None;
    egui::Area::new(Id::new("confirm")).order(Order::Tooltip).fixed_pos(screen.min).constrain(false).show(context, |ui| {
        let (rect, backdrop) = ui.allocate_exact_size(screen.size(), Sense::click());
        let painter = ui.painter().clone();
        painter.rect_filled(rect, 0.0, alpha(Color32::BLACK, 0.7 * p));
        // The panel unrolls from its left edge.
        let panel = Rect::from_center_size(rect.center(), vec2(520.0, 210.0));
        let shown = Rect::from_min_size(panel.min, vec2(panel.width() * p, panel.height()));
        fill(&painter, &slanted(shown.translate(vec2(8.0, 8.0)), SLANT * 0.3), theme.accent2);
        fill(&painter, &slanted(shown, SLANT * 0.3), theme.surface());
        let text = clamp01((p - 0.5) * 2.0);
        let clip = painter.with_clip_rect(shown);
        spaced(&clip, panel.min + vec2(36.0, 44.0), "DÉSINSTALLER LE PATCH ?", &motion::display(28.0), alpha(theme.ink, text), 1.4);
        let galley = clip.layout(
            format!("Les fichiers anglais d'origine de {} seront remis en place. Vous pourrez réinstaller le patch à tout moment.", project.title),
            motion::body(15.0),
            alpha(theme.muted, text),
            panel.width() - 72.0,
        );
        clip.galley(panel.min + vec2(36.0, 70.0), galley, theme.muted);
        let at = panel.min + vec2(36.0, 136.0);
        let danger = Theme { accent: RED, accent2: Color32::from_rgb(0x6b, 0x1a, 0x14), ..*theme };
        if slant_button(ui, &painter, Id::new("confirm-yes"), at, "DÉSINSTALLER", &danger, true, text, app.now) {
            answer = Some(true);
        }
        let cancel = Rect::from_min_size(at + vec2(250.0, 6.0), vec2(120.0, 36.0));
        let response = ui.interact(cancel, Id::new("confirm-no"), Sense::click());
        let hover = ui.ctx().animate_bool_with_time(response.id, response.hovered(), 0.12);
        spaced(&painter, pos2(cancel.min.x + 10.0, cancel.center().y), "ANNULER", &motion::semibold(14.0), alpha(mix(theme.muted, theme.ink, hover), text), 2.0);
        if response.hovered() {
            ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
        }
        if response.clicked() {
            answer = Some(false);
        }
        if backdrop.clicked() && !panel.contains(ui.ctx().pointer_interact_pos().unwrap_or_default()) {
            answer = Some(false);
        }
    });
    if let Some(yes) = answer {
        if let Some(page) = &mut app.page {
            page.confirm = None;
        }
        if yes {
            let index = app.page.as_ref().map(|page| page.project).unwrap_or(0);
            app.start(context, index, app::JobKind::Uninstall);
        }
    }
}

/// A text that acts as a link: the hand and an underline under the mouse.
fn link(ui: &mut egui::Ui, text: &str, colour: Color32) -> egui::Response {
    let font = motion::body(15.0);
    let (rect, response) = ui.allocate_exact_size(vec2(motion::width(ui.painter(), text, &font), 22.0), Sense::click());
    ui.painter().text(rect.left_center(), Align2::LEFT_CENTER, text, font, colour);
    if response.hovered() {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
        ui.painter().line_segment([pos2(rect.min.x, rect.max.y - 2.0), pos2(rect.max.x, rect.max.y - 2.0)], Stroke::new(1.2, colour));
    }
    response
}

fn reviews(app: &mut App, ui: &mut egui::Ui, project: &Project, theme: &Theme) {
    let state = app.reviews.get(&project.id).map(|slot| match &*slot.lock().unwrap() {
        None => 0,
        Some(Reviews::Closed) => 1,
        Some(Reviews::Open(list)) => 2 + list.len(),
    });
    match state {
        None | Some(0) => {
            ui.label(RichText::new("Chargement des avis…").color(theme.muted));
        }
        Some(1) => {
            ui.label(RichText::new("Les avis ne peuvent pas être chargés pour l'instant (service injoignable).").color(theme.muted));
        }
        Some(_) => {
            let list = match app.reviews.get(&project.id).and_then(|slot| slot.lock().unwrap().as_ref().map(|reviews| match reviews {
                Reviews::Open(list) => list.clone(),
                _ => Vec::new(),
            })) {
                Some(list) => list,
                None => Vec::new(),
            };
            if list.is_empty() {
                ui.label(RichText::new("Aucun avis publié pour l'instant.").color(theme.muted));
            }
            for review in list {
                ui.horizontal(|ui| {
                    let (rect, _) = ui.allocate_exact_size(vec2(70.0, 14.0), Sense::hover());
                    for star in 0..5 {
                        let mark = Rect::from_min_size(rect.min + vec2(star as f32 * 13.0, 2.0), vec2(9.0, 11.0));
                        fill(ui.painter(), &slanted(mark, SLANT), if star < review.rating { theme.accent } else { alpha(theme.muted, 0.3) });
                    }
                    ui.label(RichText::new(&review.author).font(motion::semibold(14.0)));
                    ui.label(RichText::new(&review.date).size(12.0).color(theme.muted));
                });
                ui.label(RichText::new(&review.text).size(15.0));
                ui.add_space(12.0);
            }
        }
    }
    ui.add_space(10.0);
    // A review sent: it waits for the moderators.
    let finished = app.page.as_ref().and_then(|page| page.sending.as_ref()).and_then(|slot| slot.lock().unwrap().take());
    if let (Some(result), Some(page)) = (finished, app.page.as_mut()) {
        page.sending = None;
        match result {
            Ok(()) => {
                crate::reviews::remember(&project.id);
                page.composer = false;
                page.draft.clear();
            }
            Err(error) => page.send_error = Some(error),
        }
    }
    if crate::reviews::sent().contains(&project.id) && !app.page.as_ref().is_some_and(|page| page.composer) {
        egui::Frame::new().fill(alpha(theme.accent, 0.08)).inner_margin(egui::Margin::same(12)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new("Merci ! Votre avis attend la modération : il apparaîtra ici une fois validé par l'équipe.").color(theme.ink));
        });
        return;
    }
    // Reviews are for a patch the public has: none before (a local or test patch included).
    if !app.patch_public(project) {
        ui.label(RichText::new("Les avis s'ouvriront à la sortie publique du patch.").size(14.0).color(theme.muted));
        return;
    }
    let open = app.page.as_ref().is_some_and(|page| page.composer);
    if !open {
        if link(ui, "Écrire un avis", theme.accent).clicked() {
            if let Some(page) = &mut app.page {
                page.composer = true;
            }
        }
        return;
    }
    let Some(page) = &mut app.page else { return };
    egui::Frame::new().fill(alpha(theme.ink, 0.05)).inner_margin(egui::Margin::same(14)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.label(RichText::new("Votre note").color(theme.muted));
            // Under the mouse, the marks up to the hovered one fill (and only those); otherwise
            // the chosen note.
            let marks: Vec<(Rect, egui::Response)> = (1..=5u8).map(|_| ui.allocate_exact_size(vec2(18.0, 20.0), Sense::click())).collect();
            let hovered = marks.iter().position(|(_, response)| response.hovered()).map(|index| index as u8 + 1);
            let shown = hovered.unwrap_or(page.rating);
            for (index, (rect, response)) in marks.iter().enumerate() {
                let star = index as u8 + 1;
                fill(ui.painter(), &slanted(rect.shrink2(vec2(3.0, 2.0)), SLANT), if star <= shown { theme.accent } else { alpha(theme.muted, 0.3) });
                if response.hovered() {
                    ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                }
                if response.clicked() {
                    page.rating = star;
                }
            }
        });
        ui.add(egui::TextEdit::singleline(&mut page.author).hint_text("Votre pseudo").desired_width(240.0));
        let author = page.author.trim().chars().count();
        if author > 0 && !(2..=40).contains(&author) {
            ui.label(RichText::new("Le pseudo fait de 2 à 40 caractères.").size(12.0).color(RED));
        }
        ui.add(egui::TextEdit::multiline(&mut page.draft).hint_text("Ce que vous avez pensé de la traduction…").desired_rows(3).desired_width(f32::INFINITY));
        let words = page.draft.trim().chars().count();
        if words > 0 && !(10..=2000).contains(&words) {
            ui.label(RichText::new(if words < 10 { "Encore quelques mots : 10 caractères au moins." } else { "2000 caractères au plus." }).size(12.0).color(RED));
        }
        let sending = page.sending.is_some();
        let ready = page.rating > 0 && (2..=40).contains(&author) && (10..=2000).contains(&words);
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let label = if sending { "ENVOI…" } else { "ENVOYER" };
            let (rect, response) = ui.allocate_exact_size(vec2(motion::width(ui.painter(), label, &motion::semibold(13.0)) + 34.0, 32.0), Sense::click());
            let on = ready && !sending;
            let hover = ui.ctx().animate_bool_with_time(response.id, on && response.hovered(), 0.12);
            fill(ui.painter(), &slanted(rect, SLANT), if on { mix(theme.accent, theme.accent2, hover * 0.35) } else { alpha(theme.muted, 0.25) });
            ui.painter().text(rect.center(), Align2::CENTER_CENTER, label, motion::semibold(13.0), if on { theme.bg } else { theme.muted });
            if on && response.hovered() {
                ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
            }
            ui.add_space(12.0);
            if on && response.clicked() {
                let slot: crate::app::Slot<Result<(), String>> = Default::default();
                let (inner, id, author, rating, text) = (slot.clone(), project.id.clone(), page.author.trim().to_string(), page.rating, page.draft.trim().to_string());
                let context = ui.ctx().clone();
                std::thread::spawn(move || {
                    *inner.lock().unwrap() = Some(crate::reviews::submit(&id, &author, rating, &text));
                    context.request_repaint();
                });
                page.sending = Some(slot);
                page.send_error = None;
            }
            if link(ui, "Annuler", theme.muted).clicked() {
                page.composer = false;
            }
        });
        if let Some(error) = &page.send_error {
            ui.label(RichText::new(error).size(13.0).color(RED));
        } else {
            ui.label(RichText::new("Votre avis sera publié après relecture par l'équipe.").size(12.0).color(theme.muted));
        }
    });
}

fn credits(app: &mut App, ui: &mut egui::Ui, project: &Project, theme: &Theme, left: f32, width: f32) {
    let changed = app.page.as_ref().map(|page| page.tab_changed).unwrap_or(0.0);
    let t = app.since(changed, 1.4);
    // Teams and their people.
    let mut groups: Vec<(String, Color32, Vec<String>)> = Vec::new();
    for credit in &project.sheet.credits {
        match credit {
            Credit::Team(name, [r, g, b]) => groups.push((name.to_uppercase(), Color32::from_rgb(*r, *g, *b), Vec::new())),
            Credit::Person(name) => match groups.last_mut() {
                Some(group) => group.2.push(name.clone()),
                None => groups.push((String::new(), theme.accent, vec![name.clone()])),
            },
        }
    }
    let top = ui.cursor().min.y;
    if groups.is_empty() {
        let height = place(ui, left, top, width.min(600.0), 1.0, |ui| {
            ui.label(RichText::new("Les crédits de ce projet sont sur le site.").size(16.0));
            if ui.add(egui::Button::new(RichText::new("Voir la page du projet").color(theme.accent)).frame(false)).clicked() {
                open_url(&project.page);
            }
        });
        ui.allocate_exact_size(vec2(1.0, height), Sense::hover());
        return;
    }
    let columns = ((width + 40.0) / 300.0).floor().clamp(1.0, 4.0) as usize;
    let column_width = (width - 40.0 * (columns as f32 - 1.0)) / columns as f32;
    let mut heights = vec![0.0f32; columns];
    for (order, (team, colour, people)) in groups.iter().enumerate() {
        let column = heights.iter().enumerate().min_by(|a, b| a.1.total_cmp(b.1)).map(|(index, _)| index).unwrap_or(0);
        let appear = ease_out_quint(clamp01((t - order.min(16) as f32 * 0.05) / 0.5));
        if appear < 1.0 {
            app.moving = true;
        }
        let x = left + column as f32 * (column_width + 40.0);
        // The site's colours are made for a dark ground: on a light theme, the pale ones
        // (light blue, pale green) are darkened towards the ink until they read.
        let colour = if theme.light {
            let luminance = |c: Color32| (0.299 * c.r() as f32 + 0.587 * c.g() as f32 + 0.114 * c.b() as f32) / 255.0;
            let mut shown = *colour;
            let mut step = 0;
            while luminance(shown) > 0.45 && step < 10 {
                shown = mix(shown, theme.ink, 0.15);
                step += 1;
            }
            shown
        } else {
            mix(*colour, Color32::WHITE, 0.25)
        };
        let height = place(ui, x, top + heights[column], column_width, appear, |ui| {
            if !team.is_empty() {
                let (rect, _) = ui.allocate_exact_size(vec2(column_width, 26.0), Sense::hover());
                fill(ui.painter(), &slanted(Rect::from_min_size(rect.min + vec2(0.0, 5.0), vec2(22.0 * appear, 4.0)), SLANT * 2.0), colour);
                spaced(ui.painter(), pos2(rect.min.x, rect.center().y + 7.0), team, &motion::semibold(12.0), colour, 2.4);
                ui.add_space(6.0);
            }
            for person in people {
                ui.label(RichText::new(person).font(motion::semibold(17.0)).color(theme.ink));
            }
        });
        heights[column] += height + 34.0;
    }
    let tallest = heights.iter().cloned().fold(0.0, f32::max);
    ui.allocate_exact_size(vec2(1.0, tallest), Sense::hover());
}
