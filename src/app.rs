//! The window. Two places: the home (a carousel to discover the projects one
//! at a time, or the library: every project in a grid, searchable and
//! filterable) and a project's page (page.rs). Each project brings its theme
//! (theme.rs); changing project wipes the new theme across the window. The
//! picture of the selected project carries over from the home to the page.
//!
//! Still when nothing happens: every animation is a function of the time since
//! its cause, so a new click replaces it at once, and the window redraws only
//! while something moves.

// egui 0.37 deprecates Id::new for fixed names; they are unique here.
#![allow(deprecated)]

use crate::catalog::{self, Catalog, Project, Series};
use crate::detect::{self, Store};
use crate::images::Images;
use crate::motion::{self, Icon, SLANT, alpha, clamp01, ease_in_out, ease_out, ease_out_quint, fill, lerp, mix, mix_theme, slanted};
use crate::patch::{self, Installed, Release, Shared, Source};
use crate::reviews::Reviews;
use crate::theme::{Theme, Themes, Transition};
use eframe::egui::{self, Align2, Color32, CursorIcon, FontData, FontDefinitions, FontFamily, Id, Key, Order, Pos2, Rect, Sense, Stroke, Vec2, pos2, vec2};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

pub type Slot<T> = Arc<Mutex<Option<T>>>;

pub const TOP: f32 = 70.0;
/// The news get their own column when the window is at least this wide.
const PANEL_FROM: f32 = 1240.0;
const PANEL: f32 = 392.0;
const BAND: f32 = 172.0;
const NEWS_WIDTH: f32 = 380.0;
// Close to the site's thumbnails (about 2.1:1 once the slant is set aside): shown whole.
const CARD: Vec2 = Vec2::new(200.0, 92.0);
const CARD_STEP: f32 = 220.0;
/// Seconds of a theme change.
const THEME_TIME: f32 = 0.62;
pub const OPEN_TIME: f32 = 0.55;

pub struct GameState {
    pub folder: Option<(PathBuf, Store)>,
    pub installed: Installed,
    pub release: Slot<Result<Option<Release>, String>>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Discover,
    Library,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Filter {
    All,
    Series(Series),
    Installed,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Overview,
    Credits,
}

/// The flight of a picture between the home and a page.
pub struct Flight {
    pub from: [Pos2; 4],
    pub from_placed: Rect,
    pub started: f64,
    pub closing: bool,
    pub project: usize,
}

pub struct Page {
    pub project: usize,
    pub tab: Tab,
    pub tab_changed: f64,
    pub tab_direction: f32,
    pub opened: f64,
    pub scroll: f32,
    pub scroll_target: f32,
    pub scroll_max: f32,
    pub scrolled_at: f64,
    pub gallery: usize,
    pub gallery_previous: usize,
    pub gallery_changed: f64,
    pub lightbox: Option<f64>,
    pub lightbox_from: Rect,
    pub confirm: Option<f64>,
    pub composer: bool,
    pub draft: String,
    pub rating: u8,
    pub author: String,
    /// The review being sent, and why it was refused.
    pub sending: Option<Slot<Result<(), String>>>,
    pub send_error: Option<String>,
}

/// What a job does (its progress and its success say it).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum JobKind {
    Install,
    Uninstall,
    Voices,
    RemoveVoices,
    Floppies,
}

pub struct Job {
    pub project: String,
    pub shared: Shared,
    pub kind: JobKind,
}

/// How the last job of a project ended, shown under its button (never a toast).
pub struct Outcome {
    pub ok: bool,
    pub text: String,
    pub at: f64,
    pub kind: JobKind,
}

pub struct App {
    pub catalog: Catalog,
    pub incoming: Slot<Catalog>,
    pub themes: Themes,
    pub images: Images,
    pub games: HashMap<&'static str, GameState>,
    pub voices: HashMap<&'static str, Slot<Result<Option<crate::voices::VoiceRelease>, String>>>,
    /// The retro games' patches PatchRetro serves (asked for at start).
    pub retro: HashMap<String, Slot<crate::retro::Recipe>>,
    pub reviews: HashMap<String, Slot<Reviews>>,
    pub mode: Mode,
    pub mode_changed: f64,
    pub page: Option<Page>,
    pub flight: Option<Flight>,
    /// The news column (wide windows): folded by default, opened by its tab.
    pub news_open: bool,
    /// How open it is this frame (0 folded, 1 open), animated.
    pub news_shown: f32,
    pub selected: usize,
    pub previous: usize,
    pub direction: f32,
    pub changed: f64,
    pub scroll: f32,
    pub from_theme: Theme,
    pub query: String,
    pub filter: Filter,
    pub grid_changed: f64,
    pub job: Option<Job>,
    pub outcomes: HashMap<String, Outcome>,
    pub reduced: bool,
    pub now: f64,
    pub dt: f32,
    pub moving: bool,
    pub wheel: f32,
    /// Where the home's picture is, and the grid cards' (for flights).
    pub art: ([Pos2; 4], Rect),
    pub cards: HashMap<usize, Rect>,
    pub focus_search: bool,
    pub screen: Rect,
    pub logo: egui::TextureHandle,
    pub player: crate::video::Player,
    /// A newer installer, once GitHub answered; and its download in progress.
    pub update: Slot<crate::update::Update>,
    pub updating: Slot<Result<(), String>>,
    pub update_started: bool,
    /// The trailer the page asks for this frame.
    pub trailer: Option<crate::video::Request>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum HomeAction {
    Install,
    Play,
    Page,
    Site,
    /// A creation, played in the browser.
    Browser,
    /// A retro game's floppies, translated here.
    Floppies,
    Nothing,
}

impl App {
    pub fn new(creation: &eframe::CreationContext<'_>) -> Self {
        let context = creation.egui_ctx.clone();
        setup_style(&context);
        let incoming: Slot<Catalog> = Arc::default();
        {
            let (incoming, context) = (incoming.clone(), context.clone());
            std::thread::spawn(move || {
                if let Some(catalog) = catalog::fetch() {
                    *incoming.lock().unwrap() = Some(catalog);
                    context.request_repaint();
                }
            });
        }
        let games = crate::games::GAMES
            .iter()
            .map(|game| {
                let folder = detect::find(game);
                let installed = folder.as_ref().map(|(path, _)| patch::installed(game, path)).unwrap_or(Installed::Nothing);
                let release: Slot<Result<Option<Release>, String>> = Arc::default();
                {
                    let (release, context) = (release.clone(), context.clone());
                    std::thread::spawn(move || {
                        *release.lock().unwrap() = Some(patch::release(&Source::for_game(game)));
                        context.request_repaint();
                    });
                }
                (game.project, GameState { folder, installed, release })
            })
            .collect();
        let voices = crate::games::GAMES
            .iter()
            .filter(|game| game.voices)
            .map(|game| {
                let slot: Slot<Result<Option<crate::voices::VoiceRelease>, String>> = Arc::default();
                let (inner, context) = (slot.clone(), context.clone());
                std::thread::spawn(move || {
                    *inner.lock().unwrap() = Some(crate::voices::release(game.project));
                    context.request_repaint();
                });
                (game.project, slot)
            })
            .collect();
        let themes = Themes::load();
        let catalog = catalog::bundled();
        let retro = catalog
            .projects
            .iter()
            .filter(|project| project.series == Series::Retro)
            .map(|project| {
                let slot: Slot<crate::retro::Recipe> = Arc::default();
                let (inner, context, id) = (slot.clone(), context.clone(), project.id.clone());
                std::thread::spawn(move || {
                    if let Some(recipe) = crate::retro::platform(&id) {
                        *inner.lock().unwrap() = Some(recipe);
                        context.request_repaint();
                    }
                });
                (project.id.clone(), slot)
            })
            .collect();
        let from_theme = themes.of(catalog.projects.first().map(|project| project.id.as_str()).unwrap_or("default"));
        App {
            catalog,
            incoming,
            themes,
            images: Images::new(),
            games,
            voices,
            retro,
            reviews: HashMap::new(),
            mode: Mode::Discover,
            mode_changed: -10.0,
            page: None,
            flight: None,
            news_open: false,
            news_shown: 0.0,
            selected: 0,
            previous: 0,
            direction: 1.0,
            changed: -10.0,
            scroll: 0.0,
            from_theme,
            query: String::new(),
            filter: Filter::All,
            grid_changed: -10.0,
            job: None,
            outcomes: HashMap::new(),
            reduced: motion::reduced_motion(),
            now: 0.0,
            dt: 0.016,
            moving: false,
            wheel: 0.0,
            art: ([Pos2::ZERO; 4], Rect::NOTHING),
            cards: HashMap::new(),
            focus_search: false,
            screen: Rect::from_min_size(Pos2::ZERO, vec2(1180.0, 760.0)),
            logo: {
                let image = image::load_from_memory(include_bytes!("../assets/logo.webp")).expect("logo").to_rgba8();
                let size = [image.width() as usize, image.height() as usize];
                context.load_texture("logo", egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw()), egui::TextureOptions::LINEAR)
            },
            player: crate::video::Player::default(),
            update: {
                crate::update::clean();
                let slot: Slot<crate::update::Update> = Arc::default();
                let (inner, context) = (slot.clone(), context.clone());
                std::thread::spawn(move || {
                    if let Some(update) = crate::update::check() {
                        *inner.lock().unwrap() = Some(update);
                        context.request_repaint();
                    }
                });
                slot
            },
            updating: Arc::default(),
            update_started: false,
            trailer: None,
        }
    }

    /// Progress (0 to 1) of an animation that began at `since` and lasts `seconds`;
    /// keeps the window redrawing until it ends. Instant with reduced motion.
    pub fn progress(&mut self, since: f64, seconds: f32) -> f32 {
        if self.reduced {
            return 1.0;
        }
        let t = ((self.now - since) as f32 / seconds).max(0.0);
        if t < 1.0 {
            self.moving = true;
        }
        t.min(1.0)
    }

    /// Seconds since `since`, the window kept redrawing up to `until`.
    pub fn since(&mut self, since: f64, until: f32) -> f32 {
        let t = (self.now - since) as f32;
        if t < until && !self.reduced {
            self.moving = true;
        }
        if self.reduced { until } else { t }
    }

    pub fn theme(&self, index: usize) -> Theme {
        self.catalog.projects.get(index).map(|project| self.themes.of(&project.id)).unwrap_or_else(|| self.themes.of("default"))
    }

    /// The theme on screen now (blended during a change).
    pub fn current_theme(&mut self) -> Theme {
        let target = match &self.page {
            Some(page) => self.theme(page.project),
            None => self.theme(self.selected),
        };
        let t = self.progress(self.changed, THEME_TIME);
        mix_theme(&self.from_theme, &target, ease_in_out(t))
    }

