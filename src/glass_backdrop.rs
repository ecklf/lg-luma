//! The exact two-level Liquid Glass atlas, shared by offline preparation and legacy fallback.
use image::{DynamicImage, RgbaImage};
use std::path::{Path, PathBuf};

pub const WIDTH: u32 = 960;
pub const HEIGHT: u32 = 540;
pub const ATLAS_HEIGHT: u32 = HEIGHT * 2;

pub fn sidecar_path(wallpaper: &Path) -> PathBuf {
    wallpaper.with_extension("glass.png")
}

pub fn prepare(image: DynamicImage) -> RgbaImage {
    let small = image.resize_exact(WIDTH, HEIGHT, image::imageops::FilterType::Triangle);
    let sharp = small.to_rgba8();
    let blurred = small.blur(8.).to_rgba8();
    let mut atlas = RgbaImage::new(WIDTH, ATLAS_HEIGHT);
    image::imageops::replace(&mut atlas, &sharp, 0, 0);
    image::imageops::replace(&mut atlas, &blurred, 0, i64::from(HEIGHT));
    atlas
}

pub fn load(path: &Path) -> Result<RgbaImage, String> {
    let mut reader = image::ImageReader::open(path).map_err(|error| error.to_string())?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(WIDTH);
    limits.max_image_height = Some(ATLAS_HEIGHT);
    limits.max_alloc = Some(u64::from(WIDTH * ATLAS_HEIGHT) * 8);
    reader.limits(limits);
    let image = reader.decode().map_err(|error| error.to_string())?;
    if image.width() != WIDTH || image.height() != ATLAS_HEIGHT {
        return Err("Invalid glass atlas dimensions".into());
    }
    Ok(image.into_rgba8())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atlas_matches_the_original_filter_levels_and_png_round_trips_losslessly() {
        let source = DynamicImage::ImageRgb8(image::RgbImage::from_fn(64, 36, |x, y| {
            image::Rgb([(x * 4) as u8, (y * 7) as u8, ((x + y) * 2) as u8])
        }));
        let small = source.resize_exact(960, 540, image::imageops::FilterType::Triangle);
        let atlas = prepare(source);
        assert_eq!(
            image::imageops::crop_imm(&atlas, 0, 0, WIDTH, HEIGHT).to_image(),
            small.to_rgba8()
        );
        assert_eq!(
            image::imageops::crop_imm(&atlas, 0, HEIGHT, WIDTH, HEIGHT).to_image(),
            small.blur(8.).to_rgba8()
        );
        let mut bytes = std::io::Cursor::new(Vec::new());
        atlas.write_to(&mut bytes, image::ImageFormat::Png).unwrap();
        assert_eq!(
            image::load_from_memory(bytes.get_ref()).unwrap().to_rgba8(),
            atlas
        );
    }

    #[test]
    fn sidecar_names_keep_gradient_and_plain_wallpapers_distinct() {
        assert_eq!(
            sidecar_path(Path::new("wallpapers/dunes.jpg")),
            Path::new("wallpapers/dunes.glass.png")
        );
        assert_eq!(
            sidecar_path(Path::new("wallpapers/dunes-plain.jpg")),
            Path::new("wallpapers/dunes-plain.glass.png")
        );
        assert_eq!(
            sidecar_path(Path::new("custom-123/original.jpg")),
            Path::new("custom-123/original.glass.png")
        );
    }
}
