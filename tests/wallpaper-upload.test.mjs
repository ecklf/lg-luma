import assert from 'node:assert/strict';
import test from 'node:test';
import { cropRect, panPosition } from '../assets/wallpaper-upload/crop.js';
import { readFile } from 'node:fs/promises';
import { loadStoragePath } from '../assets/wallpaper-upload/storage.js';
import {
  maskPlacements,
  packWallpaper,
  suggestedCutoff,
  thresholdAlpha,
} from '../assets/wallpaper-upload/mask.js';
import { sha256 } from '../assets/wallpaper-upload/sha256.js';
import { createHash } from 'node:crypto';

test('portrait, landscape, panorama and zoom crops stay in bounds at 16:9', () => {
  for (const [width, height] of [
    [4000, 3000],
    [3000, 4000],
    [8000, 500],
    [1920, 1080],
    [1, 12000],
  ]) {
    for (const zoom of [1, 1.5, 3]) {
      for (const x of [0, 0.5, 1]) {
        for (const y of [0, 0.5, 1]) {
          const crop = cropRect(width, height, zoom, x, y);
          assert.ok(Math.abs(crop.w / crop.h - 16 / 9) < 1e-9);
          assert.ok(crop.x >= -1e-9 && crop.y >= -1e-9);
          assert.ok(crop.x + crop.w <= width + 1e-9);
          assert.ok(crop.y + crop.h <= height + 1e-9);
        }
      }
    }
  }
});

test('dragging clamps at both edges and handles axes without overflow', () => {
  assert.equal(panPosition(0.5, 10000, 4000, 2000, 960), 0);
  assert.equal(panPosition(0.5, -10000, 4000, 2000, 960), 1);
  assert.equal(panPosition(0.5, 100, 1920, 1920, 960), 0.5);
  assert.ok(panPosition(0.5, 20, 4000, 2000, 960) < 0.5);
});

test('upload page needs no name and retains its storage-path success view', async () => {
  const html = await readFile(
    new URL('../assets/wallpaper-upload/index.html', import.meta.url),
    'utf8',
  );
  const script = await readFile(
    new URL('../assets/wallpaper-upload/app.js', import.meta.url),
    'utf8',
  );
  assert.doesNotMatch(html, /id="name"|success-name|Background name/);
  assert.match(html, /Your background was uploaded to/);
  assert.match(html, /id="success-path"/);
  assert.match(html, /Add another one/);
  assert.match(script, /fetch\('upload',/);
  assert.doesNotMatch(script, /cleanName|upload\?name/);
});

test('storage path resolves to the exact absolute directory and rejects unavailable or unsafe values', async () => {
  const path = '/media/internal/.com.ecklf.luma/wallpapers';
  assert.equal(
    await loadStoragePath(async (url, options) => {
      assert.equal(url, 'info.json');
      assert.deepEqual(options, { cache: 'no-store' });
      return { ok: true, json: async () => ({ path }) };
    }),
    path,
  );
  await assert.rejects(
    loadStoragePath(async () => ({ ok: false })),
    /unavailable/,
  );
  await assert.rejects(
    loadStoragePath(async () => ({ ok: true, json: async () => ({ path: 'relative/path' }) })),
    /Invalid storage location/,
  );
  await assert.rejects(
    loadStoragePath(async () => ({ ok: true, json: async () => ({ path: 42 }) })),
    /Invalid storage location/,
  );
});

test('private-HTTP model hashing matches SHA-256 at block and padding boundaries', () => {
  for (const size of [0, 1, 55, 56, 63, 64, 65, 127, 128, 1024, 65537]) {
    const bytes = Uint8Array.from({ length: size }, (_, i) => i % 251);
    assert.equal(sha256(bytes), createHash('sha256').update(bytes).digest('hex'));
  }
});

test('thresholds and safe placements retain usable background and foreground', () => {
  assert.deepEqual([...thresholdAlpha(new Uint8Array([0, 10, 11, 255]), 10)], [0, 0, 255, 255]);
  const depth = Uint8Array.from({ length: 1920 * 1080 }, (_, i) =>
    Math.floor(i / 1920) >= 500 ? 180 : 0,
  );
  const cutoff = suggestedCutoff(depth, 1920, 1080);
  const placements = maskPlacements(thresholdAlpha(depth, cutoff));
  assert.equal(placements.row.length, 3);
  assert.equal(placements.grid.length, 6);
  assert.ok(placements.centered[0] >= 296 && placements.centered[0] <= 664);
  assert.deepEqual(maskPlacements(new Uint8Array(1920 * 1080)).centered, [null]);
  assert.throws(() => maskPlacements(new Uint8Array(1)), /dimensions/);
});

test('masked uploads encode exact photo and mask lengths with no JSON/base64 overhead', async () => {
  const body = packWallpaper(new Blob(['photo']), new Blob(['mask']));
  const bytes = new Uint8Array(await body.arrayBuffer());
  assert.equal(body.type, 'application/x-luma-wallpaper');
  assert.equal(new TextDecoder().decode(bytes.subarray(0, 8)), 'LUMAWP01');
  assert.equal(new DataView(bytes.buffer).getUint32(8, false), 5);
  assert.equal(new DataView(bytes.buffer).getUint32(12, false), 4);
  assert.equal(new TextDecoder().decode(bytes.subarray(16)), 'photomask');
  assert.throws(() => packWallpaper(new Blob([]), new Blob(['x'])), /12 MiB/);
  assert.throws(
    () => packWallpaper(new Blob([new Uint8Array(12 * 1024 * 1024)]), new Blob(['x'])),
    /12 MiB/,
  );
});