    pub fn select(&mut self, index: usize) {
        if index == self.selected || index >= self.catalog.projects.len() {
            return;
        }
        self.from_theme = self.current_theme();
        self.direction = if index > self.selected { 1.0 } else { -1.0 };
        self.previous = self.selected;
        self.selected = index;
        self.changed = self.now;
    }

    pub fn state(&self, project: &Project) -> Option<&GameState> {
        project.game.and_then(|game| self.games.get(game.project))
    }

    pub fn release(&self, project: &Project) -> Option<Result<Option<Release>, String>> {
        self.state(project).and_then(|state| state.release.lock().unwrap().clone())
    }

    /// One short line on the patch: what the home and the cards say.
    pub fn patch_line(&self, project: &Project) -> (String, bool) {
        let Some(state) = self.state(project) else {
            return (if project.status_label.is_empty() { project.platform.clone() } else { project.status_label.to_uppercase() }, false);
        };
        let release = self.release(project);
        match (&state.installed, &release) {
            (Installed::Version(version), Some(Ok(Some(release)))) if *version == release.build => ("INSTALLÉ · À JOUR".into(), true),
            (Installed::Version(_), Some(Ok(Some(release)))) => (format!("MISE À JOUR {}", release.version), true),
            (Installed::Version(version), _) => (format!("INSTALLÉ · {}", self.installed_label(project).unwrap_or_else(|| version.clone())), true),
            (_, Some(Ok(Some(release)))) => (format!("PATCH {} DISPONIBLE", release.version), false),
            (_, None) => ("…".into(), false),
            _ => (if project.status_label.is_empty() { "EN PRÉPARATION".into() } else { project.status_label.to_uppercase() }, false),
        }
    }

    pub fn is_installed(&self, project: &Project) -> bool {
        self.state(project).is_some_and(|state| !matches!(state.installed, Installed::Nothing))
    }

    pub fn visible(&self) -> Vec<usize> {
        self.catalog
            .projects
            .iter()
            .enumerate()
            .filter(|(_, project)| match self.filter {
                Filter::All => true,
                Filter::Series(series) => project.series == series,
                Filter::Installed => self.is_installed(project),
            })
            .filter(|(_, project)| self.query.trim().is_empty() || project.matches(&self.query))
            .map(|(index, _)| index)
            .collect()
    }

    pub fn open(&mut self, index: usize, from: ([Pos2; 4], Rect)) {
        if index != self.selected {
            self.from_theme = self.current_theme();
            self.selected = index;
            self.changed = self.now;
        }
        let project = self.catalog.projects[index].id.clone();
        if !self.reviews.contains_key(&project) {
            let slot: Slot<Reviews> = Arc::default();
            let (inner, id) = (slot.clone(), project.clone());
            std::thread::spawn(move || *inner.lock().unwrap() = Some(crate::reviews::fetch(&id)));
            self.reviews.insert(project, slot);
        }
        self.page = Some(Page {
            project: index,
            tab: Tab::Overview,
            tab_changed: -10.0,
            tab_direction: 1.0,
            opened: self.now,
            scroll: 0.0,
            scroll_target: 0.0,
            scroll_max: 0.0,
            scrolled_at: -10.0,
            gallery: 0,
            gallery_previous: 0,
            gallery_changed: -10.0,
            lightbox: None,
            lightbox_from: Rect::NOTHING,
            confirm: None,
            composer: false,
            draft: String::new(),
            rating: 0,
            author: String::new(),
            sending: None,
            send_error: None,
        });
        self.flight = Some(Flight { from: from.0, from_placed: from.1, started: self.now, closing: false, project: index });
    }

    pub fn close(&mut self, to: Mode) {
        let Some(page) = self.page.take() else { return };
        self.mode = to;
        let (poly, placed) = crate::page::hero_geometry(self, page.project);
        self.flight = Some(Flight { from: poly, from_placed: placed, started: self.now, closing: true, project: page.project });
        if to == Mode::Library {
            self.grid_changed = self.now;
        }
    }

    /// Starts a job on a patched game: the patch, or its voices.
    pub fn start(&mut self, context: &egui::Context, index: usize, kind: JobKind) {
        let project = &self.catalog.projects[index];
        let Some(game) = project.game else { return };
        let Some((folder, _)) = self.games.get(game.project).and_then(|state| state.folder.clone()) else { return };
        let source = Source::for_game(game);
        let voices = self.voice_release(project).and_then(Result::ok).flatten();
        let id = project.id.clone();
        self.spawn(context, id, kind, move |progress| match kind {
            JobKind::Install => patch::install(game, &folder, &source, progress),
            JobKind::Uninstall => patch::uninstall(&folder, progress),
            JobKind::Voices => match &voices {
                Some(voices) => crate::voices::install(game, &folder, voices, progress),
                None => Err("Les voix de ce jeu ne sont pas encore disponibles.".into()),
            },
            JobKind::RemoveVoices => crate::voices::uninstall(&folder),
            JobKind::Floppies => Err(String::new()),
        });
    }

    /// Translates a retro game's floppies.
    pub fn start_floppies(&mut self, context: &egui::Context, index: usize, disks: Vec<PathBuf>) {
        let project = &self.catalog.projects[index];
        let Some(recipe) = self.recipe(project) else { return };
        let title = project.title.clone();
        self.spawn(context, project.id.clone(), JobKind::Floppies, move |progress| {
            let (message, folder) = crate::retro::translate(&title, &recipe, &disks, progress)?;
            open_folder(&folder);
            Ok(message)
        });
    }

    fn spawn(&mut self, context: &egui::Context, project: String, kind: JobKind, work: impl FnOnce(&Shared) -> Result<String, String> + Send + 'static) {
        if self.job.is_some() {
            return;
        }
        let shared: Shared = Arc::default();
        let (progress, context) = (shared.clone(), context.clone());
        std::thread::spawn(move || {
            let result = work(&progress);
            progress.lock().unwrap().finished = Some(result);
            context.request_repaint();
        });
        self.outcomes.remove(&project);
        self.job = Some(Job { project, shared, kind });
    }

    /// How a retro game's floppies are translated: PatchRetro's recipe, else projects.json's.
    pub fn recipe(&self, project: &Project) -> Option<crate::retro::Recipe> {
        self.retro
            .get(&project.id)
            .and_then(|slot| slot.lock().unwrap().clone())
            .or_else(|| project.sheet.install.as_ref().and_then(crate::retro::Recipe::read))
    }

    pub fn voice_release(&self, project: &Project) -> Option<Result<Option<crate::voices::VoiceRelease>, String>> {
        project.game.and_then(|game| self.voices.get(game.project)).and_then(|slot| slot.lock().unwrap().clone())
    }

    fn poll(&mut self, context: &egui::Context) {
        if let Some(catalog) = self.incoming.lock().unwrap().take() {
            let selected = self.catalog.projects.get(self.selected).map(|project| project.id.clone());
            let page_id = self.page.as_ref().and_then(|page| self.catalog.projects.get(page.project)).map(|project| project.id.clone());
            self.catalog = catalog;
            let find = |id: &Option<String>, catalog: &Catalog| id.as_ref().and_then(|id| catalog.projects.iter().position(|project| &project.id == id));
            self.selected = find(&selected, &self.catalog).unwrap_or(0);
            self.previous = self.selected;
            self.scroll = self.selected as f32;
            if let Some(page) = &mut self.page {
                match find(&page_id, &self.catalog) {
                    Some(index) => page.project = index,
                    None => self.page = None,
                }
            }
        }
        self.images.poll(context, self.now);
        let finished = self.job.as_ref().and_then(|job| job.shared.lock().unwrap().finished.take().map(|result| (job.project.clone(), job.kind, result)));
        if let Some((id, kind, result)) = finished {
            self.job = None;
            if let Some(game) = self.catalog.projects.iter().find(|project| project.id == id).and_then(|project| project.game) {
                if let Some(state) = self.games.get_mut(game.project) {
                    if let Some((folder, _)) = &state.folder {
                        state.installed = patch::installed(game, folder);
                    }
                }
            }
            let (ok, text) = match result {
                Ok(text) => (true, text),
                Err(text) => (false, text),
            };
            self.outcomes.insert(id, Outcome { ok, text, at: self.now, kind });
        }
    }

