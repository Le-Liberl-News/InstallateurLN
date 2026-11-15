//! Pictures from the web, loaded only when something shows them: fetched and
//! decoded in the background (four at a time, at most 1920 px), turned into
//! textures on the window's thread.

use eframe::egui::{self, ColorImage, TextureFilter, TextureHandle, TextureId, TextureOptions, Vec2};
use std::collections::HashMap;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Condvar, Mutex};

pub enum Entry {
    Loading,
    /// `blur`: a small blurred copy, the backdrop of frames the picture does not fill.
    Ready { texture: TextureHandle, blur: Option<TextureHandle>, size: Vec2, since: f64 },
    Failed,
}

pub struct Images {
    entries: HashMap<String, Entry>,
    sender: Sender<(String, Option<(ColorImage, ColorImage)>, bool)>,
    receiver: Receiver<(String, Option<(ColorImage, ColorImage)>, bool)>,
    slots: Arc<(Mutex<usize>, Condvar)>,
}

const PARALLEL: usize = 4;
const LARGEST: u32 = 1920;

impl Images {
    pub fn new() -> Images {
        let (sender, receiver) = channel();
        Images { entries: HashMap::new(), sender, receiver, slots: Arc::new((Mutex::new(0), Condvar::new())) }
    }

    /// Textures of the pictures that arrived since the last frame.
    pub fn poll(&mut self, context: &egui::Context, now: f64) {
        while let Ok((url, image, pixel)) = self.receiver.try_recv() {
            let entry = match image {
                Some((image, blurred)) => {
                    let size = Vec2::new(image.size[0] as f32, image.size[1] as f32);
                    // Mipmaps: a big picture shown small stays smooth instead of shimmering.
                    let options = if pixel { TextureOptions::NEAREST } else { TextureOptions { mipmap_mode: Some(TextureFilter::Linear), ..TextureOptions::LINEAR } };
                    let blur = Some(context.load_texture(format!("{url}#flou"), blurred, TextureOptions::LINEAR));
                    Entry::Ready { texture: context.load_texture(&url, image, options), blur, size, since: now }
                }
                None => Entry::Failed,
            };
            self.entries.insert(url, entry);
        }
    }

    /// The picture if it is here; asks for it otherwise. `pixel`: pixel art, enlarged without smoothing.
    pub fn get(&mut self, context: &egui::Context, url: &str, pixel: bool) -> Option<(&TextureHandle, Vec2, f64)> {
        if !self.entries.contains_key(url) {
            self.entries.insert(url.to_string(), Entry::Loading);
            let (url, sender, slots, context) = (url.to_string(), self.sender.clone(), self.slots.clone(), context.clone());
            std::thread::spawn(move || {
                {
                    let (count, free) = &*slots;
                    let mut count = free.wait_while(count.lock().unwrap(), |count| *count >= PARALLEL).unwrap();
                    *count += 1;
                }
                let image = download(&url);
                {
                    let (count, free) = &*slots;
                    *count.lock().unwrap() -= 1;
                    free.notify_one();
                }
                let _ = sender.send((url, image, pixel));
                context.request_repaint();
            });
        }
        match self.entries.get(url) {
            Some(Entry::Ready { texture, size, since, .. }) => Some((texture, *size, *since)),
            _ => None,
        }
    }

    /// The blurred copy of a picture already here.
    pub fn blur(&self, url: &str) -> Option<TextureId> {
        match self.entries.get(url) {
            Some(Entry::Ready { blur: Some(blur), .. }) => Some(blur.id()),
            _ => None,
        }
    }

    /// The size of a picture already here.
    pub fn size(&self, url: &str) -> Option<Vec2> {
        match self.entries.get(url) {
            Some(Entry::Ready { size, .. }) => Some(*size),
            _ => None,
        }
    }

    pub fn failed(&self, url: &str) -> bool {
        matches!(self.entries.get(url), Some(Entry::Failed))
    }
}

fn download(url: &str) -> Option<(ColorImage, ColorImage)> {
    let response = ureq::get(url).header("User-Agent", "InstallateurLiberlNews").call().ok()?;
    let bytes = response.into_body().with_config().limit(16 << 20).read_to_vec().ok()?;
    let mut image = image::load_from_memory(&bytes).ok()?;
    if image.width() > LARGEST || image.height() > LARGEST {
        image = image.resize(LARGEST, LARGEST, image::imageops::FilterType::Triangle);
    }
    // The backdrop: tiny, blurred, then smoothed again when the screen enlarges it.
    // Its edges repeated before blurring and cut after: blurred as they were, they darkened
    // (black around), a dark rim right on the frames' edges.
    let small = image.resize(48, 48, image::imageops::FilterType::Triangle).to_rgba8();
    let (w, h) = (small.width(), small.height());
    let pad = 8u32;
    let padded = image::RgbaImage::from_fn(w + pad * 2, h + pad * 2, |x, y| *small.get_pixel(x.saturating_sub(pad).min(w - 1), y.saturating_sub(pad).min(h - 1)));
    let small = image::imageops::crop_imm(&image::imageops::blur(&padded, 2.5), pad, pad, w, h).to_image();
    let image = image.to_rgba8();
    Some((
        ColorImage::from_rgba_unmultiplied([image.width() as usize, image.height() as usize], image.as_raw()),
        ColorImage::from_rgba_unmultiplied([small.width() as usize, small.height() as usize], small.as_raw()),
    ))
}
