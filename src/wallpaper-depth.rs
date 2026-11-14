//! Bounded mask decoding and placement preparation, on the wallpaper worker only.
use std::{collections::BTreeMap, io::Cursor};

pub(super) fn decode_mask(bytes: &[u8]) -> Result<image::GrayImage, String> {
    if bytes.len() > super::MAX_BODY
        || image::guess_format(bytes).ok() != Some(image::ImageFormat::Png)
    {
        return Err("The foreground mask must be a PNG image".into());
    }
    let mut reader = image::ImageReader::with_format(Cursor::new(bytes), image::ImageFormat::Png);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(1920);
    limits.max_image_height = Some(1080);
    limits.max_alloc = Some(16 * 1024 * 1024);
    reader.limits(limits);
    let decoder = reader
        .into_decoder()
        .map_err(|e| format!("Invalid foreground mask: {e}"))?;
    use image::ImageDecoder;
    if decoder.dimensions() != (1920, 1080) {
        return Err("The foreground mask must match the 1920 × 1080 crop".into());
    }
    let rgba = image::DynamicImage::from_decoder(decoder)
        .map_err(|e| e.to_string())?
        .to_rgba8();
    let mask = image::GrayImage::from_fn(1920, 1080, |x, y| {
        let p = rgba.get_pixel(x, y).0;
        let luminance =
            (u32::from(p[0]) * 299 + u32::from(p[1]) * 587 + u32::from(p[2]) * 114) / 1000;
        image::Luma([(luminance * u32::from(p[3]) / 255) as u8])
    });
    if mask.as_raw().iter().min() == mask.as_raw().iter().max() {
        return Err("The mask needs both background and foreground areas".into());
    }
    Ok(mask)
}

pub(super) fn png(pixels: &[u8], color: image::ExtendedColorType) -> Result<Vec<u8>, String> {
    use image::ImageEncoder;
    let mut bytes = Vec::new();
    image::codecs::png::PngEncoder::new_with_quality(
        &mut bytes,
        image::codecs::png::CompressionType::Fast,
        image::codecs::png::FilterType::Sub,
    )
    .write_image(pixels, 1920, 1080, color)
    .map_err(|e| e.to_string())?;
    Ok(bytes)
}

pub(super) fn mask_placements(mask: &image::GrayImage) -> BTreeMap<String, Vec<Option<f32>>> {
    // A summed-area table makes each trial constant-time. Its maximum sum is
    // 1920*1080*255, safely inside u32, with a fixed 8 MiB allocation.
    let mut sums = vec![0u32; 1921 * 1081];
    for y in 0..1080 {
        let mut row = 0;
        for x in 0..1920 {
            row += u32::from(mask.get_pixel(x as u32, y as u32).0[0]);
            sums[(y + 1) * 1921 + x + 1] = sums[y * 1921 + x + 1] + row;
        }
    }
    let mean = |x: usize, y: usize, w: usize, h: usize| {
        let sum = sums[(y + h) * 1921 + x + w] + sums[y * 1921 + x]
            - sums[y * 1921 + x + w]
            - sums[(y + h) * 1921 + x];
        sum as f32 / (w * h * 255) as f32
    };
    let mut result = BTreeMap::new();
    for (layout, columns, rows) in [("row", 3, 1), ("centered", 1, 1), ("grid", 3, 2)] {
        let height = (584 - (rows - 1) * 24) / rows;
        let mut positions = Vec::new();
        for slot in 0..columns * rows {
            let x = if columns == 1 {
                676
            } else {
                84 + slot % columns * 592
            };
            let top = 188 + slot / columns * (height + 24);
            let mut best = None;
            for center in (top + 108..=top + height - 108).step_by(4) {
                let upper = mean(x + 40, center - 65, 488, 65);
                let lower = mean(x + 40, center, 488, 65);
                let cover = (upper + lower) / 2.;
                if (0.06..=0.32).contains(&cover) && upper < 0.22 && lower > upper + 0.03 {
                    let score = (cover - 0.18).abs()
                        + upper
                        + (center as f32 - (top + height / 2) as f32).abs() / 5000.;
                    if best.is_none_or(|(old, _)| score < old) {
                        best = Some((score, center as f32));
                    }
                }
            }
            positions.push(best.map(|(_, center)| center));
        }
        result.insert(layout.to_owned(), positions);
    }
    result
}
