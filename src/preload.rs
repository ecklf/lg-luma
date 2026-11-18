//! Startup image decoding on worker threads. The first scene's images (wallpaper, its glass
//! atlas, surfaces) need no GL context, so they decode on other cores while SDL, the window
//! and the shaders start. The renderer takes a decoded image instead of decoding it, and
//! waits only for an image that is still being decoded. After startup, `finish` drops what
//! was not used, and workers still decoding discard their results.
use image::RgbaImage;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, OnceLock};

enum Slot {
    Pending,
    Ready(Option<RgbaImage>),
}

#[derive(Default)]
struct State {
    slots: HashMap<String, Slot>,
    queue: VecDeque<(String, Kind)>,
    /// Images being decoded right now.
    busy: usize,
    /// Startup is over: nothing more is queued, published or handed out.
    finished: bool,
}

#[derive(Default)]
struct Shared {
    state: Mutex<State>,
    changed: Condvar,
}

/// What a path holds: an ordinary image, or a precomputed glass atlas.
#[derive(Clone, Copy)]
pub enum Kind {
    Image,
    GlassAtlas,
}

fn decode_source(path: &str, kind: Kind) -> Option<RgbaImage> {
    match kind {
        Kind::Image => image::open(path).ok().map(image::DynamicImage::into_rgba8),
        Kind::GlassAtlas => crate::glass_backdrop::load(Path::new(path)).ok(),
    }
}

/// Where large decoded images are kept between launches (webOS only).
static IMAGE_CACHE: OnceLock<PathBuf> = OnceLock::new();
/// Images from this many pixels on are cached: wallpapers and glass atlases, not icons.
const CACHED_PIXELS: u64 = 512 * 512;
const CACHED_FILES: usize = 4;
const MAGIC: &[u8; 8] = b"LUMARGBA";

pub fn set_image_cache(directory: PathBuf) {
    let _ = IMAGE_CACHE.set(directory);
}

/// The cache file for this exact source file: its path, size and modification time.
fn cache_file(directory: &Path, path: &str, kind: Kind) -> Option<PathBuf> {
    use std::hash::{Hash, Hasher};
    let metadata = std::fs::metadata(path).ok()?;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    (path, metadata.len(), matches!(kind, Kind::GlassAtlas)).hash(&mut hasher);
    metadata
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_nanos()
        .hash(&mut hasher);
    Some(directory.join(format!("{:016x}.rgba", hasher.finish())))
}

fn read_cached(file: &Path) -> Option<RgbaImage> {
    let mut bytes = std::fs::read(file).ok()?;
    if bytes.len() < 16 || &bytes[..8] != MAGIC {
        return None;
    }
    let width = u32::from_le_bytes(bytes[8..12].try_into().ok()?);
    let height = u32::from_le_bytes(bytes[12..16].try_into().ok()?);
    if bytes.len() as u64 != 16 + u64::from(width) * u64::from(height) * 4 {
        return None;
    }
    bytes.drain(..16);
    RgbaImage::from_raw(width, height, bytes)
}

fn write_cached(directory: &Path, file: &Path, image: &RgbaImage) {
    if std::fs::create_dir_all(directory).is_err() {
        return;
    }
    let mut bytes = Vec::with_capacity(16 + image.as_raw().len());
    bytes.extend_from_slice(MAGIC);
    bytes.extend_from_slice(&image.width().to_le_bytes());
    bytes.extend_from_slice(&image.height().to_le_bytes());
    bytes.extend_from_slice(image.as_raw());
    // Write beside, then rename: a crash never leaves a truncated image in place.
    let partial = file.with_extension("partial");
    if std::fs::write(&partial, bytes).is_err() || std::fs::rename(&partial, file).is_err() {
        let _ = std::fs::remove_file(&partial);
        return;
    }
    // Keep the newest few: older wallpapers' entries are not needed again soon.
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    let mut files: Vec<_> = entries
        .flatten()
        .filter(|entry| entry.path().extension().is_some_and(|e| e == "rgba"))
        .filter_map(|entry| Some((entry.metadata().ok()?.modified().ok()?, entry.path())))
        .collect();
    files.sort();
    let excess = files.len().saturating_sub(CACHED_FILES);
    for (_, path) in files.into_iter().take(excess) {
        let _ = std::fs::remove_file(path);
    }
}

