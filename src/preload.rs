//! Startup image decoding on worker threads. The first scene's images (wallpaper, its glass
//! atlas, surfaces) need no GL context, so they decode on other cores while SDL, the window
//! and the shaders start. The renderer takes a decoded image instead of decoding it, and
//! waits only for an image that is still being decoded. After startup, `finish` drops what
//! was not used, and workers still decoding discard their results.
use image::RgbaImage;
use std::collections::{HashMap, VecDeque};
use std::path::Path;
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

fn decode(path: &str, kind: Kind) -> Option<RgbaImage> {
    match kind {
        Kind::Image => image::open(path).ok().map(image::DynamicImage::into_rgba8),
        Kind::GlassAtlas => crate::glass_backdrop::load(Path::new(path)).ok(),
    }
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
/// for the first frame. Results arrive on the receiver; dropping it stops the worker.
pub fn decode_in_background(
    paths: Vec<String>,
) -> std::sync::mpsc::Receiver<(String, Option<RgbaImage>)> {
    let (sender, receiver) = std::sync::mpsc::channel();
    let _ = std::thread::Builder::new()
        .name("luma-artwork-prefetch".into())
        .spawn(move || {
            for path in paths {
                let image = decode(&path, Kind::Image);
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
