import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import path from 'node:path';
import test from 'node:test';
import sharp from 'sharp';
import { generateForeground, loadMasks } from '../tools/wallpaper-foregrounds.mjs';

const root = path.resolve(import.meta.dirname, '..');

test('bundled foreground masks match their photos and preserve clear and opaque regions', async () => {
  const manifest = await loadMasks(path.join(root, 'assets'));
  assert.equal(Object.keys(manifest).length, 6);
  assert.ok(manifest.dunes.placements.centered[0]);
  for (const [id, entry] of Object.entries(manifest)) {
    assert.equal(entry.model_revision, '4472b7362082ad9968fee890ca0f1e5aca36b93d');
    const pixels = await sharp(path.join(root, 'assets/wallpaper-masks', `${id}.png`))
      .raw()
      .toBuffer();
    assert.ok(pixels.includes(0), `${id} must leave background visible`);
    assert.ok(pixels.includes(255), `${id} must have opaque foreground`);
    for (const [layout, slots] of Object.entries(entry.placements)) {
      assert.equal(slots.length, { row: 3, centered: 1, grid: 6 }[layout]);
      for (const center of slots)
        assert.ok(center === null || (Number.isFinite(center) && center >= 296 && center <= 664));
    }
  }
});

test('foreground generation changes alpha only, never wallpaper colour', async () => {
  await fs.mkdir(path.join(root, '.build'), { recursive: true });
  const temporary = await fs.mkdtemp(path.join(root, '.build/foreground-test-'));
  try {
    const photo = await sharp(Buffer.from([12, 34, 56, 78, 90, 123]), {
      raw: { width: 2, height: 1, channels: 3 },
    })
      .png()
      .toBuffer();
    const mask = await sharp(Buffer.from([0, 255]), { raw: { width: 2, height: 1, channels: 1 } })
      .png()
      .toBuffer();
    const output = path.join(temporary, 'foreground.png');
    await generateForeground(photo, mask, output, 2, 1);
    assert.deepEqual(
      [...(await sharp(output).raw().toBuffer())],
      [12, 34, 56, 0, 78, 90, 123, 255],
    );
  } finally {
    await fs.rm(temporary, { recursive: true, force: true });
  }
});