    fn keys(&mut self, context: &egui::Context) {
        let typing = context.egui_wants_keyboard_input();
        let (left, right, enter, escape, find) = context.input(|input| {
            (
                input.key_pressed(Key::ArrowLeft),
                input.key_pressed(Key::ArrowRight),
                input.key_pressed(Key::Enter),
                input.key_pressed(Key::Escape),
                input.modifiers.command && input.key_pressed(Key::F),
            )
        });
        if find {
            self.focus_search = true;
        }
        if let Some(page) = &mut self.page {
            if escape {
                if page.lightbox.is_some() {
                    page.lightbox = None;
                } else if page.confirm.is_some() {
                    page.confirm = None;
                } else if !typing {
                    let mode = self.mode;
                    self.close(mode);
                }
            }
            return;
        }
        if typing {
            if escape {
                self.query.clear();
                self.grid_changed = self.now;
            }
            return;
        }
        if self.mode == Mode::Discover {
            if left && self.selected > 0 {
                self.select(self.selected - 1);
            }
            if right {
                self.select((self.selected + 1).min(self.catalog.projects.len().saturating_sub(1)));
            }
            if enter {
                let art = self.art;
                self.open(self.selected, art);
            }
        } else if escape && !self.query.is_empty() {
            self.query.clear();
            self.grid_changed = self.now;
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, root: &mut egui::Ui, frame: &mut eframe::Frame) {
        let context = root.ctx().clone();
        self.now = context.input(|input| input.time);
        self.dt = context.input(|input| input.stable_dt).min(0.1);
        self.moving = false;
        self.trailer = None;
        self.poll(&context);
        self.keys(&context);
        if self.catalog.projects.is_empty() {
            return;
        }
        self.selected = self.selected.min(self.catalog.projects.len() - 1);

        egui::CentralPanel::default().frame(egui::Frame::new()).show(root, |ui| {
            let screen = ui.max_rect();
            self.screen = screen;
            let theme = self.current_theme();
            apply_theme(&context, &theme);
            self.background(ui.painter(), screen, &theme);
            if self.page.is_some() {
                crate::page::show(self, ui, screen, &theme);
            } else {
                // The home comes back under a closing page: faded in.
                let back = self.flight.as_ref().filter(|flight| flight.closing).map(|flight| flight.started);
                let appear = back.map(|since| self.progress(since + 0.12, 0.35)).unwrap_or(1.0);
                ui.set_opacity(appear);
                match self.mode {
                    Mode::Discover => self.discover(ui, screen, &theme),
                    Mode::Library => self.library(ui, screen, &theme),
                }
                ui.set_opacity(1.0);
                if self.mode == Mode::Library {
                    let painter = ui.painter().clone();
                    self.flight_back(&painter, &context, screen);
                }
            }
            self.top_bar(&context, screen, &theme);
        });
        // The trailer view follows what the page asked for (gone when it asked nothing).
        let request = self.trailer.take();
        self.player.update(frame, &context, request);
        if self.moving || self.job.is_some() {
            context.request_repaint();
        }
    }
}

impl App {
    /// The window's ground: the previous theme, the new one wiped across it.
    fn background(&mut self, painter: &egui::Painter, screen: Rect, theme: &Theme) {
        let target = match &self.page {
            Some(page) => self.theme(page.project),
            None => self.theme(self.selected),
        };
        let p = self.progress(self.changed, THEME_TIME);
        let e = ease_in_out(p);
        if p >= 1.0 {
            painter.rect_filled(screen, 0.0, target.bg);
            motion::motif(painter, screen, &target, 1.0, 0.0);
            return;
        }
        let from = self.from_theme;
        painter.rect_filled(screen, 0.0, from.bg);
        motion::motif(painter, screen, &from, 1.0 - e, -self.direction * e * 50.0);
        let (w, h) = (screen.width(), screen.height());
        let s = h * SLANT;
        let d = self.direction;
        match target.transition {
            Transition::Slash => {
                let edge = if d > 0.0 { lerp(w + 10.0, -s - 30.0, e) } else { lerp(-s - 30.0, w + 10.0, e) };
                let region = if d > 0.0 {
                    [pos2(edge + s, 0.0), pos2(w + s + 40.0, 0.0), pos2(w + s + 40.0, h), pos2(edge, h)]
                } else {
                    [pos2(-s - 40.0, 0.0), pos2(edge + s, 0.0), pos2(edge, h), pos2(-s - 40.0, h)]
                };
                fill(painter, &region.map(|p| p + screen.min.to_vec2()), target.bg);
                let blade = 26.0 * (1.0 - (2.0 * p - 1.0).abs());
                let side = if d > 0.0 { -blade } else { blade };
                let stripe = [pos2(edge + s, 0.0), pos2(edge + s + side, 0.0), pos2(edge + side, h), pos2(edge, h)];
                fill(painter, &stripe.map(|p| p + screen.min.to_vec2()), theme.accent2);
            }
            Transition::Shutter => {
                let blades = 5;
                let width = (w + s) / blades as f32;
                for k in 0..blades {
                    let order = if d > 0.0 { blades - 1 - k } else { k };
                    let local = ease_out(clamp01((p - order as f32 * 0.09) / 0.55));
                    let x0 = k as f32 * width - s;
                    let grown = width * local;
                    let (a, b) = if d > 0.0 { (x0 + width - grown, x0 + width) } else { (x0, x0 + grown) };
                    let region = [pos2(a + s, 0.0), pos2(b + s + 1.0, 0.0), pos2(b + 1.0, h), pos2(a, h)];
                    fill(painter, &region.map(|p| p + screen.min.to_vec2()), target.bg);
                }
            }
            Transition::Scan => {
                let bars = 12;
                let height = h / bars as f32;
                for j in 0..bars {
                    let local = ease_out(clamp01((p - j as f32 * 0.035) / 0.5));
                    let rect = Rect::from_min_size(screen.min + vec2(0.0, j as f32 * height), vec2(w * local, height + 1.0));
                    painter.rect_filled(rect, 0.0, target.bg);
                    if local > 0.0 && local < 1.0 {
                        painter.rect_filled(Rect::from_min_size(pos2(rect.max.x - 3.0, rect.min.y), vec2(3.0, height)), 0.0, target.accent);
                    }
                }
            }
        }
        motion::motif(painter, screen, &target, e, d * (1.0 - e) * 60.0);
    }

    /// Where the home's picture goes: the right of the window, its left edge slanted.
    pub fn art_geometry(screen: Rect) -> ([Pos2; 4], Rect) {
        let bottom = screen.max.y - BAND + 24.0;
        let area = Rect::from_min_max(pos2(screen.min.x + screen.width() * 0.34, screen.min.y), pos2(screen.max.x, bottom));
        let s = area.height() * SLANT;
        let poly = [pos2(area.min.x + s, area.min.y), pos2(area.max.x, area.min.y), pos2(area.max.x, area.max.y), pos2(area.min.x - 0.0, area.max.y)];
        (poly, area)
    }

    fn discover(&mut self, ui: &mut egui::Ui, full: Rect, theme: &Theme) {
        self.news_shown = if self.reduced { if self.news_open { 1.0 } else { 0.0 } } else { ui.ctx().animate_bool_with_time(Id::new("news-open"), self.news_open, 0.32) };
        if self.news_shown > 0.0 && self.news_shown < 1.0 {
            self.moving = true;
        }
        let (screen, panel) = self.split(full);
        let painter = ui.painter().clone();
        let p = self.progress(self.changed, THEME_TIME);
        let d = self.direction;
        let (poly, area) = Self::art_geometry(screen);

        // The pictures: the old one leaves, the new one comes in a little later.
        let closing = self.flight.as_ref().filter(|flight| flight.closing).map(|flight| flight.started);
        let flying = closing.is_some_and(|started| self.progress(started, 0.45) < 1.0);
        if p < 1.0 && self.previous != self.selected {
            let out = ease_in_out(clamp01(p / 0.6));
            self.draw_art(&painter, self.previous, &poly, area, vec2(-d * out * 70.0, 0.0), 1.0 - out, 1.04);
        }
        let enter = ease_out_quint(clamp01((p - 0.12) / 0.75));
        if flying {
            // The picture coming back from the page lands here, under the melt and the blade.
            let context = ui.ctx().clone();
            self.flight_back(&painter, &context, full);
        } else {
            self.draw_art(&painter, self.selected, &poly, area, vec2(d * (1.0 - enter) * 90.0, 0.0), enter, 1.0 + 0.06 * (1.0 - enter));
        }
        self.art = self.art_target(self.selected, full).unwrap_or((poly, area));

        // The picture melts into the ground on its left and at its bottom.
        let s = area.height() * SLANT;
        let melt = if theme.light { 0.2 } else { 0.42 };
        motion::gradient_rect(&painter, Rect::from_min_max(pos2(area.min.x - 2.0, area.min.y), pos2(area.min.x + s + area.width() * melt, area.max.y)), theme.bg, alpha(theme.bg, 0.0), false);
        motion::gradient_rect(&painter, Rect::from_min_max(pos2(area.min.x, area.max.y - area.height() * if theme.light { 0.18 } else { 0.32 }), area.max), alpha(theme.bg, 0.0), theme.bg, true);
        // The blade along its edge, with its hard shadow.
        let edge = |offset: f32, width: f32| [poly[0] + vec2(offset, 0.0), poly[0] + vec2(offset + width, 0.0), poly[3] + vec2(offset + width, 0.0), poly[3] + vec2(offset, 0.0)];
        let reach = ease_out(clamp01((p - 0.05) / 0.5));
        let cut = |points: [Pos2; 4]| {
            let bottom = lerp(points[0].y, points[3].y, reach);
            [points[0], points[1], pos2(lerp(points[1].x, points[2].x, reach), bottom), pos2(lerp(points[0].x, points[3].x, reach), bottom)]
        };
        fill(&painter, &cut(edge(-34.0, 8.0)), theme.accent2);
        fill(&painter, &cut(edge(-22.0, 10.0)), theme.accent);

        let project = self.catalog.projects[self.selected].clone();
        let left = screen.min.x + 56.0;
        let column = (area.min.x - left - 20.0).max(300.0);
        let t = self.since(self.changed, 1.6) - 0.1;

        // Kicker: an inverted label on a slanted bar that unrolls.
        let kicker = format!("{}  ·  {}  ·  {}", project.series.label().to_uppercase(), project.year, project.platform.to_uppercase());
        let kicker_font = motion::semibold(13.0);
        let kicker_width = spaced_width(&painter, &kicker, &kicker_font, 2.2) + 26.0;
        let top = screen.min.y + (area.height() * 0.26).max(TOP + 30.0);
        let unroll = ease_out_quint(clamp01(t / 0.45));
        let bar = Rect::from_min_size(pos2(left - 10.0, top), vec2(kicker_width * unroll, 26.0));
        fill(&painter, &slanted(bar.translate(vec2(5.0, 5.0)), SLANT), alpha(theme.accent2, 0.9));
        fill(&painter, &slanted(bar, SLANT), theme.accent);
        let clip = painter.with_clip_rect(bar);
        spaced(&clip, pos2(left + 4.0, top + 13.0), &kicker, &kicker_font, theme.bg, 2.2);
        // The series opens the library on its shelf.
        let series_label = project.series.label().to_uppercase();
        let series_width = spaced_width(&painter, &series_label, &kicker_font, 2.2);
        let series_rect = Rect::from_min_size(pos2(left - 2.0, top), vec2(series_width + 12.0, 26.0));
        let series = ui.interact(series_rect, Id::new("kicker-series"), Sense::click());
        if series.hovered() && unroll >= 1.0 {
            ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
            let y = top + 21.0;
            clip.line_segment([pos2(left + 4.0, y), pos2(left + 4.0 + series_width, y)], Stroke::new(1.5, theme.bg));
        }
        if series.on_hover_text("Voir la série dans la bibliothèque").clicked() {
            self.filter = Filter::Series(project.series);
            self.query.clear();
            self.set_mode(Mode::Library);
        }

        // The title, letter after letter, as big as it fits.
        let title = project.title.to_uppercase();
        // Below the title: the patch line, two lines of description, the button.
        let room = (screen.max.y - BAND - 16.0) - (top + 40.0) - 44.0 - (if project.sheet.description.is_empty() { 8.0 } else { 66.0 }) - 92.0;
        let mut size = 96.0;
        let mut lines = motion::wrap(&painter, &title, &motion::display(size), column);
        while (lines.len() > 3 || lines.len() as f32 * size > room || lines.iter().any(|line| motion::width(&painter, line, &motion::display(size)) > column)) && size > 36.0 {
            size -= 4.0;
            lines = motion::wrap(&painter, &title, &motion::display(size), column);
        }
        let font = motion::display(size);
        let mut y = top + 40.0;
        if p < 1.0 && self.previous != self.selected && t < 0.2 {
            let old = self.catalog.projects[self.previous].title.to_uppercase();
            for (index, line) in motion::wrap(&painter, &old, &font, column).iter().enumerate() {
                motion::letters(&painter, pos2(left, y + index as f32 * size * 1.0), line, font.clone(), theme.ink, t + 0.1, 0.0, true);
            }
        } else {
            let mut delay = 0.0;
            for line in &lines {
                // Once its last letter has landed, the line is drawn whole (the same picture).
                if self.reduced || t > delay + line.chars().count() as f32 * 0.022 + 0.56 {
                    painter.text(pos2(left + 4.0, y + 4.0), Align2::LEFT_TOP, line, font.clone(), alpha(theme.accent2, 0.55));
                    painter.text(pos2(left, y), Align2::LEFT_TOP, line, font.clone(), theme.ink);
                } else {
                    // Each letter rises with its shadow, so the shadow never pops in at the end.
                    motion::letters(&painter, pos2(left + 4.0, y + 4.0), line, font.clone(), alpha(theme.accent2, 0.55), t - 0.12 - delay, 0.022, false);
                    motion::letters(&painter, pos2(left, y), line, font.clone(), theme.ink, t - 0.12 - delay, 0.022, false);
                }
                delay += 0.08;
                y += size * 1.0;
            }
        }

        // What the patch is, in one line, then the way in.
        let reveal = ease_out(clamp01((t - 0.35) / 0.4));
        let shift = (1.0 - reveal) * 16.0;
        let (line, installed) = self.patch_line(&project);
        y += 14.0;
        let dot = if installed { theme.accent } else { theme.muted };
        painter.circle_filled(pos2(left + 5.0, y + 9.0 + shift), 4.0, alpha(dot, reveal));
        spaced(&painter, pos2(left + 18.0, y + 9.0 + shift), &line, &motion::semibold(13.0), alpha(theme.ink, reveal), 1.6);
        y += 30.0;
        if !project.sheet.description.is_empty() {
            let text = shorten(&project.sheet.description, 110);
            let galley = painter.layout(text, motion::body(15.0), alpha(theme.muted, reveal), column.min(470.0));
            painter.galley(pos2(left, y + shift), galley.clone(), theme.muted);
            y += galley.size().y + 22.0;
        } else {
            y += 8.0;
        }
        // The way in is the patch: install it, update it, play; a creation opens its page.
        let buttons = ease_out(clamp01((t - 0.45) / 0.4));
        let at = pos2(left, y + (1.0 - buttons) * 16.0);
        let (label, action) = self.home_action(&project);
        let enabled = action != HomeAction::Nothing;
        if slant_button(ui, &painter, Id::new("main"), at, &label, theme, enabled, buttons, self.now) {
            let art = self.art;
            match action {
                HomeAction::Install => {
                    self.open(self.selected, art);
                    let context = ui.ctx().clone();
                    self.start(&context, self.selected, JobKind::Install);
                }
                HomeAction::Play => crate::page::play(self, &project),
                HomeAction::Site => open_url(&project.page),
                HomeAction::Browser => open_url(project.link.as_deref().unwrap_or(&project.page)),
                HomeAction::Floppies => {
                    self.open(self.selected, art);
                    let context = ui.ctx().clone();
                    crate::page::pick_floppies(self, &context, &project);
                }
                HomeAction::Page | HomeAction::Nothing => self.open(self.selected, art),
            }
        }
        if action != HomeAction::Page {
            // The page is always one click away.
            let link = Rect::from_min_size(pos2(at.x + 4.0, at.y + 58.0), vec2(150.0, 26.0));
            let response = ui.interact(link, Id::new("sheet-link"), Sense::click());
            let hover = ui.ctx().animate_bool_with_time(response.id, response.hovered(), 0.14);
            let colour = alpha(mix(theme.muted, theme.accent, hover), buttons);
            let used = spaced(&painter, pos2(link.min.x, link.center().y), "FICHE DU JEU", &motion::semibold(13.0), colour, 2.0);
            motion::icon(&painter, Icon::Right, pos2(link.min.x + used + 10.0 + 4.0 * hover, link.center().y), 11.0, colour);
            if response.hovered() {
                ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
            }
            if response.clicked() {
                let art = self.art;
                self.open(self.selected, art);
            }
        }

        let wide = full.width() >= PANEL_FROM;
        self.band(ui, screen, theme, !wide);
        if let Some(panel) = panel {
            self.news_panel(ui, panel, theme);
        }
        if wide {
            self.news_tab(ui, pos2(screen.max.x, full.min.y + TOP + 64.0), theme);
        }
    }

