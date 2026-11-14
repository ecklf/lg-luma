import fs from 'node:fs/promises';
import path from 'node:path';
import { createHash } from 'node:crypto';
import sharp from 'sharp';

const sha256 = (bytes) => createHash('sha256').update(bytes).digest('hex');

export async function loadMasks(source) {
  const directory = path.join(source, 'wallpaper-masks');
  const manifest = JSON.parse(await fs.readFile(path.join(directory, 'manifest.json'), 'utf8'));
  for (const [id, entry] of Object.entries(manifest)) {
    if (!/^[a-z-]+$/.test(id)) throw new Error('Invalid foreground background ID');
    const photo = await fs.readFile(path.join(source, 'wallpapers', `${id}.jpg`));
    const mask = await fs.readFile(path.join(directory, `${id}.png`));
    if (sha256(photo) !== entry.source_sha256 || sha256(mask) !== entry.mask_sha256)
      throw new Error(`Foreground mask for ${id} must be regenerated for its source`);
    const metadata = await sharp(mask).metadata();
    if (metadata.width !== 1920 || metadata.height !== 1080 || metadata.channels !== 1)
      throw new Error(`Foreground mask for ${id} must be a 1920×1080 grayscale image`);
  }
  return manifest;
}

export async function generateForeground(photo, mask, output, width, height) {
  const alpha = await sharp(mask)
    .resize(width, height, { fit: 'fill' })
    .greyscale()
    .raw()
    .toBuffer();
  // Reuse final wallpaper pixels after colour/gradient processing, so compositing
  // the foreground cannot brighten or recolour it. Only the clock is occluded.
  const rgb = await sharp(photo).removeAlpha().toColourspace('srgb').raw().toBuffer();
  await sharp(rgb, { raw: { width, height, channels: 3 } })
    .joinChannel(alpha, { raw: { width, height, channels: 1 } })
    .png({ compressionLevel: 9 })
    .toFile(output);
}
