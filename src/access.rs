//! The platform account, signed in from the installer (the account button, top right) to get the
//! private test versions (plateforme/api/installer-session.php). The password is never kept: the
//! session's download-only token is remembered in the installer's folder (compte.json) until
//! signing out or its expiry (30 days).
#![allow(deprecated)]

use crate::app::{App, Slot, slant_button, spaced};
use crate::motion::{self, SLANT, alpha, clamp01, ease_out_quint, fill, mix, slanted};
use eframe::egui::{self, Color32, CursorIcon, Id, Order, Rect, Sense, vec2};
use serde::Deserialize;


#[derive(Clone, Deserialize, serde::Serialize)]
pub struct Session {
    pub token: String,
    pub username: String,
}

#[derive(Default)]
pub struct Access {
    /// When the sign-in dialog opened (None: closed).
    pub opened: Option<f64>,
    pub open: bool,
    pub login: String,
    pub password: String,
    pub session: Option<Session>,
    pub pending: Slot<Result<Session, String>>,
    pub busy: bool,
    pub error: Option<String>,
    /// The first field takes the keyboard when the dialog opens.
    pub focus: bool,
}

fn remembered_file() -> std::path::PathBuf {
    crate::patch::data_dir().join("compte.json")
}

impl Access {
    /// The account signed in last time, if it was remembered.
    pub fn remembered() -> Access {
        let session = std::fs::read_to_string(remembered_file()).ok().and_then(|text| serde_json::from_str::<Session>(&text).ok());
        Access { login: session.as_ref().map(|session| session.username.clone()).unwrap_or_default(), session, ..Access::default() }
    }
}

fn remember(session: Option<&Session>) {
    let file = remembered_file();
    match session {
        Some(session) => {
            let _ = std::fs::create_dir_all(crate::patch::data_dir());
            let _ = std::fs::write(&file, serde_json::to_string(session).unwrap_or_default());
        }
        None => {
            let _ = std::fs::remove_file(&file);
        }
    }
}

fn session_url() -> String {
    format!("{}/plateforme/api/installer-session.php", crate::patch::platform())
}

fn login(account: &str, password: &str) -> Result<Session, String> {
    let body = serde_json::json!({ "login": account, "password": password });
    let response = ureq::post(&session_url())
        .header("Content-Type", "application/json")
        .header("User-Agent", "InstallateurLiberlNews")
        .config()
        .http_status_as_error(false)
        .build()
        .send(body.to_string())
        .map_err(|_| "Connexion momentanément impossible.".to_string())?;
    let ok = response.status().as_u16() == 200;
    let text = response.into_body().with_config().limit(16384).read_to_string().map_err(|_| "Réponse de connexion illisible.".to_string())?;
    if ok {
        serde_json::from_str(&text).map_err(|_| "Réponse de connexion illisible.".into())
    } else {
        Err(serde_json::from_str::<serde_json::Value>(&text).ok().and_then(|value| value["error"].as_str().map(str::to_string)).unwrap_or_else(|| "Connexion refusée.".into()))
    }
}

impl App {
    pub fn open_login(&mut self) {
        self.access.open = true;
        self.access.focus = true;
        self.access.opened = Some(self.now);
        self.access.error = None;
    }

    /// Signs out: the session ends on the platform and is forgotten here; the private versions are
    /// no longer offered.
    pub fn sign_out(&mut self, context: &egui::Context) {
        if let Some(session) = self.access.session.take() {
            std::thread::spawn(move || {
                let _ = ureq::delete(&session_url()).header("User-Agent", "InstallateurLiberlNews").header("Authorization", &format!("Bearer {}", session.token)).call();
            });
        }
        remember(None);
        self.channels.retain(|_, channel| channel != "private");
        self.load_offers(context);
    }

