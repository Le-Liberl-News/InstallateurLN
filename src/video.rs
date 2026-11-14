//! The trailer, played in the page: YouTube's player in a WebView2 view (the
//! Edge engine Windows already has; nothing is shipped) laid over the
//! gallery's big view. It starts by itself when the page opens, pauses and
//! hides when something covers it (a dialog, the hero while scrolling), and
//! goes away with the page. Without WebView2, the poster opens YouTube.

use eframe::egui::Rect;

/// The site's page that holds YouTube's player (Website trailer.php).
#[cfg(windows)]
const TRAILER_PAGE: &str = "https://leliberlnews.fr/trailer.php?v=";

/// What the page asks for this frame: which video, where, and whether it may be seen.
#[derive(Clone, PartialEq)]
pub struct Request {
    pub id: String,
    pub rect: Rect,
    pub visible: bool,
}

#[cfg(windows)]
struct Trailer {
    view: wry::WebView,
    _context: wry::WebContext,
    id: String,
    rect: Rect,
    visible: bool,
    /// Paused by us (hidden), to be resumed when seen again.
    paused: bool,
}

#[derive(Default)]
pub struct Player {
    #[cfg(windows)]
    current: Option<Trailer>,
    /// WebView2 is missing or failed: the page falls back to the poster.
    pub failed: bool,
}

#[cfg(windows)]
fn bounds(rect: Rect) -> wry::Rect {
    wry::Rect {
        position: wry::dpi::LogicalPosition::new(rect.min.x as f64, rect.min.y as f64).into(),
        size: wry::dpi::LogicalSize::new(rect.width().max(1.0) as f64, rect.height().max(1.0) as f64).into(),
    }
}

impl Player {
    #[cfg(windows)]
    pub fn update(&mut self, frame: &eframe::Frame, request: Option<Request>) {
        use wry::WebViewBuilderExtWindows;
        let Some(request) = request else {
            self.current = None;
            return;
        };
        if self.current.as_ref().is_some_and(|current| current.id != request.id) {
            self.current = None;
        }
        if self.current.is_none() {
            if self.failed || !request.visible {
                return;
            }
            let folder = std::env::var_os("LOCALAPPDATA").map(std::path::PathBuf::from).map(|path| path.join("LiberlNews").join("webview"));
            let mut context = wry::WebContext::new(folder);
            // The site's page holding the player: YouTube plays embeds from a real site only.
            // LN_TRAILER_URL: a local site, to try it before it is online.
            let base = std::env::var("LN_TRAILER_URL").unwrap_or_else(|_| TRAILER_PAGE.to_string());
            let built = wry::WebViewBuilder::new_with_web_context(&mut context)
                .with_url(format!("{base}{}", request.id))
                .with_bounds(bounds(request.rect))
                .with_background_color((0, 0, 0, 255))
                .with_autoplay(true)
                // The defaults wry passes, plus sound without a click.
                .with_additional_browser_args("--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection --autoplay-policy=no-user-gesture-required")
                .build_as_child(frame);
            match built {
                Ok(view) => self.current = Some(Trailer { view, _context: context, id: request.id.clone(), rect: request.rect, visible: true, paused: false }),
                Err(_) => self.failed = true,
            }
            return;
        }
        let Some(current) = &mut self.current else { return };
        if current.rect != request.rect {
            let _ = current.view.set_bounds(bounds(request.rect));
            current.rect = request.rect;
        }
        if current.visible != request.visible {
            let _ = current.view.set_visible(request.visible);
            if !request.visible {
                let _ = current.view.evaluate_script("lnPause()");
                current.paused = true;
            } else if current.paused {
                let _ = current.view.evaluate_script("lnPlay()");
                current.paused = false;
            }
            current.visible = request.visible;
        }
    }

    #[cfg(not(windows))]
    pub fn update(&mut self, _frame: &eframe::Frame, _request: Option<Request>) {
        self.failed = true;
    }
}
