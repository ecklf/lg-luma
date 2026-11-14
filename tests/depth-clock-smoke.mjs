import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import sharp from 'sharp';

const root = path.resolve(import.meta.dirname, '..');
const run = (command, args) => {
  const result = spawnSync(command, args, { cwd: root, stdio: 'inherit', timeout: 120000 });
  assert.equal(result.status, 0, result.error?.message ?? `${command} failed`);
};
if (process.env.LUMA_SMOKE_SKIP_BUILD !== '1')
  run('cargo', ['build', '--locked', '--package', 'luma', '--no-default-features']);
const assets = path.join(root, '.build/assets');
const temporary = await fs.mkdtemp(path.join(root, '.build/depth-smoke-'));
const preferences = path.join(temporary, 'preferences.json');
const fake = path.join(temporary, 'unoccluded-assets');
try {
  await fs.mkdir(fake);
  for (const file of await fs.readdir(assets)) {
    if (file !== 'foregrounds') await fs.symlink(path.join(assets, file), path.join(fake, file));
  }
  await fs.mkdir(path.join(fake, 'foregrounds'));
  await fs.copyFile(
    path.join(assets, 'foregrounds/manifest.json'),
    path.join(fake, 'foregrounds/manifest.json'),
  );
  for (const suffix of ['', '-plain']) {
    await sharp({
      create: {
        width: 1920,
        height: 1080,
        channels: 4,
        background: { r: 0, g: 0, b: 0, alpha: 0 },
      },
    })
      .png()
      .toFile(path.join(fake, `foregrounds/summit${suffix}.png`));
  }
  const capture = async (name, folder, clock = true, gradient = true, depth = true) => {
    await fs.writeFile(
      preferences,
      JSON.stringify({
        background: 'summit',
        home_layout: 'centered',
        home_widgets: [clock ? 'clock' : 'empty', 'empty', 'empty', 'empty', 'empty', 'empty'],
        home_clock_depth: depth,
        wallpaper_gradient: gradient,
        show_weather: false,
        hour_24: true,
        reduced_motion: true,
      }),
    );
    const image = path.join(root, '.build', `depth-clock-${name}.png`);
    const diagnostics = path.join(root, '.build', `depth-clock-${name}-render.json`);
    run(path.join(root, process.env.CARGO_TARGET_DIR ?? 'target', 'debug/luma'), [
      '--assets',
      folder,
      '--preferences',
      preferences,
      '--script',
      'home',
      '--time',
      '2026-10-22T20:41:00+02:00',
      '--frames',
      '12',
      '--capture-scale',
      '1',
      '--screenshot',
      image,
      '--diagnostics',
      diagnostics,
    ]);
    const render = JSON.parse(await fs.readFile(diagnostics, 'utf8'));
    assert.equal(render.capture.missing_images, 0);
    assert.equal(render.capture.missing_glyphs, 0);
    assert.equal(render.capture.glyph_failures, 0);
    return sharp(image).ensureAlpha().raw().toBuffer();
  };
  for (const gradient of [true, false]) {
    const base = await capture(`base-${gradient}`, assets, false, gradient);
    const clear = await capture(`unoccluded-${gradient}`, fake, true, gradient);
    const depth = await capture(`depth-${gradient}`, assets, true, gradient);
    const mask = await sharp(path.join(assets, `foregrounds/summit${gradient ? '' : '-plain'}.png`))
      .ensureAlpha()
      .raw()
      .toBuffer();
    let hidden = 0;
    let shown = 0;
    for (let y = 188; y < 772; y++) {
      for (let x = 676; x < 1244; x++) {
        const pixel = (y * 1920 + x) * 4;
        const difference = (a, b) =>
          Math.max(
            ...[0, 1, 2].map((channel) => Math.abs(a[pixel + channel] - b[pixel + channel])),
          );
        if (difference(clear, base) < 80) continue;
        if (mask[pixel + 3] > 254) {
          assert.ok(
            difference(depth, base) < 12,
            'Opaque foreground must hide the actual clock glyph',
          );
          hidden++;
        } else if (mask[pixel + 3] === 0) {
          assert.ok(
            difference(depth, clear) < 12,
            'Clear background must preserve the clock glyph',
          );
          shown++;
        }
      }
    }
    assert.ok(hidden > 100, `Expected visible glyph occlusion, got ${hidden}`);
    assert.ok(shown > 1000, `Clock must remain readable, got ${shown}`);
    assert.deepEqual(
      depth.subarray(796 * 1920 * 4),
      base.subarray(796 * 1920 * 4),
      'Foreground must not cover the favorites dock',
    );
    console.log(
      `Depth clock ${gradient ? 'shaded' : 'plain'}: ${hidden} glyph pixels occluded, ${shown} preserved.`,
    );
  }
  await capture('disabled', assets, true, true, false);
  console.log(
    'Native depth-clock checks passed: real glyph occlusion, colour matching, gradient toggle, readable clock and unobscured dock.',
  );
} finally {
  await fs.rm(temporary, { recursive: true, force: true });
}