    /// The home's stage, and the news column beside it when the window is wide and the column
    /// open (it slides in from the right: its rectangle keeps its width, partly off the window).
    pub fn split(&self, screen: Rect) -> (Rect, Option<Rect>) {
        if screen.width() < PANEL_FROM || self.news_shown <= 0.0 {
            return (screen, None);
        }
        let open = PANEL * ease_out(self.news_shown);
        let stage = Rect::from_min_max(screen.min, pos2(screen.max.x - open, screen.max.y));
        (stage, Some(Rect::from_min_size(pos2(stage.max.x, screen.min.y), vec2(PANEL, screen.height()))))
    }

    /// The tab on the stage's right edge that unfolds (or folds) the news.
    fn news_tab(&mut self, ui: &mut egui::Ui, edge: Pos2, theme: &Theme) {
        let painter = ui.painter().clone();
        let rect = Rect::from_min_size(pos2(edge.x - 34.0, edge.y), vec2(34.0, 132.0));
        let response = ui.interact(rect, Id::new("news-tab"), Sense::click());
        let hover = ui.ctx().animate_bool_with_time(response.id, response.hovered(), 0.14);
        let shape = [rect.min + vec2(0.0, 10.0), pos2(rect.max.x, rect.min.y), rect.max, pos2(rect.min.x, rect.max.y - 10.0)];
        fill(&painter, &shape.map(|p| p + vec2(-4.0, 4.0)), alpha(theme.accent2, 0.6 + 0.4 * hover));
        fill(&painter, &shape, mix(theme.accent, Color32::WHITE, 0.12 * hover));
        let arrow = if self.news_open { Icon::Right } else { Icon::Left };
        motion::icon(&painter, arrow, pos2(rect.center().x - 2.0 * hover, rect.min.y + 26.0), 11.0, theme.bg);
        let galley = painter.layout_no_wrap("NOUVELLES".to_string(), motion::semibold(11.0), theme.bg);
        let at = pos2(rect.center().x - galley.size().y / 2.0, rect.max.y - 18.0);
        painter.add(egui::epaint::TextShape::new(at, galley, theme.bg).with_angle(-std::f32::consts::FRAC_PI_2));
        if response.hovered() {
            ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
        }
        if response.on_hover_text(if self.news_open { "Replier les nouvelles" } else { "Afficher les nouvelles" }).clicked() {
            self.news_open = !self.news_open;
        }
    }

    /// What the home's button does for a project.
    fn home_action(&self, project: &Project) -> (String, HomeAction) {
        if project.series == Series::Other {
            return match project.link {
                Some(_) => ("JOUER".into(), HomeAction::Browser),
                None => ("OUVRIR LA FICHE".into(), HomeAction::Page),
            };
        }
        let Some(state) = self.state(project) else {
            // Floppies translated here, a patch the site distributes, or one still in the works.
            if self.recipe(project).is_some_and(|recipe| recipe.local()) {
                return ("TRADUIRE MES DISQUETTES".into(), if self.job.is_none() { HomeAction::Floppies } else { HomeAction::Page });
            }
            return if project.status_label.to_lowercase().contains("termin") {
                ("TÉLÉCHARGER LE PATCH".into(), HomeAction::Site)
            } else {
                ("TRADUCTION EN COURS".into(), HomeAction::Nothing)
            };
        };
        if self.job.as_ref().is_some_and(|job| job.project == project.id) {
            return ("INSTALLATION…".into(), HomeAction::Page);
        }
        let release = self.release(project);
        match (&state.installed, &release) {
            (Installed::Version(version), Some(Ok(Some(release)))) if *version == release.build => ("JOUER".into(), HomeAction::Play),
            (Installed::Version(_), Some(Ok(Some(_)))) => ("METTRE À JOUR".into(), HomeAction::Install),
            (_, Some(Ok(Some(_)))) if state.folder.is_some() => ("INSTALLER LE PATCH".into(), HomeAction::Install),
            (_, Some(Ok(Some(_)))) => ("INSTALLER LE PATCH".into(), HomeAction::Page),
            // Installed, no public release yet (a test version): it plays.
            (Installed::Version(_), _) => ("JOUER".into(), HomeAction::Play),
            (_, None) => ("…".into(), HomeAction::Nothing),
            _ => ("PATCH À VENIR".into(), HomeAction::Nothing),
        }
    }

    /// The installed version as players know it: the release's number when the build is a release.
    pub fn installed_label(&self, project: &Project) -> Option<String> {
        let state = self.state(project)?;
        match &state.installed {
            Installed::Version(build) => Some(match self.release(project) {
                Some(Ok(Some(release))) if release.build == *build => release.version,
                _ => build.clone(),
            }),
            _ => None,
        }
    }

    /// Where the home's picture lies for a project, as draw_art lays it (None until it is loaded).
    pub fn art_target(&self, index: usize, full: Rect) -> Option<([Pos2; 4], Rect)> {
        let (poly, area) = Self::art_geometry(self.split(full).0);
        let project = self.catalog.projects.get(index)?;
        let url = project.key_art().filter(|url| !self.images.failed(url)).or_else(|| project.thumbnail())?;
        let size = self.images.size(&url)?;
        let pixel = self.themes.of(&project.id).pixel;
        Some(Self::art_shape(size, pixel, project.art_focus(), poly, area, Vec2::ZERO, 1.0))
    }

    /// The picture's outline and placement: covering the stage, or a small picture as a poster.
    #[allow(clippy::too_many_arguments)]
    fn art_shape(size: Vec2, pixel: bool, focus: f32, poly: [Pos2; 4], area: Rect, offset: Vec2, zoom: f32) -> ([Pos2; 4], Rect) {
        if size.x < 800.0 && !pixel {
            let room = Rect::from_min_max(pos2(area.min.x + area.width() * 0.22, area.min.y + TOP + 40.0), pos2(area.max.x - 50.0, area.max.y - 90.0));
            let scale = (room.width() / size.x).min(room.height() / size.y).min(1.6);
            let placed = Rect::from_center_size(room.center(), size * scale * zoom).translate(offset);
            return (slanted(placed, SLANT * 0.3), placed);
        }
        let s = area.height() * SLANT;
        // What is seen of the stage: right of the slant, under the top bar, left of the news tab.
        let room = Rect::from_min_max(pos2(area.min.x + s, area.min.y + TOP), pos2(area.max.x - 44.0, area.max.y - 8.0));
        if size.x / size.y < 2.0 {
            // A screen (the retro games' title screens): whole, or barely cut, in what is seen;
            // its blurred copy fills the rest of the stage.
            let whole = (room.width() / size.x).min(room.height() / size.y);
            let full = (room.width() / size.x).max(room.height() / size.y);
            let placed = Rect::from_center_size(room.center(), size * full.min(whole * 1.02) * zoom).translate(offset);
            return (poly.map(|point| point + offset), placed);
        }
        // A wide picture (Steam's hero art) covers the stage, its point `focus` (0..1 across the
        // picture) in the middle of what is seen, never leaving a side bare.
        let scale = (area.width() / size.x).max(area.height() / size.y) * zoom;
        let shown = size * scale;
        let middle = (area.min.x + s * 0.5 + area.max.x) * 0.5;
        let left = (middle - focus * shown.x).clamp(area.max.x - shown.x, area.min.x);
        let placed = Rect::from_min_size(pos2(left, area.center().y - shown.y * 0.5), shown);
        (poly.map(|point| point + offset), placed.translate(offset))
    }