    /// The sign-in dialog (drawn like the others), and the answer of a sign-in under way.
    pub fn login_dialog(&mut self, context: &egui::Context) {
        let answer = self.access.pending.lock().unwrap().take();
        if let Some(answer) = answer {
            self.access.busy = false;
            match answer {
                Ok(session) => {
                    remember(Some(&session));
                    self.access.session = Some(session);
                    self.access.open = false;
                    self.access.error = None;
                    self.load_offers(context);
                }
                Err(error) => self.access.error = Some(error),
            }
        }
        if !self.access.open {
            return;
        }
        let opened = self.access.opened.unwrap_or(self.now);
        let p = ease_out_quint(self.progress(opened, 0.3));
        let theme = self.current_theme();
        let screen = self.screen;
        let (mut submit, mut close, mut leave) = (false, false, false);
        let signed_in = self.access.session.as_ref().map(|session| session.username.clone());
        egui::Area::new(Id::new("login")).order(Order::Tooltip).fixed_pos(screen.min).constrain(false).show(context, |ui| {
            let (rect, backdrop) = ui.allocate_exact_size(screen.size(), Sense::click());
            let painter = ui.painter().clone();
            painter.rect_filled(rect, 0.0, alpha(Color32::BLACK, 0.7 * p));
            let panel = Rect::from_center_size(rect.center(), vec2(520.0, 330.0));
            let shown = Rect::from_min_size(panel.min, vec2(panel.width() * p, panel.height()));
            fill(&painter, &slanted(shown.translate(vec2(8.0, 8.0)), SLANT * 0.3), theme.accent2);
            fill(&painter, &slanted(shown, SLANT * 0.3), theme.surface());
            let text = clamp01((p - 0.5) * 2.0);
            let clip = painter.with_clip_rect(shown);
            spaced(&clip, panel.min + vec2(36.0, 44.0), "COMPTE DE LA PLATEFORME", &motion::display(26.0), alpha(theme.ink, text), 1.4);
            if let Some(name) = &signed_in {
                // Signed in: who, and the way out.
                let galley = clip.layout(
                    format!("Connecté en tant que {name}. Les versions de test privées sont proposées sur les fiches des jeux qui en ont."),
                    motion::body(14.0),
                    alpha(theme.muted, text),
                    panel.width() - 72.0,
                );
                clip.galley(panel.min + vec2(36.0, 66.0), galley, theme.muted);
                let at = panel.min + vec2(36.0, 250.0);
                if slant_button(ui, &painter, Id::new("logout"), at, "SE DÉCONNECTER", &theme, self.job.is_none(), text, self.now) {
                    leave = true;
                }
                let back = Rect::from_min_size(at + vec2(250.0, 6.0), vec2(120.0, 36.0));
                let response = ui.interact(back, Id::new("login-no"), Sense::click());
                let hover = ui.ctx().animate_bool_with_time(response.id, response.hovered(), 0.12);
                spaced(&painter, egui::pos2(back.min.x + 10.0, back.center().y), "FERMER", &motion::semibold(14.0), alpha(mix(theme.muted, theme.ink, hover), text), 2.0);
                if response.hovered() {
                    ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                }
                close |= response.clicked() || ui.input(|input| input.key_pressed(egui::Key::Escape));
                if backdrop.clicked() && !panel.contains(ui.ctx().pointer_interact_pos().unwrap_or_default()) {
                    close = true;
                }
                return;
            }
            let galley = clip.layout(
                "Pour les versions de test privées, réservées à l'équipe et aux testeurs. Le mot de passe n'est pas enregistré : la connexion est retenue 30 jours.".to_string(),
                motion::body(14.0),
                alpha(theme.muted, text),
                panel.width() - 72.0,
            );
            clip.galley(panel.min + vec2(36.0, 66.0), galley, theme.muted);
            let field = |ui: &mut egui::Ui, top: f32, value: &mut String, hint: &str, password: bool| {
                let area = Rect::from_min_size(panel.min + vec2(36.0, top), vec2(panel.width() - 72.0, 34.0));
                ui.put(area, egui::TextEdit::singleline(value).hint_text(hint).password(password).font(motion::body(15.0)).margin(vec2(10.0, 7.0)))
            };
            let account = field(ui, 116.0, &mut self.access.login, "Pseudo ou e-mail", false);
            let password = field(ui, 160.0, &mut self.access.password, "Mot de passe", true);
            if std::mem::take(&mut self.access.focus) {
                self.player.release_focus();
                if self.access.login.is_empty() { account.request_focus() } else { password.request_focus() }
            }
            if password.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)) {
                submit = true;
            }
            if let Some(error) = &self.access.error {
                clip.text(panel.min + vec2(36.0, 210.0), egui::Align2::LEFT_TOP, error, motion::body(14.0), alpha(Color32::from_rgb(0xd8, 0x4a, 0x3a), text));
            }
            let at = panel.min + vec2(36.0, 250.0);
            let ready = !self.access.busy && !self.access.login.trim().is_empty() && !self.access.password.is_empty();
            if slant_button(ui, &painter, Id::new("login-yes"), at, if self.access.busy { "CONNEXION…" } else { "SE CONNECTER" }, &theme, ready, text, self.now) {
                submit = true;
            }
            let cancel = Rect::from_min_size(at + vec2(250.0, 6.0), vec2(120.0, 36.0));
            let response = ui.interact(cancel, Id::new("login-no"), Sense::click());
            let hover = ui.ctx().animate_bool_with_time(response.id, response.hovered(), 0.12);
            spaced(&painter, egui::pos2(cancel.min.x + 10.0, cancel.center().y), "ANNULER", &motion::semibold(14.0), alpha(mix(theme.muted, theme.ink, hover), text), 2.0);
            if response.hovered() {
                ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
            }
            close |= response.clicked() || ui.input(|input| input.key_pressed(egui::Key::Escape));
            if backdrop.clicked() && !panel.contains(ui.ctx().pointer_interact_pos().unwrap_or_default()) {
                close = true;
            }
        });
        if leave {
            self.access.open = false;
            self.sign_out(context);
            return;
        }
        if close && !self.access.busy {
            self.access.open = false;
            self.access.password.clear();
        } else if submit && !self.access.busy && !self.access.login.trim().is_empty() && !self.access.password.is_empty() {
            self.access.busy = true;
            self.access.error = None;
            let account = self.access.login.trim().to_string();
            let password = std::mem::take(&mut self.access.password);
            let (slot, context) = (self.access.pending.clone(), context.clone());
            std::thread::spawn(move || {
                *slot.lock().unwrap() = Some(login(&account, &password));
                context.request_repaint();
            });
        }
    }
}
