//! Custom wallpapers: JPEG, PNG or WebP files the owner copies into `wallpapers/` beside the
//! preferences file. Each one is prepared once, off the UI thread: a 1920x1080 centre crop and a
//! 480x270 thumbnail in `wallpaper-cache/`. The renderer then only loads screen-sized JPEGs,
//! never a camera-sized photo, on a TV with about 1 GB of memory.
use image::{
    DynamicImage, ImageReader, Limits, RgbImage, codecs::jpeg::JpegEncoder, imageops::FilterType,
};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver, Sender},
    thread,
};

/// Background IDs of custom wallpapers: this prefix and the file name.
pub const PREFIX: &str = "custom:";
pub const MAX_FILES: usize = 30;
const MAX_FILE_BYTES: u64 = 40 * 1024 * 1024;
const EXTENSIONS: [&str; 4] = ["jpg", "jpeg", "png", "webp"];

/// Where the owner puts the images.
pub fn folder(preferences: &Path) -> PathBuf {
    preferences.with_file_name("wallpapers")
}

fn cache(preferences: &Path) -> PathBuf {
    preferences.with_file_name("wallpaper-cache")
}

/// A plain file name in the folder with a supported extension: no path separators, no hidden
/// files, bounded length. Anything else could point outside the folder.
pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 120
        && !name.starts_with('.')
        && !name.contains(['/', '\\', '\0'])
        && Path::new(name)
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| EXTENSIONS.contains(&extension.to_ascii_lowercase().as_str()))
}

pub fn id(name: &str) -> String {
    format!("{PREFIX}{name}")
}

/// The file name of a custom background ID, if it is one and the name is valid.
pub fn name(id: &str) -> Option<&str> {
    id.strip_prefix(PREFIX).filter(|name| valid_name(name))
}

/// Shown in the background list: the file name without its extension.
pub fn title(id: &str) -> &str {
    let name = id.strip_prefix(PREFIX).unwrap_or(id);
    name.rsplit_once('.').map_or(name, |(stem, _)| stem)
}

pub fn wallpaper(preferences: &Path, name: &str) -> PathBuf {
    cache(preferences).join(format!("{name}.jpg"))
}

pub fn thumbnail(preferences: &Path, name: &str) -> PathBuf {
    cache(preferences).join(format!("{name}.thumb.jpg"))
}

/// Usable image names in the folder, sorted, at most `MAX_FILES`.
pub fn scan(folder: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(folder) else {
        return vec![];
    };
    let mut names: Vec<String> = entries
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| valid_name(name))
        .collect();
    names.sort_by_key(|name| name.to_lowercase());
    names.truncate(MAX_FILES);
    names
}

fn decode(path: &Path) -> Result<DynamicImage, String> {
    let size = fs::metadata(path).map_err(|e| e.to_string())?.len();
    if size > MAX_FILE_BYTES {
        return Err(format!("larger than {} MB", MAX_FILE_BYTES / 1024 / 1024));
    }
    let mut reader = ImageReader::open(path)
        .and_then(ImageReader::with_guessed_format)
        .map_err(|e| e.to_string())?;
    let mut limits = Limits::default();
    // A 40-megapixel photo decodes to 160 MB of RGBA; anything larger is refused.
    limits.max_image_width = Some(12_000);
    limits.max_image_height = Some(12_000);
    limits.max_alloc = Some(192 * 1024 * 1024);
    reader.limits(limits);
    reader.decode().map_err(|e| e.to_string())
}

fn write_jpeg(image: &RgbImage, path: &Path) -> Result<(), String> {
    let temporary = path.with_extension("jpg.new");
    let file = fs::File::create(&temporary).map_err(|e| e.to_string())?;
    let mut writer = std::io::BufWriter::new(file);
    JpegEncoder::new_with_quality(&mut writer, 90)
        .encode_image(image)
        .map_err(|e| e.to_string())?;
    drop(writer);
    fs::rename(&temporary, path).map_err(|e| e.to_string())
}

fn modified(path: &Path) -> Option<std::time::SystemTime> {
    fs::metadata(path).and_then(|meta| meta.modified()).ok()
}