    #[allow(clippy::too_many_arguments)]
    /// A project's big picture inside `poly`; returns where the whole picture lies.
    fn draw_art(&mut self, painter: &egui::Painter, index: usize, poly: &[Pos2; 4], area: Rect, offset: Vec2, opacity: f32, zoom: f32) -> Option<Rect> {
        let project = &self.catalog.projects[index];
        let project_id = project.id.clone();
        let focus = project.art_focus();
        let pixel = self.themes.of(&project.id).pixel;
        let mut url = project.key_art()?;
        if self.images.failed(&url) {
            url = project.thumbnail()?;
        }
        let context = painter.ctx().clone();
        let (texture, size, since) = self.images.get(&context, &url, pixel).map(|(texture, size, since)| (texture.id(), size, since))?;
        let arrived = ease_out(clamp01((self.now - since) as f32 / 0.4));
        if arrived < 1.0 {
            self.moving = true;
        }
        let tint = Color32::from_white_alpha((255.0 * opacity * arrived) as u8);
        let (shape, placed) = Self::art_shape(size, pixel, focus, *poly, area, offset, zoom);
        let blur = self.images.blur(&url);
        if size.x >= 800.0 || pixel {
            if let (Some(blur), false) = (blur, placed.contains_rect(Rect::from_points(&shape))) {
                // A screen that does not cover the stage: its blurred copy behind, its edges fading.
                let bounds = Rect::from_points(&shape);
                let dim = Color32::from_white_alpha((255.0 * opacity * arrived * 0.8) as u8);
                motion::image_in(painter, blur, motion::cover(size, bounds, zoom * 1.08, Vec2::ZERO), &shape, dim);
                let visible = placed.intersect(bounds);
                let fade = (visible.width().min(visible.height()) * 0.05).clamp(2.0, 16.0);
                motion::image_soft(painter, texture, placed, visible, fade, tint);
                return Some(placed);
            }
        }
        if size.x < 800.0 && !pixel {
            // A small picture is not blown up: shown crisp, as a poster in a slanted frame.
            let frame = slanted(placed.expand(10.0), SLANT * 0.3);
            let theme = self.themes.of(&project_id);
            fill(painter, &frame.map(|p| p + vec2(12.0, 12.0)), alpha(theme.accent2, opacity * arrived));
            fill(painter, &frame, alpha(theme.accent, opacity * arrived));
        }
        motion::image_in(painter, texture, placed, &shape, tint);
        Some(placed)
    }

    /// The bottom of the home: the carousel of every project, and the news.
    fn band(&mut self, ui: &mut egui::Ui, screen: Rect, theme: &Theme, with_news: bool) {
        let painter = ui.painter().clone();
        let top = screen.max.y - BAND;
        let band = [pos2(screen.min.x, top + 22.0), pos2(screen.max.x, top), screen.max, pos2(screen.min.x, screen.max.y)];
        fill(&painter, &band, theme.surface());
        painter.line_segment([band[0], band[1]], Stroke::new(2.0, alpha(theme.accent, 0.5)));

        let news_left = if with_news { screen.max.x - NEWS_WIDTH } else { screen.max.x - 16.0 };
        let strip = Rect::from_min_max(pos2(screen.min.x + 40.0, top + 26.0), pos2(news_left - 24.0, screen.max.y - 12.0));
        let target = self.selected as f32;
        if self.reduced {
            self.scroll = target;
        } else {
            self.scroll += (target - self.scroll) * (1.0 - (-self.dt * 11.0).exp());
            if (target - self.scroll).abs() > 0.002 {
                self.moving = true;
            } else {
                self.scroll = target;
            }
        }

        // The wheel turns the carousel (one project per notch).
        let hover = ui.ctx().pointer_hover_pos();
        let over = hover.is_some_and(|pos| pos.x < news_left && pos.y > TOP);
        if over && self.page.is_none() {
            let delta = ui.ctx().input(|input| input.smooth_scroll_delta);
            self.wheel += delta.y + delta.x;
            if self.wheel.abs() >= 40.0 {
                let step = if self.wheel > 0.0 { -1i32 } else { 1 };
                self.wheel = 0.0;
                let next = (self.selected as i32 + step).clamp(0, self.catalog.projects.len() as i32 - 1) as usize;
                self.select(next);
            }
        }

        let count = self.catalog.projects.len();
        let center = strip.center().x;
        let clip = painter.with_clip_rect(strip.expand2(vec2(0.0, 12.0)));
        for index in 0..count {
            let offset = index as f32 - self.scroll;
            if offset.abs() > 4.5 {
                continue;
            }
            let active = clamp01(1.0 - offset.abs());
            let scale = 1.0 + 0.12 * active;
            let size = CARD * scale;
            let rect = Rect::from_center_size(pos2(center + offset * CARD_STEP, strip.min.y + 8.0 + CARD.y / 2.0 - 8.0 * active), size);
            let response = ui.interact(rect, Id::new(("card", index)), Sense::click());
            let lift = ui.ctx().animate_bool_with_time(response.id, response.hovered(), 0.15);
            let rect = rect.translate(vec2(0.0, -4.0 * lift));
            let shape = slanted(rect, SLANT * 0.5);
            let project = &self.catalog.projects[index];
            let card_theme = self.themes.of(&project.id);
            if active > 0.5 {
                fill(&clip, &shape.map(|p| p + vec2(6.0, 6.0)), card_theme.accent2);
            }
            fill(&clip, &shape, card_theme.shade(0.2));
            if let Some(url) = project.thumbnail() {
                let context = ui.ctx().clone();
                if let Some((texture, image, _)) = self.images.get(&context, &url, card_theme.pixel) {
                    let texture = texture.id();
                    let shade = (150.0 + 105.0 * active.max(lift)) as u8;
                    let blur = self.images.blur(&url);
                    motion::picture_in(&clip, texture, blur, image, &shape, motion::upright(rect, SLANT * 0.5).shrink2(vec2(0.0, 2.0)), 1.08, 1.0 + 0.05 * lift, Color32::from_rgb(shade, shade, shade));
                }
            }
            let distance = clamp01(offset.abs() / 4.5);
            if distance > 0.0 {
                fill(&clip, &shape, alpha(theme.surface(), distance * 0.85));
            }
            if active > 0.0 {
                clip.add(egui::Shape::closed_line(shape.to_vec(), Stroke::new(2.0 * active, alpha(card_theme.accent, active))));
            }
            if self.job.as_ref().is_some_and(|job| job.project == project.id) {
                let spin = (self.now as f32 * 4.0) % std::f32::consts::TAU;
                clip.add(egui::Shape::line(
                    (0..16).map(|k| rect.right_top() + vec2(-14.0, 14.0) + Vec2::angled(spin + k as f32 * 0.3) * 7.0).collect(),
                    Stroke::new(2.0, card_theme.accent),
                ));
            } else if self.is_installed(project) {
                clip.circle_filled(rect.right_top() + vec2(-12.0, 12.0), 4.5, card_theme.accent);
            }
            if response.hovered() {
                ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
            }
            if response.clicked() {
                if index == self.selected {
                    let art = self.art;
                    self.open(index, art);
                } else {
                    self.select(index);
                }
            }
        }
        // The active project's name under it.
        let project = &self.catalog.projects[self.selected];
        clip.text(pos2(center, strip.min.y + CARD.y * 1.12 + 10.0), Align2::CENTER_TOP, project.title.to_uppercase(), motion::semibold(12.0), alpha(theme.ink, 0.9));

        // Arrows, and where we are.
        for (icon, delta, x) in [(Icon::Left, -1i32, strip.min.x - 18.0), (Icon::Right, 1, strip.max.x + 6.0)] {
            let rect = Rect::from_center_size(pos2(x, strip.min.y + CARD.y / 2.0), vec2(30.0, 50.0));
            let next = self.selected as i32 + delta;
            let possible = next >= 0 && next < count as i32;
            let response = ui.interact(rect, Id::new(("arrow", delta)), if possible { Sense::click() } else { Sense::hover() });
            let hover = ui.ctx().animate_bool_with_time(response.id, response.hovered() && possible, 0.12);
            motion::icon(&painter, icon, rect.center() + vec2(delta as f32 * 3.0 * hover, 0.0), 18.0, alpha(mix(theme.muted, theme.accent, hover), if possible { 1.0 } else { 0.25 }));
            if response.clicked() && possible {
                self.select(next as usize);
            }
        }
        let counter = format!("{:02}", self.selected + 1);
        // Where we are, at the bottom right of the picture.
        let at = pos2(screen.max.x - 30.0, top - 8.0);
        let total = painter.text(at, Align2::RIGHT_BOTTOM, format!("/ {count:02}"), motion::display(20.0), alpha(theme.muted, 0.8));
        motion::shadowed(&painter, pos2(total.min.x - 8.0, at.y + 6.0), Align2::RIGHT_BOTTOM, &counter, motion::display(64.0), theme.accent, alpha(theme.accent2, 0.8), 4.0);

        if with_news {
            self.news(ui, Rect::from_min_max(pos2(news_left, top + 4.0), screen.max), theme);
        }
    }

    fn news(&mut self, ui: &mut egui::Ui, rect: Rect, theme: &Theme) {
        let painter = ui.painter().clone();
        let x = rect.min.x + 6.0;
        spaced(&painter, pos2(x, rect.min.y + 26.0), "DERNIÈRES NOUVELLES", &motion::semibold(11.0), theme.accent, 2.4);
        let mut y = rect.min.y + 44.0;
        let articles: Vec<catalog::Article> = self.catalog.articles.iter().take(3).cloned().collect();
        if articles.is_empty() {
            painter.text(pos2(x, y + 6.0), Align2::LEFT_TOP, "Les nouvelles du site s'afficheront ici.", motion::body(13.0), theme.muted);
        }
        for (index, article) in articles.iter().enumerate() {
            let row = Rect::from_min_size(pos2(x - 6.0, y), vec2(rect.width() - 24.0, 36.0));
            let response = ui.interact(row, Id::new(("news", index)), Sense::click());
            let hover = ui.ctx().animate_bool_with_time(response.id, response.hovered(), 0.18);
            if hover > 0.0 {
                let bar = Rect::from_min_size(row.min + vec2(0.0, 4.0), vec2(4.0 + (row.width() - 4.0) * ease_out(hover), row.height() - 6.0));
                fill(&painter, &slanted(bar, SLANT * 0.4), alpha(theme.accent, 0.12 + 0.06 * hover));
                fill(&painter, &slanted(Rect::from_min_size(bar.min, vec2(4.0, bar.height())), SLANT * 0.4), theme.accent);
            }
            painter.text(pos2(x + 8.0 * hover, y + 4.0), Align2::LEFT_TOP, article.short_date(), motion::semibold(10.5), alpha(theme.accent, 0.9));
            let title = shorten(&article.title, 46);
            painter.text(pos2(x + 8.0 * hover, y + 16.0), Align2::LEFT_TOP, title, motion::editorial(15.5), mix(theme.ink, Color32::WHITE, hover));
            if response.hovered() {
                ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
            }
            if response.on_hover_text("Lire sur le site").clicked() {
                open_url(&article.url);
            }
            y += 40.0;
        }
    }

