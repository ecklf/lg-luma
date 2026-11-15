//! Built-in wallpapers that are not shipped in the IPK are fetched from the
//! upstream GitHub tree on first use, then cached next to preferences.
//!
//! Full packs are kept only for the active background (or every gallery pack
//! while wallpaper cycling is on). Inactive downloads are reduced to a small
//! thumbnail so the gallery can still offer a re-download from GitHub.
use crate::model::BACKGROUNDS;
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::mpsc,
    thread,
    time::Duration,
};

/// Always packaged with the app. Vector wallpapers stay local (tiny after
/// bake); photo packs download from GitHub on first use.
pub const BUNDLED: &[&str] = &["slate", "midnight", "prism"];
/// Default when preferences name a missing or retired wallpaper.
pub const DEFAULT_ID: &str = "slate";

const SOURCE_BASE: &str = "https://raw.githubusercontent.com/ecklf/lg-luma/main/assets/wallpapers";
const MAX_BYTES: u64 = 8 * 1024 * 1024;
const TIMEOUT: Duration = Duration::from_secs(45);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status {
    Ready,
    Missing,
    Downloading,
    Failed(String),
}

pub enum Event {
    Progress(String, Status),
    Installed(String),
}

enum Command {
    Ensure(String),
    Prune(Vec<String>),
}

pub struct Service {
    wake: mpsc::Sender<Command>,
    pub receiver: mpsc::Receiver<Event>,
}

impl Service {
    pub fn new(cache: PathBuf) -> Result<Self, String> {
        let (events_tx, events_rx) = mpsc::sync_channel(8);
        let (wake, commands) = mpsc::channel();
        thread::Builder::new()
            .name("luma-wallpaper-packs".into())
            .spawn(move || worker(cache, commands, events_tx))
            .map_err(|error| format!("Could not start wallpaper pack worker: {error}"))?;
        Ok(Self {
            wake,
            receiver: events_rx,
        })
    }

    pub fn ensure(&self, id: &str) {
        if bundled(id) {
            return;
        }
        let _ = self.wake.send(Command::Ensure(id.into()));
    }

    /// Drop full assets for packs not in `keep`, leaving `{id}.thumb.jpg`.
    pub fn prune(&self, keep: Vec<String>) {
        let _ = self.wake.send(Command::Prune(keep));
    }
}

pub fn bundled(id: &str) -> bool {
    BUNDLED.contains(&id)
}

pub fn remote_builtin(id: &str) -> bool {
    BACKGROUNDS.iter().any(|(known, _)| *known == id) && !bundled(id)
}

pub fn source_name(id: &str) -> Option<&'static str> {
    match id {
        "slate" | "midnight" | "prism" => Some("svg"),
        "dunes" | "san-francisco" | "summit" | "forest" | "yosemite" | "northern-lights" => {
            Some("jpg")
        }
        _ => None,
    }
}

pub fn pack_dir(cache: &Path, id: &str) -> PathBuf {
    cache.join(id)
}

pub fn wallpaper_file(cache: &Path, assets: &Path, id: &str, plain: bool) -> PathBuf {
    let name = if plain {
        format!("{id}-plain.jpg")
    } else {
        format!("{id}.jpg")
    };
    let cached = pack_dir(cache, id).join(&name);
    if cached.is_file() {
        return cached;
    }
    assets.join("wallpapers").join(name)
}

pub fn installed(cache: &Path, assets: &Path, id: &str) -> bool {
    if bundled(id) {
        return assets
            .join("wallpapers")
            .join(format!("{id}.jpg"))
            .is_file();
    }
    let dir = pack_dir(cache, id);
    dir.join(format!("{id}.jpg")).is_file()
        && dir.join(format!("{id}-plain.jpg")).is_file()
        && dir.join(format!("{id}.glass.png")).is_file()
        && dir.join(format!("{id}-plain.glass.png")).is_file()
}

pub fn thumbnail_file(cache: &Path, id: &str) -> PathBuf {
    pack_dir(cache, id).join(format!("{id}.thumb.jpg"))
}

