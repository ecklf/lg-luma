import assert from 'node:assert/strict';
import test from 'node:test';
import { cropRect, panPosition } from '../assets/wallpaper-upload/crop.js';
import { readFile } from 'node:fs/promises';
import { loadStoragePath } from '../assets/wallpaper-upload/storage.js';

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