/// Decodes an image, reusing a cached decode of large images when the file is unchanged.
fn decode(path: &str, kind: Kind) -> Option<RgbaImage> {
    let Some(directory) = IMAGE_CACHE.get() else {
        return decode_source(path, kind);
    };
    let file = cache_file(directory, path, kind);
    if let Some(image) = file.as_deref().and_then(read_cached) {
        return Some(image);
    }
    let image = decode_source(path, kind)?;
    if let Some(file) = file
        && u64::from(image.width()) * u64::from(image.height()) >= CACHED_PIXELS
    {
        write_cached(directory, &file, &image);
    }
    Some(image)
}

#[derive(Default)]
struct Preloader {
    shared: Arc<Shared>,
}

impl Preloader {
    fn start(&self, paths: Vec<(String, Kind)>, threads: usize) {
        {
            let mut state = self.shared.state.lock().unwrap();
            if state.finished {
                return;
            }
            for (path, kind) in paths {
                if !state.slots.contains_key(&path) {
                    state.slots.insert(path.clone(), Slot::Pending);
                    state.queue.push_back((path, kind));
                }
            }
        }
        for _ in 0..threads.max(1) {
            let shared = self.shared.clone();
            let spawned = std::thread::Builder::new()
                .name("luma-preload".into())
                .spawn(move || work(&shared));
            if spawned.is_err() {
                // Without a worker, the renderer decodes synchronously as before.
                let mut state = self.shared.state.lock().unwrap();
                let queued: Vec<_> = state.queue.drain(..).map(|(path, _)| path).collect();
                for path in queued {
                    state.slots.remove(&path);
                }
                self.shared.changed.notify_all();
                break;
            }
        }
    }

    fn take(&self, path: &str) -> Option<Option<RgbaImage>> {
        let mut state = self.shared.state.lock().unwrap();
        loop {
            if state.finished {
                return None;
            }
            match state.slots.remove(path)? {
                Slot::Ready(image) => return Some(image),
                Slot::Pending => {
                    state.slots.insert(path.to_owned(), Slot::Pending);
                    state = self.shared.changed.wait(state).unwrap();
                }
            }
        }
    }

    fn finish(&self) {
        let mut state = self.shared.state.lock().unwrap();
        state.finished = true;
        state.slots.clear();
        state.queue.clear();
        self.shared.changed.notify_all();
    }
}

fn work(shared: &Shared) {
    loop {
        let (path, kind) = {
            let mut state = shared.state.lock().unwrap();
            if state.finished {
                return;
            }
            let Some(next) = state.queue.pop_front() else {
                return;
            };
            state.busy += 1;
            shared.changed.notify_all();
            next
        };
        let image = decode(&path, kind);
        let mut state = shared.state.lock().unwrap();
        state.busy -= 1;
        // A result that arrives after startup cleanup is dropped, never kept.
        if !state.finished {
            state.slots.insert(path, Slot::Ready(image));
        }
        shared.changed.notify_all();
    }
}

/// Decodes `paths` one by one on a background worker, for images wanted later rather than
/// for the first frame. Each distinct path is decoded once. At most one result is queued
/// and one more is being decoded or waiting to send; dropping the receiver stops the worker.
pub fn decode_in_background(
    paths: Vec<String>,
) -> std::sync::mpsc::Receiver<(String, Option<RgbaImage>)> {
    background_decode(paths, |path| decode(path, Kind::Image))
}