    /// The news, in their own column: the latest one large, the others below it.
    fn news_panel(&mut self, ui: &mut egui::Ui, panel: Rect, theme: &Theme) {
        let painter = ui.painter().clone();
        let s = (panel.height() - TOP) * SLANT * 0.12;
        let shape = [pos2(panel.min.x + s, panel.min.y + TOP), pos2(panel.max.x, panel.min.y + TOP), panel.max, pos2(panel.min.x, panel.max.y)];
        fill(&painter, &shape, theme.surface());
        painter.line_segment([shape[0], shape[3]], Stroke::new(3.0, theme.accent));
        let x = panel.min.x + 30.0;
        let width = panel.max.x - x - 26.0;
        let mut y = panel.min.y + TOP + 26.0;
        spaced(&painter, pos2(x, y), "DERNIÈRES NOUVELLES", &motion::semibold(12.0), theme.accent, 2.4);
        y += 22.0;
        let articles: Vec<catalog::Article> = self.catalog.articles.iter().take(4).cloned().collect();
        if articles.is_empty() {
            painter.text(pos2(x, y), Align2::LEFT_TOP, "Les nouvelles du site s'afficheront ici.", motion::body(13.0), theme.muted);
            return;
        }
        let context = ui.ctx().clone();
        for (index, article) in articles.iter().enumerate() {
            let first = index == 0;
            let title_width = if first { width } else { width - 112.0 };
            let title_font = motion::editorial(if first { 20.0 } else { 15.5 });
            let measured = painter.layout(article.title.clone(), title_font.clone(), theme.ink, title_width).size().y
                + if first { painter.layout(shorten(&article.summary, 150), motion::body(13.0), theme.muted, width).size().y + 8.0 } else { 0.0 };
            let height = if first { width * 0.5625 + 34.0 + measured } else { (measured + 24.0).max(70.0) };
            if y + height > panel.max.y - 50.0 {
                break;
            }
            let row = Rect::from_min_size(pos2(x - 8.0, y), vec2(width + 16.0, height));
            let response = ui.interact(row, Id::new(("panel-news", index)), Sense::click());
            let hover = ui.ctx().animate_bool_with_time(response.id, response.hovered(), 0.18);
            let picture = if first { Rect::from_min_size(pos2(x, y), vec2(width, width * 0.5625)) } else { Rect::from_min_size(pos2(x, y + 4.0), vec2(96.0, 60.0)) };
            let lean = SLANT * if first { 0.25 } else { 0.5 };
            let cut = slanted(picture, lean);
            fill(&painter, &cut.map(|p| p + vec2(5.0, 5.0)), alpha(theme.accent2, 0.35 + 0.5 * hover));
            fill(&painter, &cut, theme.shade(0.15));
            if let Some(url) = &article.illustration {
                if let Some((texture, size, _)) = self.images.get(&context, url, false) {
                    let texture = texture.id();
                    motion::picture_in(&painter, texture, self.images.blur(url), size, &cut, motion::upright(picture, lean), if first { 1.45 } else { 1.3 }, 1.0 + 0.05 * hover, Color32::WHITE);
                }
            }
            let (tx, ty, tw) = if first { (x, picture.max.y + 12.0, width) } else { (picture.max.x + 16.0, y + 4.0, width - 112.0) };
            let tx = tx + 4.0 * hover;
            painter.text(pos2(tx, ty), Align2::LEFT_TOP, article.short_date(), motion::semibold(11.0), theme.accent);
            let title = painter.layout(article.title.clone(), motion::editorial(if first { 20.0 } else { 15.5 }), mix(theme.ink, theme.accent, hover), tw);
            let title_height = title.size().y;
            painter.galley(pos2(tx, ty + 16.0), title, theme.ink);
            if first {
                let summary = painter.layout(shorten(&article.summary, 150), motion::body(13.0), theme.muted, tw);
                painter.galley(pos2(tx, ty + 22.0 + title_height), summary, theme.muted);
            }
            if response.hovered() {
                ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
            }
            if response.on_hover_text("Lire sur le site").clicked() {
                open_url(&article.url);
            }
            y += height + if first { 18.0 } else { 10.0 };
        }
        // Everything else is on the site.
        let all = Rect::from_min_size(pos2(x, panel.max.y - 42.0), vec2(width, 26.0));
        let response = ui.interact(all, Id::new("all-news"), Sense::click());
        let hover = ui.ctx().animate_bool_with_time(response.id, response.hovered(), 0.14);
        let colour = mix(theme.muted, theme.accent, hover);
        let used = spaced(&painter, pos2(all.min.x, all.center().y), "TOUTES LES NOUVELLES", &motion::semibold(12.0), colour, 2.0);
        motion::icon(&painter, Icon::Right, pos2(all.min.x + used + 10.0 + 4.0 * hover, all.center().y), 10.0, colour);
        if response.hovered() {
            ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
        }
        if response.clicked() {
            open_url(catalog::SITE);
        }
    }

    fn library(&mut self, ui: &mut egui::Ui, screen: Rect, theme: &Theme) {
        let painter = ui.painter().clone();
        // A dark veil: the grid is for finding, not for the decor.
        painter.rect_filled(screen, 0.0, alpha(if theme.light { Color32::WHITE } else { Color32::BLACK }, 0.35));
        // A blue light rising from the bottom and the left: the shelf is lit, not flat.
        let blue = if theme.light { Color32::from_rgb(0x2c, 0x5b, 0xb5) } else { Color32::from_rgb(0x4b, 0x7f, 0xd6) };
        let (low, side) = if theme.light { (0.13, 0.07) } else { (0.16, 0.08) };
        motion::gradient_rect(&painter, Rect::from_min_max(pos2(screen.min.x, screen.min.y + screen.height() * 0.35), screen.max), alpha(blue, 0.0), alpha(blue, low), true);
        motion::gradient_rect(&painter, Rect::from_min_max(screen.min, pos2(screen.min.x + screen.width() * 0.45, screen.max.y)), alpha(blue, side), alpha(blue, 0.0), false);
        motion::gradient_rect(&painter, Rect::from_min_max(pos2(screen.max.x - screen.width() * 0.3, screen.min.y), screen.max), alpha(theme.accent2, 0.0), alpha(theme.accent2, 0.10), false);
        let left = screen.min.x + 40.0;
        let mut x = left;
        let y = screen.min.y + TOP + 22.0;
        let count_of = |filter: Filter| {
            self.catalog
                .projects
                .iter()
                .filter(|project| match filter {
                    Filter::All => true,
                    Filter::Series(series) => project.series == series,
                    Filter::Installed => self.is_installed(project),
                })
                .count()
        };
        let filters = [
            (Filter::All, "TOUS"),
            (Filter::Series(Series::Trails), "TRAILS"),
            (Filter::Series(Series::Retro), "RÉTRO"),
            (Filter::Series(Series::Other), "CRÉATIONS"),
            (Filter::Installed, "INSTALLÉS"),
        ];
        let counts: Vec<usize> = filters.iter().map(|(filter, _)| count_of(*filter)).collect();
        for ((filter, label), count) in filters.into_iter().zip(counts) {
            let text = format!("{label} ({count})");
            let width = spaced_width(&painter, &text, &motion::semibold(12.0), 1.6) + 30.0;
            let rect = Rect::from_min_size(pos2(x, y), vec2(width, 30.0));
            let response = ui.interact(rect, Id::new(("filter", label)), Sense::click());
            let on = ui.ctx().animate_bool_with_time(response.id, self.filter == filter, 0.2);
            let hover = ui.ctx().animate_bool_with_time(response.id.with("hover"), response.hovered(), 0.12);
            let shape = slanted(rect, SLANT);
            fill(&painter, &shape, alpha(theme.accent, 0.08 + 0.08 * hover));
            if on > 0.0 {
                let grown = Rect::from_min_size(rect.min, vec2(rect.width() * ease_out(on), rect.height()));
                fill(&painter, &slanted(grown, SLANT), theme.accent);
            }
            spaced(&painter, pos2(rect.min.x + 15.0, rect.center().y), &text, &motion::semibold(12.0), mix(theme.ink, theme.bg, on), 1.6);
            if response.hovered() {
                ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
            }
            if response.clicked() && self.filter != filter {
                self.filter = filter;
                self.grid_changed = self.now;
            }
            x += width + 10.0;
        }

        let visible = self.visible();
        let area = Rect::from_min_max(pos2(screen.min.x, y + 48.0), screen.max);
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(area));
        let t = self.since(self.grid_changed, 1.2);
        self.cards.clear();
        egui::ScrollArea::vertical().id_salt("grid").auto_shrink([false, false]).show(&mut child, |ui| {
            let width = ui.available_width() - 80.0;
            let gap = 22.0;
            let columns = ((width + gap) / (250.0 + gap)).floor().max(1.0) as usize;
            let card_width = (width - gap * (columns as f32 - 1.0)) / columns as f32;
            let art_height = card_width * 0.467;
            let card_height = art_height + 58.0;
            let rows = visible.len().div_ceil(columns);
            let (space, _) = ui.allocate_exact_size(vec2(ui.available_width(), rows as f32 * (card_height + gap) + 30.0), Sense::hover());
            if visible.is_empty() {
                ui.painter().text(pos2(space.min.x + 40.0, space.min.y + 30.0), Align2::LEFT_TOP, format!("Aucun projet ne correspond à « {} ».", self.query.trim()), motion::body(16.0), theme.muted);
            }
            for (slot, index) in visible.iter().copied().enumerate() {
                let (row, column) = (slot / columns, slot % columns);
                let appear = ease_out_quint(clamp01((t - slot.min(14) as f32 * 0.035) / 0.5));
                let rect = Rect::from_min_size(
                    pos2(space.min.x + 40.0 + column as f32 * (card_width + gap), space.min.y + row as f32 * (card_height + gap) + (1.0 - appear) * 28.0),
                    vec2(card_width, card_height),
                );
                if !ui.is_rect_visible(rect) {
                    continue;
                }
                self.grid_card(ui, index, rect, art_height, appear, theme);
            }
        });
    }

