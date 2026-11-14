import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import sharp from 'sharp';

const root = path.resolve(import.meta.dirname, '..');
const out = path.join(root, '.build');
const binary = path.resolve(root, process.env.CARGO_TARGET_DIR ?? 'target', 'debug/luma');
function run(command, args, env = {}) {
  const result = spawnSync(command, args, {
    cwd: root,
    env: { ...process.env, ...env },
    encoding: 'utf8',
    timeout: 120000,
  });
  if (result.error) throw result.error;
  assert.equal(result.status, 0, result.stderr);
}
if (process.env.LUMA_SMOKE_SKIP_BUILD !== '1') {
  run('cargo', ['build', '--locked', '--package', 'luma', '--no-default-features']);
}
await fs.mkdir(out, { recursive: true });
const scratch = await fs.mkdtemp(path.join(out, 'startup-smoke-'));
try {
  const source = path.join(out, 'assets');
  const assets = path.join(scratch, 'assets');
  await fs.mkdir(assets);
  // Private sidecars let us test missing/corrupt old-installation data without changing shared assets.
  for (const file of await fs.readdir(source)) {
    if (file !== 'wallpapers') await fs.symlink(path.join(source, file), path.join(assets, file));
  }
  await fs.mkdir(path.join(assets, 'wallpapers'));
  await fs.copyFile(
    path.join(source, 'wallpapers/dunes.jpg'),
    path.join(assets, 'wallpapers/dunes.jpg'),
  );
  const atlas = path.join(assets, 'wallpapers/dunes.glass.png');
  const prepared = await fs.readFile(path.join(source, 'wallpapers/dunes.glass.png'));
  const preferences = path.join(scratch, 'preferences.json');
  await fs.writeFile(
    preferences,
    JSON.stringify({ show_weather: false, reduced_motion: true, background: 'dunes' }),
  );
  for (const gles of [false, true]) {
    let reference;
    for (const mode of ['prepared', 'runtime', 'missing', 'corrupt']) {
      if (mode === 'missing') await fs.rm(atlas);
      else await fs.writeFile(atlas, mode === 'corrupt' ? 'invalid PNG' : prepared);
      const name = `startup-${gles ? 'gles' : 'desktop'}-${mode}`;
      const report = path.join(out, `${name}-render.json`);
      const screenshot = path.join(out, `${name}.png`);
      run(
        binary,
        [
          '--assets',
          assets,
          '--preferences',
          preferences,
          '--script',
          'home',
          '--frames',
          '12',
          '--time',
          '2026-10-22T20:41:00+02:00',
          '--weather-test-server',
          'http://127.0.0.1:1',
          '--diagnostics',
          report,
          '--screenshot',
          screenshot,
        ],
        { LUMA_GLES: gles ? '1' : '0', LUMA_GLASS_PRECOMPUTED: mode === 'runtime' ? '0' : '1' },
      );
      const result = JSON.parse(await fs.readFile(report, 'utf8'));
      assert.equal(result.capture.missing_images, 0);
      assert.equal(result.capture.glyph_failures, 0);
      assert.equal(result.startup.first_frame.glass_precomputed_loads, mode === 'prepared' ? 1 : 0);
      assert.equal(result.startup.first_frame.glass_runtime_prepares, mode === 'prepared' ? 0 : 1);
      assert.ok(result.startup.stages_ms['first frame'] >= result.startup.stages_ms.renderer);
      assert.ok(
        result.startup.stages_ms['apps ready frame'] >= result.startup.stages_ms['app inventory'],
      );
      const pixels = await sharp(screenshot).raw().toBuffer();
      if (reference)
        assert.deepEqual(pixels, reference, `${name} must preserve every framebuffer pixel`);
      else reference = pixels;
    }
  }
  console.log(
    'Startup smoke checks passed: precomputed glass skips runtime filtering with identical desktop/GLES pixels and safe missing/corrupt fallback.',
  );
} finally {
  await fs.rm(scratch, { recursive: true, force: true });
}
