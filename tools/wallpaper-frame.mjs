import sharp from 'sharp';

// One crop/colour pipeline for packaged wallpapers and off-TV mask inference.
export async function wallpaperFrame(input, id, scale = 1) {
  const width = 1920 * scale;
  const height = 1080 * scale;
  const photo = await sharp(input, { density: 72 * scale })
    .resize(width, height, { fit: 'outside' })
    .png()
    .toBuffer({ resolveWithObject: true });
  const position = id === 'dunes' || id === 'summit' ? 0.4 : id === 'san-francisco' ? 0.42 : 0.45;
  let image = sharp(photo.data).extract({
    left: Math.round((photo.info.width - width) / 2),
    top: Math.round((photo.info.height - height) * position),
    width,
    height,
  });
  const saturation =
    id === 'san-francisco'
      ? 0.75
      : id === 'summit'
        ? 0.7
        : ['forest', 'yosemite', 'northern-lights'].includes(id)
          ? 0.8
          : 1;
  const brightness = id === 'san-francisco' ? 0.82 : id === 'summit' ? 0.8 : 1;
  if (saturation !== 1) {
    const s = saturation;
    image = image.recomb([
      [0.213 + 0.787 * s, 0.715 - 0.715 * s, 0.072 - 0.072 * s],
      [0.213 - 0.213 * s, 0.715 + 0.285 * s, 0.072 - 0.072 * s],
      [0.213 - 0.213 * s, 0.715 - 0.715 * s, 0.072 + 0.928 * s],
    ]);
  }
  return image.linear(brightness).png().toBuffer();
}