    fn grid_card(&mut self, ui: &mut egui::Ui, index: usize, rect: Rect, art_height: f32, appear: f32, screen: &Theme) {
        let painter = ui.painter().clone();
        let project = self.catalog.projects[index].clone();
        let theme = self.themes.of(&project.id);
        let response = ui.interact(rect, Id::new(("grid", index)), Sense::click());
        let hover = ui.ctx().animate_bool_with_time(response.id, response.hovered(), 0.16);
        let rect = rect.translate(vec2(0.0, -4.0 * hover));
        let art = Rect::from_min_size(rect.min, vec2(rect.width(), art_height));
        let corner = 26.0;
        let shape = [art.min, pos2(art.max.x - corner, art.min.y), pos2(art.max.x, art.min.y + corner), art.max, pos2(art.min.x, art.max.y)];
        let fade = |colour: Color32| alpha(colour, appear * colour.a() as f32 / 255.0);
        painter.add(egui::Shape::convex_polygon(shape.iter().map(|p| *p + vec2(5.0, 5.0)).collect(), fade(alpha(theme.accent2, 0.35 + 0.5 * hover)), Stroke::NONE));
        painter.add(egui::Shape::convex_polygon(shape.to_vec(), fade(theme.shade(0.2)), Stroke::NONE));
        if let Some(url) = project.thumbnail() {
            let context = ui.ctx().clone();
            if let Some((texture, size, _)) = self.images.get(&context, &url, theme.pixel) {
                let placed = motion::cover(size, art, 1.0 + 0.06 * hover, Vec2::ZERO);
                motion::image_in(&painter, texture.id(), placed, &shape, Color32::from_white_alpha((255.0 * appear) as u8));
                self.cards.insert(index, placed);
            }
        }
        // The cut corner lights up in the project's colour.
        fill(&painter, &[pos2(art.max.x - corner, art.min.y), pos2(art.max.x, art.min.y), pos2(art.max.x, art.min.y + corner)], fade(alpha(theme.accent, 0.25 + 0.75 * hover)));
        let (line, installed) = self.patch_line(&project);
        painter.text(pos2(rect.min.x, art.max.y + 10.0), Align2::LEFT_TOP, &project.title, motion::semibold(16.0), fade(screen.ink));
        let meta = format!("{} · {} · {}", project.series.label(), project.year, project.platform);
        painter.text(pos2(rect.min.x, art.max.y + 34.0), Align2::LEFT_TOP, meta, motion::body(12.5), fade(screen.muted));
        painter.text(pos2(rect.max.x, art.max.y + 35.0), Align2::RIGHT_TOP, line, motion::semibold(10.5), fade(if !installed { screen.muted } else if theme.light == screen.light { theme.accent } else { screen.accent }));
        if response.hovered() {
            ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
        }
        if response.clicked() {
            let placed = self.cards.get(&index).copied().unwrap_or(art);
            self.open(index, ([art.min, pos2(art.max.x, art.min.y), art.max, pos2(art.min.x, art.max.y)], placed));
        }
    }

    /// The picture flying back from a closed page to the home (the page flies it in: page.rs).
    fn flight_back(&mut self, painter: &egui::Painter, context: &egui::Context, screen: Rect) {
        let Some(flight) = &self.flight else { return };
        let (started, closing, index, from, from_placed) = (flight.started, flight.closing, flight.project, flight.from, flight.from_placed);
        let p = self.progress(started, 0.45);
        if !closing || p >= 1.0 {
            self.flight = None;
            return;
        }
        let e = ease_out_quint(p);
        let (to, to_placed) = match self.mode {
            Mode::Discover => match self.art_target(index, screen) {
                Some(target) => target,
                None => {
                    self.flight = None;
                    return;
                }
            },
            Mode::Library => match self.cards.get(&index) {
                Some(placed) => {
                    let r = placed.intersect(screen);
                    ([r.min, pos2(r.max.x, r.min.y), r.max, pos2(r.min.x, r.max.y)], *placed)
                }
                None => {
                    self.flight = None;
                    return;
                }
            },
        };
        let poly: Vec<Pos2> = (0..4).map(|k| motion::lerp_pos(from[k], to[k], e)).collect();
        let placed = motion::lerp_rect(from_placed, to_placed, e);
        let project = &self.catalog.projects[index];
        let pixel = self.themes.of(&project.id).pixel;
        let url = if self.mode == Mode::Library { project.thumbnail() } else { project.key_art().filter(|url| !self.images.failed(url)).or_else(|| project.thumbnail()) };
        let Some(url) = url else { return };
        if let Some((texture, _, _)) = self.images.get(context, &url, pixel) {
            // Opaque even while the home fades in around it.
            let mut painter = painter.clone();
            painter.set_opacity(1.0);
            motion::image_in(&painter, texture.id(), placed, &poly, Color32::WHITE);
        }
    }

    fn top_bar(&mut self, context: &egui::Context, screen: Rect, theme: &Theme) {
        let rect = Rect::from_min_size(screen.min, vec2(screen.width(), TOP));
        egui::Area::new(Id::new("top"))
            .order(Order::Foreground)
            .fixed_pos(rect.min)
            .constrain(false)
            .show(context, |ui| {
                let (rect, _) = ui.allocate_exact_size(rect.size(), Sense::hover());
                let painter = ui.painter().clone();
                motion::gradient_rect(&painter, rect.expand2(vec2(0.0, 6.0)).translate(vec2(0.0, 6.0)), alpha(theme.bg, 0.92), alpha(theme.bg, 0.0), true);

                // The logo: back to the home.
                let logo = self.logo.size_vec2();
                let height = TOP - 14.0;
                let mark = Rect::from_min_size(rect.min + vec2(34.0, 7.0), vec2(logo.x * height / logo.y, height));
                let response = ui.interact(mark, Id::new("mark"), Sense::click());
                let hover = ui.ctx().animate_bool_with_time(response.id, response.hovered(), 0.15);
                let shown = Rect::from_center_size(mark.center(), mark.size() * (1.0 + 0.04 * hover));
                let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
                for (offset, strength) in [(vec2(0.0, 5.0), 0.10), (vec2(0.0, 3.0), 0.16), (vec2(0.0, 1.5), 0.20)] {
                    painter.image(self.logo.id(), shown.translate(offset).expand(1.0), uv, Color32::from_black_alpha((255.0 * strength) as u8));
                }
                painter.image(self.logo.id(), shown, uv, Color32::WHITE);
                if response.hovered() {
                    ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                }
                if response.clicked() {
                    if self.page.is_some() {
                        self.close(Mode::Discover);
                    } else {
                        self.set_mode(Mode::Discover);
                    }
                }

                // Discover / Library, a slanted bar sliding under the one shown.
                let mut x = mark.max.x + 40.0;
                let mut spots = Vec::new();
                for (mode, label, icon) in [(Mode::Discover, "DÉCOUVRIR", Icon::Wheel), (Mode::Library, "BIBLIOTHÈQUE", Icon::Grid)] {
                    let width = spaced_width(&painter, label, &motion::semibold(13.0), 2.0) + 34.0;
                    let spot = Rect::from_min_size(pos2(x, rect.center().y - 16.0), vec2(width, 32.0));
                    let response = ui.interact(spot, Id::new(("mode", label)), Sense::click());
                    let hover = ui.ctx().animate_bool_with_time(response.id, response.hovered(), 0.12);
                    let on = self.mode == mode;
                    let colour = if on { theme.ink } else { mix(theme.muted, theme.ink, hover) };
                    motion::icon(&painter, icon, pos2(spot.min.x + 9.0, spot.center().y), 13.0, if on { theme.accent } else { colour });
                    spaced(&painter, pos2(spot.min.x + 24.0, spot.center().y), label, &motion::semibold(13.0), colour, 2.0);
                    if response.hovered() {
                        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                    }
                    if response.clicked() {
                        if self.page.is_some() {
                            self.close(mode);
                        } else {
                            self.set_mode(mode);
                        }
                    }
                    spots.push(spot);
                    x = spot.max.x + 18.0;
                }
                let target = if self.mode == Mode::Discover { spots[0] } else { spots[1] };
                let bar_x = ui.ctx().animate_value_with_time(Id::new("mode-x"), target.min.x, 0.28);
                let bar_w = ui.ctx().animate_value_with_time(Id::new("mode-w"), target.width(), 0.28);
                fill(&painter, &slanted(Rect::from_min_size(pos2(bar_x + 20.0, target.max.y - 2.0), vec2(bar_w - 22.0, 4.0)), SLANT * 3.0), theme.accent);

                // The community, then the search.
                let mut right = rect.max.x - 34.0;
                // From the right: the help, the site, the Discord, X (X never next to the corner,
                // where it would read as a close button).
                for (icon, url, tip) in [
                    (Icon::Help, catalog::HELP, "Aide (sur le site)"),
                    (Icon::Globe, catalog::SITE, "Le site du Liberl News"),
                    (Icon::Discord, catalog::DISCORD, "Le Discord du Liberl News"),
                    (Icon::X, catalog::X, "Le Liberl News sur X"),
                ] {
                    let spot = Rect::from_center_size(pos2(right - 10.0, rect.center().y), vec2(34.0, 32.0));
                    let response = ui.interact(spot, Id::new(("social", tip)), Sense::click());
                    let hover = ui.ctx().animate_bool_with_time(response.id, response.hovered(), 0.12);
                    let chip = slanted(spot.shrink(1.0), SLANT);
                    fill(&painter, &chip.map(|p| p + vec2(3.0, 3.0)), alpha(theme.accent2, 0.5 + 0.5 * hover));
                    fill(&painter, &chip, mix(theme.bg, theme.accent, hover));
                    motion::icon_on(&painter, icon, spot.center(), if matches!(icon, Icon::Discord) { 18.0 } else { 15.0 }, mix(theme.ink, theme.bg, hover), mix(theme.bg, theme.accent, hover));
                    if response.hovered() {
                        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                    }
                    if response.on_hover_text(tip).clicked() {
                        open_url(url);
                    }
                    right -= 42.0;
                }
                let search = Rect::from_min_size(pos2(right - 250.0, rect.center().y - 16.0), vec2(240.0, 32.0));
                // A newer installer: offered next to the search, installed in one click.
                let update = self.update.lock().unwrap().clone();
                if let Some(update) = update {
                    let failed = self.updating.lock().unwrap().clone().and_then(Result::err);
                    let label = if failed.is_some() {
                        "MISE À JOUR IMPOSSIBLE".to_string()
                    } else if self.update_started {
                        "MISE À JOUR…".to_string()
                    } else {
                        format!("MISE À JOUR {}", update.version)
                    };
                    let width = spaced_width(&painter, &label, &motion::semibold(12.0), 1.6) + 30.0;
                    let chip = Rect::from_min_size(pos2(search.min.x - width - 16.0, rect.center().y - 15.0), vec2(width, 30.0));
                    let response = ui.interact(chip, Id::new("update"), Sense::click());
                    let hover = ui.ctx().animate_bool_with_time(response.id, response.hovered(), 0.12);
                    fill(&painter, &slanted(chip.translate(vec2(4.0, 4.0)), SLANT), theme.accent2);
                    fill(&painter, &slanted(chip, SLANT), mix(theme.accent, Color32::WHITE, 0.15 * hover));
                    spaced(&painter, pos2(chip.min.x + 15.0, chip.center().y), &label, &motion::semibold(12.0), theme.bg, 1.6);
                    if response.hovered() {
                        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                    }
                    let tip = failed.unwrap_or_else(|| format!("Installer la version {} de l'installateur (il redémarre)", update.version));
                    if response.on_hover_text(tip).clicked() && !self.update_started && self.job.is_none() {
                        self.update_started = true;
                        let slot = self.updating.clone();
                        std::thread::spawn(move || *slot.lock().unwrap() = Some(crate::update::apply(&update)));
                    }
                    if self.update_started {
                        ui.ctx().request_repaint_after(std::time::Duration::from_millis(250));
                    }
                }
                fill(&painter, &slanted(search, SLANT * 0.5), alpha(theme.bg, 0.88));
                painter.add(egui::Shape::closed_line(slanted(search, SLANT * 0.5).to_vec(), Stroke::new(1.0, alpha(theme.accent, 0.35))));
                motion::icon(&painter, Icon::Search, pos2(search.min.x + 18.0, search.center().y), 13.0, theme.muted);
                let mut field = ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_min_max(search.min + vec2(32.0, 7.0), search.max - vec2(10.0, 4.0))));
                let edit = field.add(
                    egui::TextEdit::singleline(&mut self.query)
                        .hint_text(egui::RichText::new("Rechercher un projet  (Ctrl+F)").color(alpha(theme.muted, 0.8)))
                        .frame(egui::Frame::NONE)
                        .text_color(theme.ink)
                        .desired_width(search.width() - 44.0),
                );
                if self.focus_search {
                    edit.request_focus();
                    self.focus_search = false;
                }
                if edit.changed() {
                    self.grid_changed = self.now;
                    if !self.query.trim().is_empty() {
                        if self.page.is_some() {
                            self.close(Mode::Library);
                        } else if self.mode != Mode::Library {
                            self.set_mode(Mode::Library);
                        }
                    }
                }
            });
    }

    pub fn set_mode(&mut self, mode: Mode) {
        if self.mode != mode {
            self.mode = mode;
            self.mode_changed = self.now;
            self.grid_changed = self.now;
            self.changed = self.now - THEME_TIME as f64;
        }
    }
}

