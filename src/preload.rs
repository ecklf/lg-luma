//! Startup image decoding on worker threads. The first scene's images (wallpaper, its glass
//! atlas, surfaces) need no GL context, so they decode on other cores while SDL, the window
//! and the shaders start. The renderer takes a preloaded image instead of decoding it, and
//! waits only for an image that is still being decoded.
use image::RgbaImage;
use std::collections::{HashMap, VecDeque};
use std::path::Path;
use std::sync::{Arc, Condvar, Mutex, OnceLock};

enum Slot {
    Pending,
    Ready(Option<RgbaImage>),
}

#[derive(Default)]
struct Shared {
    slots: Mutex<HashMap<String, Slot>>,
    ready: Condvar,
}

static PRELOAD: OnceLock<Arc<Shared>> = OnceLock::new();

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

/// Starts decoding `paths` on up to `threads` workers. Call once, before the renderer exists.
pub fn start(paths: Vec<(String, Kind)>, threads: usize) {
    let shared = PRELOAD.get_or_init(Default::default).clone();
    let mut queue = VecDeque::new();
    {
        let mut slots = shared.slots.lock().unwrap();
        for (path, kind) in paths {
            if !slots.contains_key(&path) {
                slots.insert(path.clone(), Slot::Pending);
                queue.push_back((path, kind));
            }
        }
    }
    let queue = Arc::new(Mutex::new(queue));
    for _ in 0..threads.max(1) {
        let (worker_shared, worker_queue) = (shared.clone(), queue.clone());
        let spawned = std::thread::Builder::new()
            .name("luma-preload".into())
            .spawn(move || {
                let (shared, queue) = (worker_shared, worker_queue);
                loop {
                    let Some((path, kind)) = queue.lock().unwrap().pop_front() else {
                        break;
                    };
                    let image = decode(&path, kind);
                    shared
                        .slots
                        .lock()
                        .unwrap()
                        .insert(path, Slot::Ready(image));
                    shared.ready.notify_all();
                }
            });
        if spawned.is_err() {
            // Without a worker, the renderer decodes synchronously as before.
            let mut slots = shared.slots.lock().unwrap();
            for (path, _) in queue.lock().unwrap().drain(..) {
                slots.remove(&path);
            }
            break;
        }
    }
}

/// The preloaded image for `path`, waiting if it is still decoding. None when it was never
/// requested (the caller decodes it itself); Some(None) when decoding failed.
pub fn take(path: &str) -> Option<Option<RgbaImage>> {
    let shared = PRELOAD.get()?;
    let mut slots = shared.slots.lock().unwrap();
    loop {
        match slots.remove(path)? {
            Slot::Ready(image) => return Some(image),
            Slot::Pending => {
                slots.insert(path.to_owned(), Slot::Pending);
                slots = shared.ready.wait(slots).unwrap();
            }
        }
    }
}

/// Drops images the first frames did not use, once startup is over.
pub fn finish() {
    if let Some(shared) = PRELOAD.get() {
        shared
            .slots
            .lock()
            .unwrap()
            .retain(|_, slot| matches!(slot, Slot::Pending));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preloaded_images_are_taken_once_and_unknown_paths_fall_back() {
        let dir = std::env::temp_dir().join(format!("luma-preload-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("a.png");
        RgbaImage::from_pixel(3, 2, image::Rgba([1, 2, 3, 255]))
            .save(&path)
            .unwrap();
        let path = path.to_string_lossy().into_owned();
        let missing = dir.join("missing.png").to_string_lossy().into_owned();
        start(
            vec![(path.clone(), Kind::Image), (missing.clone(), Kind::Image)],
            2,
        );
        let image = take(&path).unwrap().unwrap();
        assert_eq!(image.dimensions(), (3, 2));
        assert_eq!(image.get_pixel(0, 0).0, [1, 2, 3, 255]);
        assert!(take(&path).is_none(), "taken images are not kept");
        assert!(matches!(take(&missing), Some(None)));
        assert!(take("never-requested.png").is_none());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