fn background_decode(
    mut paths: Vec<String>,
    mut decode: impl FnMut(&str) -> Option<RgbaImage> + Send + 'static,
) -> std::sync::mpsc::Receiver<(String, Option<RgbaImage>)> {
    let mut seen = HashSet::new();
    paths.retain(|path| seen.insert(path.clone()));
    // Uploads pause during animation and while hidden. Backpressure keeps those pauses
    // from retaining a decoded copy of every card outside the renderer's cache budget.
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    let _ = std::thread::Builder::new()
        .name("luma-artwork-prefetch".into())
        .spawn(move || {
            for path in paths {
                let image = decode(&path);
                if sender.send((path, image)).is_err() {
                    break;
                }
            }
        });
    receiver
}

static PRELOAD: OnceLock<Preloader> = OnceLock::new();

/// Starts decoding `paths` on up to `threads` workers. Call once, before the renderer exists.
pub fn start(paths: Vec<(String, Kind)>, threads: usize) {
    PRELOAD.get_or_init(Default::default).start(paths, threads);
}

/// The preloaded image for `path`, waiting if it is still decoding. None when it was never
/// requested or startup is over (the caller decodes it itself); Some(None) when decoding
/// failed.
pub fn take(path: &str) -> Option<Option<RgbaImage>> {
    PRELOAD.get()?.take(path)
}

