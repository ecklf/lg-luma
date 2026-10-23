// Exercise every curated logo with the native desktop renderer. No TV, network
// service or weather API is used. The contact sheet is a manual design preview.
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import sharp from 'sharp';

const root = path.resolve(import.meta.dirname, '..');
const out = path.join(root, '.build');
await fs.mkdir(out, { recursive: true });
function run(command, args) {
  const result = spawnSync(command, args, { cwd: root, stdio: 'inherit', timeout: 120000 });
  if (result.error) throw result.error;
  assert.equal(result.status, 0, `${command} failed`);
}
run('cargo', ['build', '--locked', '--package', 'luma', '--no-default-features']);
const binary = path.resolve(root, process.env.CARGO_TARGET_DIR ?? 'target', 'debug/luma');
const scratch = await fs.mkdtemp(path.join(out, 'brand-gallery-'));
try {
  const preferences = path.join(scratch, 'preferences.json');
  const dump = path.join(scratch, 'state.json');
  const diagnostics = path.join(scratch, 'render.json');
  const screenshot = path.join(scratch, 'card.png');
  await fs.writeFile(preferences, JSON.stringify({ reduced_motion: true, show_weather: false }));
  run(binary, [
    '--preferences',
    preferences,
    '--script',
    'apps',
    '--frames',
    '18',
    '--dump-state',
    dump,
  ]);
  const { inventory } = JSON.parse(await fs.readFile(dump, 'utf8'));
  const logos = (await fs.readdir(path.join(root, 'assets/logos')))
    .filter((file) => file.endsWith('.svg'))
    .map((file) => path.parse(file).name)
    .sort();
  const cards = [];
  for (const [index, logo] of logos.entries()) {
    const app = inventory.find((app) => app.brand === logo);
    assert.ok(app, `Desktop demo is missing ${logo}`);
    run(binary, [
      '--preferences',
      preferences,
      '--script',
      `apps,type:${app.title},back`,
      '--frames',
      '18',
      '--screenshot',
      screenshot,
      '--dump-state',
      dump,
      '--diagnostics',
      diagnostics,
    ]);
    const state = JSON.parse(await fs.readFile(dump, 'utf8'));
    const report = JSON.parse(await fs.readFile(diagnostics, 'utf8'));
    assert.ok(state.apps.includes(app.id), `${logo} must be searchable in the demo`);
    assert.equal(report.capture.missing_images, 0, `Missing artwork for ${logo}`);
    assert.equal(report.capture.glyph_failures, 0, `Invalid caption for ${logo}`);
    assert.ok(report.capture.vector_meshes > 0);
    const input = await sharp(screenshot)
      .extract({ left: 78, top: 264, width: 282, height: 238 })
      .png()
      .toBuffer();
    cards.push({ input, left: (index % 6) * 282, top: Math.floor(index / 6) * 238 });
  }
  await sharp({
    create: {
      width: 6 * 282,
      height: Math.ceil(cards.length / 6) * 238,
      channels: 4,
      background: '#11151b',
    },
  })
    .composite(cards)
    .png()
    .toFile(path.join(out, 'brands-gallery.png'));
  console.log(
    `All ${logos.length} SVG-backed demo apps rendered. Contact sheet: .build/brands-gallery.png`,
  );
} finally {
  await fs.rm(scratch, { recursive: true, force: true });
}