/// Remove full-resolution assets for every remote pack not listed in `keep`,
/// preserving a gallery thumbnail for a later GitHub re-download.
pub fn prune_cache(cache: &Path, keep: &[String]) {
    let Ok(entries) = fs::read_dir(cache) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(id) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if id.starts_with('.') || !remote_builtin(id) {
            continue;
        }
        if keep.iter().any(|kept| kept == id) {
            continue;
        }
        let _ = prune_to_thumbnail(cache, id);
    }
}

fn prune_to_thumbnail(cache: &Path, id: &str) -> Result<(), String> {
    let dir = pack_dir(cache, id);
    if !dir.is_dir() {
        return Ok(());
    }
    let thumb_name = format!("{id}.thumb.jpg");
    let thumb = dir.join(&thumb_name);
    if !thumb.is_file() {
        let source = [
            dir.join(format!("{id}-plain.jpg")),
            dir.join(format!("{id}.jpg")),
        ]
        .into_iter()
        .find(|path| path.is_file());
        if let Some(path) = source {
            let bytes = fs::read(&path).map_err(|error| error.to_string())?;
            let image = decode_cover(&bytes, id)?;
            let thumbnail =
                image::imageops::resize(&image, 480, 270, image::imageops::FilterType::Triangle);
            write(&thumb, &jpeg(&thumbnail, 82)?)?;
        }
    }
    if !thumb.is_file() {
        let _ = fs::remove_dir_all(&dir);
        return Ok(());
    }
    let aside = cache.join(format!(".{id}.thumb.jpg"));
    let _ = fs::remove_file(&aside);
    fs::rename(&thumb, &aside).map_err(|error| error.to_string())?;
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    fs::rename(&aside, dir.join(thumb_name)).map_err(|error| error.to_string())?;
    Ok(())
}

fn worker(cache: PathBuf, commands: mpsc::Receiver<Command>, events: mpsc::SyncSender<Event>) {
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(TIMEOUT))
        .max_redirects(2)
        .https_only(true)
        .proxy(None)
        .build()
        .new_agent();
    while let Ok(command) = commands.recv() {
        match command {
            Command::Prune(keep) => prune_cache(&cache, &keep),
            Command::Ensure(id) => {
                if !remote_builtin(&id) {
                    continue;
                }
                if pack_dir(&cache, &id).join(format!("{id}.jpg")).is_file()
                    && pack_dir(&cache, &id)
                        .join(format!("{id}-plain.jpg"))
                        .is_file()
                {
                    let _ = events.send(Event::Installed(id));
                    continue;
                }
                let _ = events.send(Event::Progress(id.clone(), Status::Downloading));
                match fetch_and_install(&agent, &cache, &id) {
                    Ok(()) => {
                        let _ = events.send(Event::Installed(id));
                    }
                    Err(error) => {
                        let _ = events.send(Event::Progress(id, Status::Failed(error)));
                    }
                }
            }
        }
    }
}

fn fetch_and_install(agent: &ureq::Agent, cache: &Path, id: &str) -> Result<(), String> {
    let kind = source_name(id).ok_or_else(|| format!("Unknown wallpaper '{id}'"))?;
    if kind == "svg" {
        return Err("Vector wallpapers must ship in the package; only photo packs download".into());
    }
    let url = format!("{SOURCE_BASE}/{id}.jpg");
    let mut response = agent
        .get(&url)
        .header("User-Agent", "LumaNative")
        .call()
        .map_err(|error| error.to_string())?;
    let mut bytes = Vec::new();
    response
        .body_mut()
        .as_reader()
        .take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err("Wallpaper download exceeded 8 MiB".into());
    }
    install_photo(cache, id, &bytes)
}

