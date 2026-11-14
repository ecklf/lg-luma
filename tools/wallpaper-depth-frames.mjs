import fs from 'node:fs/promises';
import path from 'node:path';
import sharp from 'sharp';
import { wallpaperFrame } from './wallpaper-frame.mjs';

const root = path.resolve(import.meta.dirname, '..');
const source = path.join(root, 'assets/wallpapers');
const output = path.join(root, '.build/depth-frames');
await fs.mkdir(output, { recursive: true });
for (const file of await fs.readdir(source)) {
  if (!file.endsWith('.jpg')) continue;
  const id = path.parse(file).name;
  await sharp(await wallpaperFrame(path.join(source, file), id))
    .jpeg({ quality: 96 })
    .toFile(path.join(output, file));
}