/// The main slanted button: a hard shadow, a blade of light crossing it on hover, sinks when pressed.
#[allow(clippy::too_many_arguments)]
pub fn slant_button(ui: &mut egui::Ui, painter: &egui::Painter, id: Id, at: Pos2, label: &str, theme: &Theme, enabled: bool, appear: f32, now: f64) -> bool {
    // As large as it can be, but never wider than its column: a long label gets a smaller font.
    let room = ui.available_width().max(160.0) - 12.0;
    let mut size = 21.0;
    while size > 14.0 && spaced_width(painter, label, &motion::display(size), 1.4) + 56.0 > room {
        size -= 1.0;
    }
    let font = motion::display(size);
    let width = spaced_width(painter, label, &font, 1.4) + 56.0;
    let rect = Rect::from_min_size(at, vec2(width, 48.0));
    let response = ui.interact(rect, id, if enabled { Sense::click() } else { Sense::hover() });
    let hover = ui.ctx().animate_bool_with_time(id.with("hover"), enabled && response.hovered(), 0.14);
    let press = ui.ctx().animate_bool_with_time(id.with("press"), enabled && response.is_pointer_button_down_on(), 0.06);
    let sink = vec2(3.0, 3.0) * press;
    let shape = slanted(rect.translate(sink + vec2(3.0 * hover, 0.0)), SLANT);
    let fade = |colour: Color32| alpha(colour, appear * colour.a() as f32 / 255.0);
    fill(painter, &shape.map(|p| p + vec2(7.0, 7.0) - sink), fade(if enabled { theme.accent2 } else { alpha(theme.muted, 0.2) }));
    fill(painter, &shape, fade(if enabled { mix(theme.accent, Color32::WHITE, 0.12 * hover) } else { alpha(theme.muted, 0.35) }));
    if hover > 0.0 {
        // The blade crosses once each time the mouse arrives.
        let since = ui.ctx().memory_mut(|memory| {
            let start = memory.data.get_temp_mut_or_insert_with(id.with("since"), || now);
            if !response.hovered() {
                *start = now;
            }
            *start
        });
        let t = clamp01((now - since) as f32 / 0.5);
        if t < 1.0 {
            ui.ctx().request_repaint();
        }
        let x = lerp(rect.min.x - 40.0, rect.max.x + 40.0, ease_in_out(t));
        let blade = slanted(Rect::from_min_size(pos2(x, rect.min.y), vec2(18.0, rect.height())), SLANT * 2.0);
        fill(&painter.with_clip_rect(rect.expand(6.0)), &blade, alpha(Color32::WHITE, 0.35 * (1.0 - t) * hover));
    }
    spaced(painter, pos2(rect.min.x + 28.0 + 3.0 * hover + sink.x, rect.center().y + sink.y), label, &font, fade(if enabled { theme.bg } else { alpha(theme.ink, 0.5) }), 1.4);
    if response.hovered() && enabled {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }
    response.clicked()
}

/// Letter-spaced text, vertically centred on `at.y`; returns its width.
pub fn spaced(painter: &egui::Painter, at: Pos2, text: &str, font: &egui::FontId, colour: Color32, spacing: f32) -> f32 {
    let mut x = at.x;
    for character in text.chars() {
        let galley = painter.layout_no_wrap(character.to_string(), font.clone(), colour);
        let size = galley.size();
        painter.galley(pos2(x, at.y - size.y / 2.0), galley, colour);
        x += size.x + spacing;
    }
    x - at.x
}

pub fn spaced_width(painter: &egui::Painter, text: &str, font: &egui::FontId, spacing: f32) -> f32 {
    text.chars().map(|character| motion::width(painter, &character.to_string(), font) + spacing).sum()
}

pub fn open_folder(path: &std::path::Path) {
    #[cfg(windows)]
    let _ = std::process::Command::new("explorer").arg(path).spawn();
    #[cfg(not(windows))]
    let _ = std::process::Command::new("xdg-open").arg(path).spawn();
}

pub fn shorten(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let cut: String = text.chars().take(max).collect();
    let cut = cut.rsplit_once(' ').map(|(head, _)| head.to_string()).unwrap_or(cut);
    format!("{}…", cut.trim_end_matches([',', '.', ';', ':']))
}

pub fn open_url(url: &str) {
    #[cfg(windows)]
    let _ = std::process::Command::new("rundll32").args(["url.dll,FileProtocolHandler", url]).spawn();
    #[cfg(not(windows))]
    let _ = std::process::Command::new("xdg-open").arg(url).spawn();
}

fn setup_style(context: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let windows = PathBuf::from(std::env::var_os("WINDIR").unwrap_or_else(|| "C:\\Windows".into())).join("Fonts");
    let fallback = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    // Titles: Anton, in the executable (assets/OFL-Anton.txt), the same for everyone.
    // Its capitals' top where Impact's was (motion::display): 74/1000 of the asked size higher.
    let anton = FontData::from_static(include_bytes!("../assets/Anton-Regular.ttf")).tweak(egui::FontTweak { y_offset_factor: -0.074 / 0.92, ..Default::default() });
    fonts.font_data.insert("display".into(), Arc::new(anton));
    fonts.families.entry(FontFamily::Name("display".into())).or_default().insert(0, "display".into());
    // Segoe UI to read, Segoe UI Semibold for labels, Georgia italic for the news.
    for (name, file, family) in [
        ("segoe", "segoeui.ttf", FontFamily::Proportional),
        ("semibold", "seguisb.ttf", FontFamily::Name("semibold".into())),
        ("editorial", "georgiai.ttf", FontFamily::Name("editorial".into())),
    ] {
        if let Ok(bytes) = std::fs::read(windows.join(file)) {
            fonts.font_data.insert(name.into(), Arc::new(FontData::from_owned(bytes)));
            fonts.families.entry(family).or_default().insert(0, name.into());
        }
    }
    for family in ["semibold", "display", "editorial"] {
        let entry = fonts.families.entry(FontFamily::Name(family.into())).or_default();
        if !entry.contains(&"segoe".to_string()) && fonts.font_data.contains_key("segoe") {
            entry.push("segoe".into());
        }
        entry.extend(fallback.iter().cloned());
    }
    context.set_fonts(fonts);
    for mode in [egui::Theme::Light, egui::Theme::Dark] {
        context.style_mut_of(mode, |style| {
            style.text_styles.insert(egui::TextStyle::Body, egui::FontId::proportional(15.0));
            style.text_styles.insert(egui::TextStyle::Button, egui::FontId::proportional(15.0));
            style.animation_time = 0.18;
            style.spacing.scroll.floating = true;
        });
    }
}

/// The widgets follow the project's theme: light or dark, its ink, its accent.
fn apply_theme(context: &egui::Context, theme: &Theme) {
    let mode = if theme.light { egui::Theme::Light } else { egui::Theme::Dark };
    if context.theme() != mode {
        context.set_theme(mode);
    }
    let current = context.style_of(mode).visuals.override_text_color;
    if current == Some(theme.ink) {
        return;
    }
    context.style_mut_of(mode, |style| {
        let visuals = &mut style.visuals;
        visuals.override_text_color = Some(theme.ink);
        visuals.panel_fill = theme.bg;
        visuals.extreme_bg_color = alpha(theme.ink, 0.06);
        visuals.selection.bg_fill = alpha(theme.accent, 0.45);
        visuals.selection.stroke = Stroke::new(1.0, theme.ink);
        visuals.hyperlink_color = theme.accent;
        visuals.widgets.inactive.bg_fill = alpha(theme.ink, 0.15);
        visuals.widgets.hovered.bg_fill = alpha(theme.ink, 0.3);
        visuals.widgets.active.bg_fill = alpha(theme.accent, 0.6);
        visuals.widgets.inactive.weak_bg_fill = alpha(theme.ink, 0.07);
        visuals.widgets.hovered.weak_bg_fill = alpha(theme.ink, 0.12);
        visuals.window_fill = theme.surface();
        visuals.text_cursor.stroke = Stroke::new(2.0, theme.ink);
    });
}