fn install_photo(cache: &Path, id: &str, bytes: &[u8]) -> Result<(), String> {
    let image = decode_cover(bytes, id)?;
    let plain = jpeg(&image, 90)?;
    let mut shaded = image.clone();
    shade(&mut shaded);
    let shaded_jpeg = jpeg(&shaded, 90)?;
    let thumbnail =
        image::imageops::resize(&image, 480, 270, image::imageops::FilterType::Triangle);
    let thumbnail = jpeg(&thumbnail, 82)?;

    fs::create_dir_all(cache).map_err(|error| error.to_string())?;
    let staging = cache.join(format!(".{id}"));
    let _ = fs::remove_dir_all(&staging);
    fs::create_dir_all(&staging).map_err(|error| error.to_string())?;
    let finish = (|| {
        write(&staging.join(format!("{id}-plain.jpg")), &plain)?;
        write(&staging.join(format!("{id}.jpg")), &shaded_jpeg)?;
        write(&staging.join(format!("{id}.thumb.jpg")), &thumbnail)?;
        for (name, bytes) in [
            (format!("{id}-plain.jpg"), &plain),
            (format!("{id}.jpg"), &shaded_jpeg),
        ] {
            let decoded = image::load_from_memory(bytes).map_err(|error| error.to_string())?;
            let atlas = crate::glass_backdrop::prepare(decoded);
            let mut png = std::io::Cursor::new(Vec::new());
            atlas
                .write_to(&mut png, image::ImageFormat::Png)
                .map_err(|error| error.to_string())?;
            write(
                &crate::glass_backdrop::sidecar_path(&staging.join(name)),
                png.get_ref(),
            )?;
        }
        let final_dir = pack_dir(cache, id);
        let _ = fs::remove_dir_all(&final_dir);
        fs::rename(&staging, &final_dir).map_err(|error| error.to_string())
    })();
    if finish.is_err() {
        let _ = fs::remove_dir_all(&staging);
    }
    finish
}

fn decode_cover(bytes: &[u8], _id: &str) -> Result<image::RgbImage, String> {
    let image = image::load_from_memory(bytes).map_err(|error| error.to_string())?;
    Ok(image
        .resize_to_fill(1920, 1080, image::imageops::FilterType::Triangle)
        .to_rgb8())
}

fn shade(image: &mut image::RgbImage) {
    let stops = [
        (0., [13., 16., 19.], 0.24),
        (0.19, [14., 17., 21.], 0.08),
        (0.35, [15., 17., 20.], 0.35),
        (0.63, [17., 19., 23.], 0.86),
        (0.85, [23., 25., 29.], 0.92157),
        (1., [23., 25., 29.], 0.96078),
    ];
    for (x, y, pixel) in image.enumerate_pixels_mut() {
        let y = y as f32 / 1079.;
        let pair = stops.windows(2).find(|s| y <= s[1].0).unwrap();
        let t = (y - pair[0].0) / (pair[1].0 - pair[0].0);
        let alpha = pair[0].2 * (1. - t) + pair[1].2 * t;
        let horizontal = 0.42 * (1. - x as f32 / (1919. * 0.75)).max(0.);
        for i in 0..3 {
            let base = pixel[i] as f32 * 0.92 + [23., 25., 28.][i] * 0.08;
            let base = base * (1. - horizontal) + [17., 20., 23.][i] * horizontal;
            let gradient = pair[0].1[i] * pair[0].2 * (1. - t) + pair[1].1[i] * pair[1].2 * t;
            pixel[i] = (base * (1. - alpha) + gradient).round() as u8;
        }
    }
}

fn jpeg(image: &image::RgbImage, quality: u8) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, quality)
        .encode_image(image)
        .map_err(|error| error.to_string())?;
    Ok(bytes)
}

fn write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    fs::write(path, bytes).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vector_wallpapers_ship_photos_download() {
        assert!(bundled("slate"));
        assert!(bundled("midnight"));
        assert!(!bundled("dunes"));
        assert!(remote_builtin("yosemite"));
        assert!(!remote_builtin("custom-1"));
    }

    #[test]
    fn install_photo_writes_gradient_plain_and_glass() {
        let dir = std::env::temp_dir().join(format!("luma-wallpaper-pack-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let source = image::RgbImage::from_fn(64, 36, |x, y| {
            image::Rgb([(x * 3) as u8, (y * 5) as u8, 90])
        });
        let mut bytes = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, 90)
            .encode_image(&source)
            .unwrap();
        install_photo(&dir, "dunes", &bytes).unwrap();
        assert!(installed(&dir, Path::new("/missing"), "dunes"));
        install_photo(&dir, "yosemite", &bytes).unwrap();
        assert!(installed(&dir, Path::new("/missing"), "yosemite"));
        prune_cache(&dir, &["dunes".into()]);
        assert!(installed(&dir, Path::new("/missing"), "dunes"));
        assert!(!installed(&dir, Path::new("/missing"), "yosemite"));
        assert!(thumbnail_file(&dir, "yosemite").is_file());
        let _ = fs::remove_dir_all(&dir);
    }
}
