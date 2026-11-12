import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import sharp from 'sharp';

const root = path.resolve(import.meta.dirname, '..');
const out = path.join(root, '.build');
const run = (command, args) => {
  const result = spawnSync(command, args, { cwd: root, stdio: 'inherit', timeout: 120000 });
  if (result.error) throw result.error;
  assert.equal(result.status, 0, `${command} failed`);
};
if (process.env.LUMA_SMOKE_SKIP_BUILD !== '1')
  run('cargo', ['build', '--locked', '--package', 'luma']);
const binary = path.resolve(root, process.env.CARGO_TARGET_DIR ?? 'target', 'debug/luma');
await fs.mkdir(out, { recursive: true });
const temporary = await fs.mkdtemp(path.join(out, 'home-layout-smoke-'));
const preferences = path.join(temporary, 'preferences.json');
const edit = 'settings,down,down,down,enter,enter';
try {
  const scenario = async (name, script) => {
    const dump = path.join(temporary, `${name}.json`);
    const diagnostics = path.join(out, `${name}-render.json`);
    const screenshot = path.join(out, `${name}.png`);
    run(binary, [
      '--preferences',
      preferences,
      '--script',
      script,
      '--frames',
      '100',
      '--time',
      '2026-10-21T20:41:00+02:00',
      '--screenshot',
      screenshot,
      '--dump-state',
      dump,
      '--diagnostics',
      diagnostics,
    ]);
    const state = JSON.parse(await fs.readFile(dump, 'utf8'));
    const render = JSON.parse(await fs.readFile(diagnostics, 'utf8'));
    assert.equal(render.capture.glyph_failures, 0);
    assert.equal(render.capture.missing_glyphs, 0);
    assert.equal(render.capture.missing_images, 0);
    assert.equal(state.error, null);
    const stats = await sharp(screenshot).stats();
    assert.ok(stats.channels.slice(0, 3).some((channel) => channel.stdev > 10));
    return state;
  };
  const editor = await scenario('layout-editor', edit);
  assert.deepEqual(editor.focus, { LayoutWidget: 1 });
  assert.ok(editor.ui_text.includes('Save layout'));
  assert.equal(editor.preferences.home_layout, null);
  const classic = await scenario(
    'layout-classic-remote-save',
    `${edit},down,down,down,down,down,down,right,right,right,enter,home`,
  );
  assert.equal(classic.preferences.home_layout.widgets.length, 8);
  assert.equal(classic.page, 'Home');
  const split = await scenario(
    'layout-split',
    `${edit},click:600:1020,down,enter,click:1300:1020,home`,
  );
  const saved = JSON.parse(await fs.readFile(preferences, 'utf8'));
  assert.equal(saved.home_layout.version, 1);
  const favorites = saved.home_layout.widgets.find((widget) => widget.kind === 'favorites');
  assert.equal(favorites.width, 7);
  assert.equal(split.page, 'Home');
  const restored = await scenario('layout-restored', 'home');
  assert.deepEqual(restored.preferences.home_layout, saved.home_layout);
  const scrolled = await scenario(
    'layout-narrow-favorites',
    'home,click:220:600,right,right,right,right,right',
  );
  assert.ok(scrolled.home_scroll > 0);
  const inputs = await scenario('layout-narrow-inputs', 'home,click:1400:580,right');
  assert.ok(inputs.input_scroll > 0);
  await scenario(
    'layout-move-cancelled',
    `${edit},enter,enter,right,back,back,click:1660:1020,home`,
  );
  assert.deepEqual(
    JSON.parse(await fs.readFile(preferences, 'utf8')).home_layout,
    saved.home_layout,
  );
  await scenario(
    'layout-resized',
    `${edit},click:220:600,down,enter,left,enter,back,click:1300:1020,home`,
  );
  const resized = JSON.parse(await fs.readFile(preferences, 'utf8'));
  assert.equal(resized.home_layout.widgets.find((widget) => widget.kind === 'favorites').width, 6);
  const minimal = await scenario(
    'layout-minimal',
    `${edit},click:600:1020,down,down,enter,click:1300:1020,home`,
  );
  assert.equal(minimal.preferences.show_date, false);
  assert.equal(minimal.preferences.show_weather, false);
  for (const language of ['de', 'zh-tw', 'vi']) {
    await fs.writeFile(preferences, JSON.stringify({ ...saved, language }));
    const localized = await scenario(`layout-editor-${language}`, `${edit},enter,down,down,enter`);
    assert.deepEqual(localized.focus, { LayoutAction: 0 });
  }
  await fs.writeFile(
    preferences,
    JSON.stringify({ ...saved, home_layout: { version: 1, widgets: [] } }),
  );
  const empty = await scenario('layout-recovery', 'home,enter');
  assert.equal(empty.page, 'Settings');
  await fs.writeFile(
    preferences,
    JSON.stringify({ ...saved, home_layout: { version: 255, widgets: [] } }),
  );
  const recovered = await scenario('layout-invalid-fallback', 'home,down');
  assert.equal(recovered.preferences.home_layout, null);
  assert.ok(recovered.focus.App);
  console.log(
    'Home layout smoke passed: editor, save/reload, resize, cancel, narrow strips, localization, recovery.',
  );
} finally {
  await fs.rm(temporary, { recursive: true, force: true });
}