/// Crops like `tools/assets.mjs` does for the bundled photos: scale to cover 1920x1080, keep the
/// centre horizontally and 45% from the top vertically.
fn cover(image: &DynamicImage) -> RgbImage {
    let (width, height) = (image.width().max(1) as f32, image.height().max(1) as f32);
    let scale = (1920. / width).max(1080. / height);
    let (scaled_width, scaled_height) = (
        (width * scale).round().max(1920.) as u32,
        (height * scale).round().max(1080.) as u32,
    );
    let scaled = image
        .resize_exact(scaled_width, scaled_height, FilterType::Triangle)
        .into_rgb8();
    let left = (scaled_width - 1920) / 2;
    let top = ((scaled_height - 1080) as f32 * 0.45).round() as u32;
    image::imageops::crop_imm(&scaled, left, top, 1920, 1080).to_image()
}

/// The shading `tools/assets.mjs` gives the bundled wallpapers, so the clock, rows and text stay
/// legible on the owner's photos too: the photo at 92% over #17191c, a dark gradient from the
/// left, and the vertical gradient that darkens towards the rows.
fn shade(image: &mut RgbImage) {
    const VERTICAL: [(f32, u32, f32); 6] = [
        (0., 0x0d1013, 0.24),
        (0.19, 0x0e1115, 0.08),
        (0.35, 0x0f1114, 0.35),
        (0.63, 0x111317, 0.86),
        (0.85, 0x17191d, 0.92157),
        (1., 0x17191d, 0.96078),
    ];
    let rgb = |color: u32| {
        [
            (color >> 16 & 0xff) as f32,
            (color >> 8 & 0xff) as f32,
            (color & 0xff) as f32,
        ]
    };
    let blend = |pixel: &mut [f32; 3], color: [f32; 3], alpha: f32| {
        for channel in 0..3 {
            pixel[channel] += (color[channel] - pixel[channel]) * alpha;
        }
    };
    let (width, height) = (image.width(), image.height());
    for y in 0..height {
        let t = (y as f32 + 0.5) / height as f32;
        let segment = VERTICAL
            .windows(2)
            .find(|pair| t <= pair[1].0)
            .unwrap_or(&VERTICAL[4..]);
        let (start, end) = (segment[0], segment[1]);
        let f = ((t - start.0) / (end.0 - start.0)).clamp(0., 1.);
        let (a, b) = (rgb(start.1), rgb(end.1));
        let vertical = [
            a[0] + (b[0] - a[0]) * f,
            a[1] + (b[1] - a[1]) * f,
            a[2] + (b[2] - a[2]) * f,
        ];
        let vertical_alpha = start.2 + (end.2 - start.2) * f;
        for x in 0..width {
            let horizontal_alpha = 0.42 * (1. - (x as f32 + 0.5) / (width as f32 * 0.75)).max(0.);
            let source = image.get_pixel(x, y).0;
            let mut pixel = [0.; 3];
            for channel in 0..3 {
                pixel[channel] = rgb(0x17191c)[channel] * 0.08 + source[channel] as f32 * 0.92;
            }
            blend(&mut pixel, rgb(0x111417), horizontal_alpha);
            blend(&mut pixel, vertical, vertical_alpha);
            image.put_pixel(
                x,
                y,
                image::Rgb(pixel.map(|value| value.round().clamp(0., 255.) as u8)),
            );
        }
    }
}

/// Writes the cached wallpaper and thumbnail for one image, unless they are newer than it.
pub fn prepare(preferences: &Path, name: &str) -> Result<(), String> {
    let source = folder(preferences).join(name);
    let (full, thumb) = (wallpaper(preferences, name), thumbnail(preferences, name));
    if let (Some(source_time), Some(full_time), Some(_)) =
        (modified(&source), modified(&full), modified(&thumb))
        && full_time >= source_time
    {
        return Ok(());
    }
    let image = decode(&source)?;
    let mut screen = cover(&image);
    drop(image);
    fs::create_dir_all(cache(preferences)).map_err(|e| e.to_string())?;
    // The thumbnail shows the photo itself, like the bundled ones; the wallpaper is shaded.
    let small = DynamicImage::ImageRgb8(screen.clone())
        .resize_exact(480, 270, FilterType::Triangle)
        .into_rgb8();
    write_jpeg(&small, &thumb)?;
    shade(&mut screen);
    write_jpeg(&screen, &full)
}

/// Prepares every image in the folder, removes cache files of images that are gone, and returns
/// the IDs of those ready to show.
pub fn refresh(preferences: &Path) -> Vec<String> {
    let names = scan(&folder(preferences));
    let mut ready = Vec::new();
    for name in &names {
        match prepare(preferences, name) {
            Ok(()) => ready.push(id(name)),
            Err(error) => eprintln!("Luma wallpaper {name}: {error}"),
        }
    }
    if let Ok(entries) = fs::read_dir(cache(preferences)) {
        for entry in entries.filter_map(Result::ok) {
            let file = entry.file_name().to_string_lossy().into_owned();
            let source = file
                .strip_suffix(".thumb.jpg")
                .or_else(|| file.strip_suffix(".jpg"));
            if source.is_some_and(|source| !names.iter().any(|name| name == source)) {
                let _ = fs::remove_file(entry.path());
            }
        }
    }
    ready
}

