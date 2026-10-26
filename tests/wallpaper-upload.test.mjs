import assert from 'node:assert/strict';
import test from 'node:test';
import { cleanName, cropRect, panPosition } from '../assets/wallpaper-upload/crop.js';

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

test('names retain Unicode, remove controls, and match the TV’s 40-character limit', () => {
  assert.equal(cleanName('  My\n view  '), 'My view');
  assert.equal(cleanName('旅行 & mer'), '旅行 & mer');
  assert.equal(Array.from(cleanName('🌄'.repeat(50))).length, 40);
  assert.equal(cleanName(' \n '), '');
});
