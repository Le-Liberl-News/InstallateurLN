//! The trailer, played in the page: YouTube's player in a WebView2 view (the
//! Edge engine Windows already has; nothing is shipped) laid over the
//! gallery's big view. It starts by itself when the page opens, follows the
//! page as it scrolls (cut where the hero covers it), pauses and hides when it
//! is out of sight or a dialog covers it, and goes away with the page. Without WebView2, the poster opens YouTube.

use eframe::egui::Rect;

/// The site's page that holds YouTube's player (Website trailer.php).
#[cfg(windows)]
const TRAILER_PAGE: &str = "https://leliberlnews.fr/trailer.php?v=";

/// What the page asks for this frame: which video, where, and whether it may be seen.
#[derive(Clone, PartialEq)]
pub struct Request {
    pub id: String,
    pub rect: Rect,
    /// The part of `rect` that may be seen (the hero and the window's edge cut the rest).
    pub clip: Rect,
    pub visible: bool,
    /// Out of sight or covered by a dialog: the video pauses too.
    pub pause: bool,
}

#[cfg(windows)]
struct Trailer {
    view: wry::WebView,
    _context: wry::WebContext,
    id: String,
    /// The view's own window (wry's child window), moved and cut without resizing the browser.
    hwnd: *mut std::ffi::c_void,
    rect: Rect,
    clip: Rect,
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
    /// The wheel turned over the video (the trailer page hands it over): the page scrolls with it.
    pub wheel: std::sync::Arc<std::sync::Mutex<f32>>,
}

#[cfg(windows)]
mod win {
    use std::ffi::c_void;
    pub const SWP_NOSIZE: u32 = 0x0001;
    pub const SWP_NOZORDER: u32 = 0x0004;
    pub const SWP_NOACTIVATE: u32 = 0x0010;
    pub const SWP_ASYNCWINDOWPOS: u32 = 0x4000;
    #[link(name = "user32")]
    unsafe extern "system" {
        pub fn SetWindowPos(hwnd: *mut c_void, after: *mut c_void, x: i32, y: i32, cx: i32, cy: i32, flags: u32) -> i32;
        pub fn SetWindowRgn(hwnd: *mut c_void, region: *mut c_void, redraw: i32) -> i32;
    }
    #[link(name = "gdi32")]
    unsafe extern "system" {
        pub fn CreateRectRgn(left: i32, top: i32, right: i32, bottom: i32) -> *mut c_void;
    }
}

#[cfg(windows)]
fn bounds(rect: Rect) -> wry::Rect {
    wry::Rect {
        position: wry::dpi::LogicalPosition::new(rect.min.x as f64, rect.min.y as f64).into(),
        size: wry::dpi::LogicalSize::new(rect.width().max(1.0) as f64, rect.height().max(1.0) as f64).into(),
    }
}

impl Player {
    /// What the wheel did over the video since the last frame (points, egui's direction).
    pub fn take_wheel(&self) -> f32 {
        std::mem::take(&mut *self.wheel.lock().unwrap())
    }

    #[cfg(windows)]
    pub fn update(&mut self, frame: &eframe::Frame, egui_context: &eframe::egui::Context, request: Option<Request>) {
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
                .with_ipc_handler({
                    let (wheel, repaint) = (self.wheel.clone(), egui_context.clone());
                    move |message: wry::http::Request<String>| {
                        if let Ok(delta) = message.body().trim().parse::<f32>() {
                            *wheel.lock().unwrap() -= delta;
                            repaint.request_repaint();
                        }
                    }
                })
                // The defaults wry passes, plus sound without a click.
                .with_additional_browser_args("--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection --autoplay-policy=no-user-gesture-required")
                .build_as_child(frame);
            match built {
                Ok(view) => {
                    use wry::WebViewExtWindows;
                    let mut parent = Default::default();
                    let hwnd = match unsafe { view.controller().ParentWindow(&mut parent) } {
                        Ok(()) => parent.0 as *mut std::ffi::c_void,
                        Err(_) => std::ptr::null_mut(),
                    };
                    self.current = Some(Trailer { view, _context: context, id: request.id.clone(), hwnd, rect: request.rect, clip: request.rect, visible: true, paused: false });
                    self.cut(egui_context.pixels_per_point(), request.clip);
                }
                Err(_) => self.failed = true,
            }
            return;
        }
        let ppp = egui_context.pixels_per_point();
        let Some(current) = &mut self.current else { return };
        // Moving only moves its window (cheap); a new size resizes the browser too. Never hidden
        // while it scrolls: a hidden view is a background page, where Edge stops the picture.
        if request.visible && current.rect != request.rect {
            if current.rect.size() != request.rect.size() || current.hwnd.is_null() {
                let _ = current.view.set_bounds(bounds(request.rect));
            } else {
                let (x, y) = ((request.rect.min.x * ppp).round() as i32, (request.rect.min.y * ppp).round() as i32);
                unsafe { win::SetWindowPos(current.hwnd, std::ptr::null_mut(), x, y, 0, 0, win::SWP_NOSIZE | win::SWP_NOZORDER | win::SWP_NOACTIVATE | win::SWP_ASYNCWINDOWPOS) };
            }
            current.rect = request.rect;
        }
        if request.visible {
            self.cut(ppp, request.clip);
        }
        let Some(current) = &mut self.current else { return };
        if current.visible != request.visible {
            let _ = current.view.set_visible(request.visible);
            current.visible = request.visible;
        }
        if request.pause && !current.paused {
            let _ = current.view.evaluate_script("lnPause()");
            current.paused = true;
        } else if !request.pause && current.paused && request.visible {
            let _ = current.view.evaluate_script("lnPlay()");
            current.paused = false;
        }
    }

    /// Only `clip` of the view shows (a window region, in the view's own pixels).
    #[cfg(windows)]
    fn cut(&mut self, ppp: f32, clip: Rect) {
        let Some(current) = &mut self.current else { return };
        if current.hwnd.is_null() || current.clip == clip {
            return;
        }
        current.clip = clip;
        let whole = clip.min.y <= current.rect.min.y + 0.5 && clip.max.y >= current.rect.max.y - 0.5 && clip.min.x <= current.rect.min.x + 0.5 && clip.max.x >= current.rect.max.x - 0.5;
        unsafe {
            let region = if whole {
                std::ptr::null_mut()
            } else {
                let at = |value: f32, from: f32| ((value - from) * ppp).round() as i32;
                win::CreateRectRgn(at(clip.min.x, current.rect.min.x), at(clip.min.y, current.rect.min.y), at(clip.max.x, current.rect.min.x), at(clip.max.y, current.rect.min.y))
            };
            win::SetWindowRgn(current.hwnd, region, 1);
        }
    }

    #[cfg(not(windows))]
    pub fn update(&mut self, _frame: &eframe::Frame, _context: &eframe::egui::Context, _request: Option<Request>) {
        self.failed = true;
    }
}
