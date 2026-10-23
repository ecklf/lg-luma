import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import sharp from 'sharp';

const root = path.resolve(import.meta.dirname, '..');
const out = path.join(root, '.build');
await fs.mkdir(out, { recursive: true });
const run = (command, args) => {
  const result = spawnSync(command, args, { cwd: root, stdio: 'inherit', timeout: 120000 });
  if (result.error) throw result.error;
  assert.equal(result.status, 0, `${command} failed`);
};
run('cargo', ['build', '--locked', '--package', 'luma']);
const binary = path.resolve(root, process.env.CARGO_TARGET_DIR ?? 'target', 'debug/luma');
const temporary = await fs.mkdtemp(path.join(out, 'smoke-'));
try {
  const preferences = path.join(temporary, 'preferences.json');
  const scenario = async (name, script, extra = []) => {
    const screenshot = path.join(out, `${name}.png`);
    const dump = path.join(temporary, `${name}.json`);
    const diagnostics = path.join(out, `${name}-render.json`);
    run(binary, [
      '--preferences',
      preferences,
      '--script',
      script,
      '--frames',
      '90',
      '--screenshot',
      screenshot,
      '--dump-state',
      dump,
      '--diagnostics',
      diagnostics,
      ...extra,
    ]);
    const state = JSON.parse(await fs.readFile(dump, 'utf8'));
    const render = JSON.parse(await fs.readFile(diagnostics, 'utf8'));
    assert.equal(
      render.capture.glyph_failures,
      0,
      'Visible vector text must remain valid across cache reuse',
    );
    assert.equal(render.last_presented.glyph_failures, 0);
    assert.equal(render.capture.font_antialiasing, 'vector grayscale coverage');
    assert.equal(render.capture.missing_glyphs, 0);
    assert.equal(render.capture.missing_images, 0);
    state.render = render;
    assert.equal(state.error, null);
    const stats = await sharp(screenshot).stats();
    assert.equal(
      stats.channels[3].min,
      255,
      'UI compositing must preserve opaque framebuffer alpha',
    );
    assert.ok(
      stats.channels.slice(0, 3).some((channel) => channel.stdev > 10),
      'Framebuffer must contain rendered UI',
    );
    return state;
  };
  const home = await scenario('home', 'down');
  assert.deepEqual(home.focus, { App: 'netflix' });
  const hidden = await scenario('apps', 'apps,down,menu,down,enter');
  assert.ok(hidden.preferences.hidden.includes('netflix'));
  assert.deepEqual(hidden.focus, { App: 'youtube.leanback.v4' });
  // Reload from disk, find Netflix in Hidden, and restore it.
  const restored = await scenario(
    'restored',
    'apps,down,up,right,right,right,right,enter,down,menu,down,enter',
  );
  assert.deepEqual(restored.preferences.hidden, []);
  const searched = await scenario('search', 'apps,type:YouTube,back');
  assert.equal(searched.query, 'YouTube');
  assert.deepEqual(searched.apps, ['youtube.leanback.v4']);
  for (const [id, title] of [
    ['coupangplay', 'Coupang Play'],
    ['com.frograms.watchaplay.webos', 'Watcha Play'],
    ['pooq', 'Wavve'],
    ['com.cmb.rainbowtv', 'Rainbow TV'],
    ['com.lgtvod.kr.app', 'LG TV VOD'],
  ]) {
    const koreanApp = await scenario(`korean-${id}`, `apps,type:${title},back`);
    assert.deepEqual(koreanApp.apps, [id]);
    assert.ok(koreanApp.ui_text.includes(title));
  }
  const settings = await scenario('settings', 'settings,down,down,enter,down,enter');
  assert.equal(settings.preferences.background, 'forest');
  const saved = await scenario('saved', 'down,hold,right,back');
  assert.equal(saved.preferences.background, 'forest');
  assert.deepEqual(saved.preferences.favorites.slice(0, 2), ['youtube.leanback.v4', 'netflix']);
  const clock = await scenario('clock', 'settings,click:900:284');
  assert.equal(clock.subpage, 'Clock');
  const input = await scenario('inputs', 'settings,down,down,down,down,enter,enter,up,enter');
  assert.equal(input.preferences.input_modes['com.webos.app.hdmi1'], 'Off');
  assert.equal(input.subpage, null);
  const closed = await scenario('dialog-closed', 'apps,down,menu,click:100:900');
  assert.equal(closed.modal, false);
  assert.deepEqual(closed.focus, { App: 'youtube.leanback.v4' });
  const fixedTime = ['--time', '2026-10-21T20:41:00+02:00'];
  const one = await scenario('density-1x', 'home,down', fixedTime);
  const two = await scenario('density-2x', 'home,down', [...fixedTime, '--capture-scale', '2']);
  assert.deepEqual(two.render.capture.target, { width: 3840, height: 2160 });
  assert.equal(two.render.capture.viewport.scale, 2);
  assert.equal(one.render.capture.viewport.scale, 1);
  assert.equal(two.render.capture.max_logical_font_px, one.render.capture.max_logical_font_px);
  for (const capture of [one.render.capture, two.render.capture]) {
    assert.ok(
      Math.abs(
        capture.max_physical_font_px - capture.max_logical_font_px * capture.viewport.scale,
      ) <= 0.126,
    );
  }
  assert.ok(
    two.render.capture.max_physical_font_px > one.render.capture.max_physical_font_px * 1.9,
  );
  assert.ok(two.render.capture.cached_glyphs > 0);
  assert.ok(two.render.capture.cached_text_runs > 0);
  assert.ok(two.render.capture.text_geometry_bytes > 0);
  assert.ok(two.render.capture.text_triangles > 0);
  assert.equal(
    two.render.drawable_is_4k,
    two.render.drawable.width >= 3840 && two.render.drawable.height >= 2160,
  );
  const uhd = await sharp(path.join(out, 'density-2x.png')).metadata();
  assert.equal(uhd.width, 3840);
  assert.equal(uhd.height, 2160);
  const low = await sharp(path.join(out, 'density-1x.png'))
    .extract({ left: 84, top: 265, width: 600, height: 96 })
    .resize(1200, 192, { kernel: 'cubic' })
    .raw()
    .toBuffer();
  const high = await sharp(path.join(out, 'density-2x.png'))
    .extract({ left: 168, top: 530, width: 1200, height: 192 })
    .raw()
    .toBuffer();
  assert.notDeepEqual(high, low, '4K vector text must use the drawable pixel scale');
  const hasWallpaper = await fs.access(path.join(out, 'assets/wallpapers/forest@2x.jpg')).then(
    () => true,
    () => false,
  );
  assert.equal(
    two.render.capture.high_resolution_wallpaper,
    hasWallpaper && two.render.graphics.max_texture_size >= 3840,
  );
  const profiled = await scenario('profile', 'home,down', ['--profile-frames', '24', ...fixedTime]);
  assert.equal(profiled.render.profile.presented, 24);
  assert.ok(profiled.render.profile.pacing.samples > 0);
  assert.ok(profiled.render.profile.cpu_submission.mean_ms >= 0);
  console.log(
    'Native smoke checks passed: rendering/alpha, navigation, persistence, SDL pointer input, DPI-aware glyphs, 4K capture and presentation profiling.',
  );
} finally {
  await fs.rm(temporary, { recursive: true, force: true });
}