/// Ends startup preloading: drops unused images and discards results still being decoded.
pub fn finish() {
    if let Some(preload) = PRELOAD.get() {
        preload.finish();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("luma-preload-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn png_bytes() -> Vec<u8> {
        let mut bytes = std::io::Cursor::new(Vec::new());
        RgbaImage::from_pixel(3, 2, image::Rgba([1, 2, 3, 255]))
            .write_to(&mut bytes, image::ImageFormat::Png)
            .unwrap();
        bytes.into_inner()
    }

    #[test]
    fn background_decoding_deduplicates_paths_and_keeps_failures_in_order() {
        let dir = temp_dir("background-duplicates");
        let path = dir.join("a.png");
        std::fs::write(&path, png_bytes()).unwrap();
        let path = path.to_string_lossy().into_owned();
        let missing = dir.join("missing.png").to_string_lossy().into_owned();
        let mut paths = vec![path.clone(); 100];
        paths.extend([missing.clone(), path.clone(), missing.clone()]);
        let receiver = decode_in_background(paths);
        let timeout = std::time::Duration::from_secs(5);
        let (decoded_path, image) = receiver.recv_timeout(timeout).unwrap();
        assert_eq!(decoded_path, path);
        let image = image.unwrap();
        assert_eq!(image.dimensions(), (3, 2));
        assert_eq!(image.get_pixel(0, 0).0, [1, 2, 3, 255]);
        let (decoded_path, image) = receiver.recv_timeout(timeout).unwrap();
        assert_eq!(decoded_path, missing);
        assert!(image.is_none());
        assert!(matches!(
            receiver.recv_timeout(timeout),
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected)
        ));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn background_decoding_bounds_a_paused_consumer_and_stops_when_dropped() {
        let (started, decodes) = std::sync::mpsc::channel();
        let receiver = background_decode(vec!["a".into(), "b".into(), "c".into()], move |path| {
            started.send(path.to_owned()).unwrap();
            Some(RgbaImage::new(3, 2))
        });
        let timeout = std::time::Duration::from_secs(5);
        assert_eq!(decodes.recv_timeout(timeout).unwrap(), "a");
        assert_eq!(decodes.recv_timeout(timeout).unwrap(), "b");
        // With one result queued and one held by the worker, it cannot start a third.
        assert!(matches!(
            decodes.recv_timeout(std::time::Duration::from_millis(100)),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout)
        ));
        // A hidden window can discard its receiver even while the worker is blocked.
        // The closure's sender is dropped when the worker exits; no third decode starts.
        drop(receiver);
        assert!(matches!(
            decodes.recv_timeout(timeout),
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected)
        ));
    }

    #[test]
    fn background_decoding_resumes_after_the_consumer_drains_a_result() {
        let (started, decodes) = std::sync::mpsc::channel();
        let receiver = background_decode(vec!["a".into(), "b".into(), "c".into()], move |path| {
            started.send(path.to_owned()).unwrap();
            Some(RgbaImage::new(3, 2))
        });
        let timeout = std::time::Duration::from_secs(5);
        assert_eq!(decodes.recv_timeout(timeout).unwrap(), "a");
        assert_eq!(decodes.recv_timeout(timeout).unwrap(), "b");
        assert_eq!(receiver.recv_timeout(timeout).unwrap().0, "a");
        assert_eq!(decodes.recv_timeout(timeout).unwrap(), "c");
        assert_eq!(receiver.recv_timeout(timeout).unwrap().0, "b");
        assert_eq!(receiver.recv_timeout(timeout).unwrap().0, "c");
        assert!(matches!(
            receiver.recv_timeout(timeout),
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected)
        ));
    }

    #[test]
    fn preloaded_images_are_taken_once_and_unknown_paths_fall_back() {
        let dir = temp_dir("take");
        let path = dir.join("a.png");
        std::fs::write(&path, png_bytes()).unwrap();
        let path = path.to_string_lossy().into_owned();
        let missing = dir.join("missing.png").to_string_lossy().into_owned();
        let preload = Preloader::default();
        preload.start(
            vec![(path.clone(), Kind::Image), (missing.clone(), Kind::Image)],
            2,
        );
        let image = preload.take(&path).unwrap().unwrap();
        assert_eq!(image.dimensions(), (3, 2));
        assert_eq!(image.get_pixel(0, 0).0, [1, 2, 3, 255]);
        assert!(preload.take(&path).is_none(), "taken images are not kept");
        assert!(matches!(preload.take(&missing), Some(None)));
        assert!(preload.take("never-requested.png").is_none());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn large_decodes_are_cached_by_file_identity_and_damaged_entries_are_ignored() {
        let dir = temp_dir("cache");
        let source = dir.join("wide.png");
        let image = RgbaImage::from_fn(600, 500, |x, y| image::Rgba([x as u8, y as u8, 7, 255]));
        image.save(&source).unwrap();
        let path = source.to_string_lossy().into_owned();
        let cache = dir.join("image-cache");
        let file = cache_file(&cache, &path, Kind::Image).unwrap();
        write_cached(&cache, &file, &image);
        assert_eq!(read_cached(&file).unwrap(), image);
        // A different kind or a changed source file has another entry.
        assert_ne!(cache_file(&cache, &path, Kind::GlassAtlas).unwrap(), file);
        std::fs::write(&file, b"LUMARGBA\x01\0\0\0\x01\0\0\0").unwrap();
        assert!(
            read_cached(&file).is_none(),
            "truncated data is not an image"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn a_decode_finishing_after_cleanup_is_discarded() {
        let dir = temp_dir("late");
        // Reading a FIFO blocks until it is written: the decode is in flight during finish().
        let fifo = dir.join("late.png");
        assert!(
            std::process::Command::new("mkfifo")
                .arg(&fifo)
                .status()
                .unwrap()
                .success()
        );
        let path = fifo.to_string_lossy().into_owned();
        let preload = Preloader::default();
        preload.start(vec![(path.clone(), Kind::Image)], 1);
        {
            let state = preload.shared.state.lock().unwrap();
            drop(
                preload
                    .shared
                    .changed
                    .wait_while(state, |state| state.busy == 0)
                    .unwrap(),
            );
        }
        preload.finish();
        std::fs::write(&fifo, png_bytes()).unwrap();
        let state = preload.shared.state.lock().unwrap();
        let state = preload
            .shared
            .changed
            .wait_while(state, |state| state.busy > 0)
            .unwrap();
        assert!(state.slots.is_empty(), "a late result must not be kept");
        drop(state);
        assert!(preload.take(&path).is_none());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