/// Runs `refresh` on its own thread; one scan at a time, results arrive on `receiver`.
pub struct Worker {
    requests: Sender<()>,
    pub receiver: Receiver<Vec<String>>,
}

impl Worker {
    pub fn new(preferences: PathBuf) -> Self {
        let (requests, incoming) = mpsc::channel::<()>();
        let (results, receiver) = mpsc::channel();
        thread::Builder::new()
            .name("luma-wallpapers".into())
            .spawn(move || {
                while incoming.recv().is_ok() {
                    // Coalesce requests that queued up during a scan.
                    while incoming.try_recv().is_ok() {}
                    if results.send(refresh(&preferences)).is_err() {
                        break;
                    }
                }
            })
            .expect("spawn the wallpaper worker");
        Self { requests, receiver }
    }

    pub fn request(&self) {
        let _ = self.requests.send(());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageFormat, RgbImage};
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn temporary() -> PathBuf {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "luma-wallpapers-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("wallpapers")).unwrap();
        dir
    }

    fn photo(path: &Path, width: u32, height: u32) {
        RgbImage::from_fn(width, height, |x, y| {
            image::Rgb([(x % 256) as u8, (y % 256) as u8, 90])
        })
        .save_with_format(path, ImageFormat::Png)
        .unwrap();
    }

    #[test]
    fn only_plain_image_names_inside_the_folder_are_valid() {
        for name in ["Beach.JPG", "my photo.webp", "a.jpeg", "x.png"] {
            assert!(valid_name(name), "{name}");
        }
        for name in [
            "",
            ".hidden.jpg",
            "../x.jpg",
            "a/b.jpg",
            "a\\b.png",
            "x.gif",
            "noext",
        ] {
            assert!(!valid_name(name), "{name}");
        }
        assert!(!valid_name(&format!("{}.jpg", "a".repeat(120))));
        assert_eq!(name(&id("Beach.jpg")), Some("Beach.jpg"));
        assert_eq!(name("custom:../../etc/passwd.jpg"), None);
        assert_eq!(name("dunes"), None);
        assert_eq!(title("custom:Summer 2026.jpeg"), "Summer 2026");
    }

    #[test]
    fn scan_lists_supported_files_sorted_and_bounded() {
        let dir = temporary();
        let folder = dir.join("wallpapers");
        for name in ["b.png", "A.jpg", "c.gif", ".hidden.jpg"] {
            fs::write(folder.join(name), b"x").unwrap();
        }
        fs::create_dir(folder.join("sub.jpg")).unwrap();
        assert_eq!(scan(&folder), ["A.jpg", "b.png"]);
        for index in 0..40 {
            fs::write(folder.join(format!("{index:02}.jpg")), b"x").unwrap();
        }
        assert_eq!(scan(&folder).len(), MAX_FILES);
        assert_eq!(scan(&dir.join("missing")), Vec::<String>::new());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn images_are_cropped_to_the_screen_cached_and_cleaned_up() {
        let dir = temporary();
        let preferences = dir.join("preferences.json");
        photo(&dir.join("wallpapers/wide.png"), 3000, 1000);
        fs::write(dir.join("wallpapers/broken.jpg"), b"not an image").unwrap();
        assert_eq!(refresh(&preferences), [id("wide.png")]);
        let full = image::open(wallpaper(&preferences, "wide.png")).unwrap();
        assert_eq!((full.width(), full.height()), (1920, 1080));
        let small = image::open(thumbnail(&preferences, "wide.png")).unwrap();
        assert_eq!((small.width(), small.height()), (480, 270));
        // An unchanged image is not prepared again.
        let written = modified(&wallpaper(&preferences, "wide.png")).unwrap();
        prepare(&preferences, "wide.png").unwrap();
        assert_eq!(
            modified(&wallpaper(&preferences, "wide.png")).unwrap(),
            written
        );
        // Removing the image removes its cache files.
        fs::remove_file(dir.join("wallpapers/wide.png")).unwrap();
        assert!(refresh(&preferences).is_empty());
        assert!(!wallpaper(&preferences, "wide.png").exists());
        assert!(!thumbnail(&preferences, "wide.png").exists());
        fs::remove_dir_all(dir).unwrap();
    }
}
