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
if (process.env.LUMA_SMOKE_SKIP_BUILD !== '1')
  run('cargo', ['build', '--locked', '--package', 'luma']);
const binary = path.resolve(root, process.env.CARGO_TARGET_DIR ?? 'target', 'debug/luma');
const temporary = await fs.mkdtemp(path.join(out, 'smoke-'));
try {
  const preferences = path.join(temporary, 'preferences.json');
  const scenario = async (name, script, extra = [], frames = 90) => {
    const screenshot = path.join(out, `${name}.png`);
    const dump = path.join(temporary, `${name}.json`);
    const diagnostics = path.join(out, `${name}-render.json`);
    run(binary, [
      '--preferences',
      preferences,
      '--script',
      script,
      '--frames',
      String(frames),
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
    if (state.ui_text.includes('No recent apps')) {
      // The intentionally sparse empty switcher has no bright header/footer
      // controls. Verify its message pixels instead of full-screen variance.
      const messagePixels = await sharp(screenshot)
        .extract({ left: 360, top: 526, width: 1200, height: 60 })
        .toBuffer();
      const message = await sharp(messagePixels).stats();
      assert.ok(
        message.channels.slice(0, 3).some((channel) => channel.max > 200 && channel.stdev > 10),
      );
    } else {
      assert.ok(
        stats.channels.slice(0, 3).some((channel) => channel.stdev > 10),
        'Framebuffer must contain rendered UI',
      );
    }
    return state;
  };
  const home = await scenario('home', 'down');
  assert.deepEqual(home.focus, { App: 'netflix' });
  const recents = await scenario('recents', 'recents,await-recents');
  assert.equal(recents.modal, true);
  assert.deepEqual(recents.focus, { Recent: 'netflix' });
  assert.equal(recents.recents.length, 8);
  assert.equal(recents.recents_error, null);
  assert.equal(recents.recents_visibility, 1);
  assert.ok(!recents.ui_text.includes('Recent apps'));
  assert.ok(!recents.ui_text.includes('Pick up where you left off.'));
  assert.ok(!recents.ui_text.includes('Desktop preview · Sample history'));
  assert.ok(!recents.ui_text.includes('Netflix'));
  assert.ok(!recents.ui_text.includes('YouTube'));
  assert.ok(!recents.ui_text.includes('1 / 8'));
  assert.ok(!recents.ui_text.includes('Close all'));
  assert.ok(
    !recents.ui_text.some((value) => value.includes('Switch') && value.includes('Close app')),
  );
  const recentClosed = await scenario(
    'recent-closed',
    'recents,await-recents,right,right,up,await-recents',
  );
  assert.equal(recentClosed.recents.length, 7);
  assert.ok(!recentClosed.recents.some((entry) => entry.app.id === 'com.apple.appletv'));
  assert.deepEqual(recentClosed.focus, { Recent: 'com.disney.disneyplus-prod' });
  const recentsEmpty = await scenario(
    'recents-empty',
    `recents,await-recents,${Array(8).fill('up,await-recents').join(',')}`,
    [],
    // Let the final card finish its upward exit before capturing the empty state.
    150,
  );
  assert.deepEqual(recentsEmpty.recents, []);
  assert.equal(recentsEmpty.focus, 'RecentEmpty');
  assert.ok(recentsEmpty.ui_text.includes('No recent apps'));
  assert.ok(!recentsEmpty.ui_text.includes('Apps you open will appear here.'));
  const recentBack = await scenario('recents-back', 'settings,recents,await-recents,back');
  assert.equal(recentBack.modal, false);
  assert.equal(recentBack.page, 'Settings');
  assert.equal(recentBack.recents_visibility, 0);
  const recentLaunch = await scenario(
    'recents-launch',
    'recents,await-recents,right,enter,await-recents',
  );
  assert.equal(recentLaunch.modal, false);
  const recentPointerLaunch = await scenario(
    'recents-pointer-launch',
    'recents,await-recents,click:960:527,await-recents',
  );
  assert.equal(recentPointerLaunch.modal, false);
  assert.equal(recentPointerLaunch.recents_visibility, 0);
  const recentHomeBack = await scenario('recents-home-back', 'home,down,back,await-recents');
  assert.equal(recentHomeBack.modal, true);
  assert.deepEqual(recentHomeBack.focus, { Recent: 'netflix' });
  const recentYoutube = await scenario('recents-youtube', 'recents,await-recents,right');
  assert.deepEqual(recentYoutube.focus, { Recent: 'youtube.leanback.v4' });
  const appOptions = await scenario('app-options', 'apps,down,enter');
  assert.deepEqual(appOptions.focus, 'OpenApp');
  assert.ok(appOptions.ui_text.includes('Delete app'));
  const deleteFocused = await scenario('delete-focused', 'apps,down,enter,down,down,down');
  assert.deepEqual(deleteFocused.focus, { Option: 2 });
  const deleteConfirm = await scenario('delete-confirm', 'apps,down,enter,down,down,down,enter');
  assert.deepEqual(deleteConfirm.focus, 'CancelDelete');
  assert.ok(deleteConfirm.ui_text.some((value) => value.includes('uninstall “Netflix”')));
  const deleteCancel = await scenario(
    'delete-cancel',
    'apps,down,enter,down,down,down,enter,enter',
  );
  assert.deepEqual(deleteCancel.focus, { Option: 2 });
  assert.ok(deleteCancel.apps.includes('netflix'));
  const hidden = await scenario('apps', 'apps,down,enter,down,down,enter');
  assert.ok(hidden.preferences.hidden.includes('netflix'));
  assert.deepEqual(hidden.focus, { App: 'youtube.leanback.v4' });
  // Reload from disk, find Netflix in Hidden, and restore it.
  const restored = await scenario(
    'restored',
    'apps,down,up,right,right,right,right,enter,down,enter,down,down,enter',
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
  const settings = await scenario('settings', 'settings,down,down,enter,down,enter,enter');
  assert.equal(settings.preferences.background, 'forest');
  const saved = await scenario('saved', 'down,hold,right,back');
  assert.equal(saved.preferences.background, 'forest');
  assert.deepEqual(saved.preferences.favorites.slice(0, 2), ['youtube.leanback.v4', 'netflix']);
  const upload = await scenario(
    'wallpaper-upload',
    'settings,down,down,enter,up,up,up,enter,await-upload',
  );
  assert.equal(upload.modal, true);
  assert.equal(upload.focus, 'CloseDialog');
  assert.ok(upload.ui_text.includes('Upload'));
  assert.ok(upload.ui_text.includes('Scan this QR code'));
  assert.ok(upload.ui_text.includes('or enter this URL in the browser'));
  assert.ok(upload.ui_text.some((text) => text.startsWith('Keep this window open')));
  assert.ok(
    upload.ui_text.some((text) =>
      /^http:\/\/127\.0\.0\.1:\d+\/[1-9A-HJ-NP-Za-km-z]{8}$/.test(text),
    ),
  );
  const uploadClosed = await scenario(
    'wallpaper-upload-closed',
    'settings,down,down,enter,up,up,up,enter,await-upload,back',
  );
  assert.equal(uploadClosed.modal, false);
  assert.equal(uploadClosed.focus, 'UploadWallpaper');
  const customId = `custom-${'1'.repeat(32)}`;
  const customDirectory = path.join(temporary, 'wallpapers', customId);
  await fs.mkdir(customDirectory, { recursive: true });
  await sharp({ create: { width: 1920, height: 1080, channels: 3, background: '#263832' } })
    .jpeg()
    .toFile(path.join(customDirectory, 'wallpaper.jpg'));
  await sharp({ create: { width: 480, height: 270, channels: 3, background: '#749788' } })
    .jpeg()
    .toFile(path.join(customDirectory, 'thumbnail.jpg'));
  await fs.writeFile(path.join(customDirectory, 'metadata.json'), JSON.stringify({ id: customId }));
  const custom = await scenario('wallpaper-custom', 'settings,down,down,enter,down,enter,enter');
  assert.equal(custom.preferences.background, customId);
  assert.equal(custom.modal, false);
  assert.deepEqual(custom.focus, { Background: 9 });
  const customRestored = await scenario('wallpaper-custom-restored', 'home,down');
  assert.equal(customRestored.preferences.background, customId);
  const bundledRestored = await scenario(
    'wallpaper-bundled-restored',
    'settings,down,down,enter,up,enter,enter',
  );
  assert.equal(bundledRestored.preferences.background, 'forest');
  const clock = await scenario('clock', 'settings,click:900:284');
  assert.equal(clock.subpage, 'Clock');
  assert.equal(clock.focus, 'ClockFormat');
  assert.ok(clock.ui_text.includes('Clock format'));
  assert.ok(clock.ui_text.includes('Time format'));
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
  const greetingEditor = 'settings,down,enter,down,down,down,down,enter,enter';
  for (const [id, name] of [
    ['Ana', 'Ana'],
    ['Bob', 'Bob'],
    ['long', 'Alexandra Catherine Elizabeth Montgomery'],
    ['wide', 'W'.repeat(40)],
    ['cjk', '小明'.repeat(20)],
    ['korean', '한글'.repeat(20)],
  ]) {
    const named = await scenario(
      `greeting-${id}`,
      `${greetingEditor},type:${name},back,home`,
      fixedTime,
    );
    assert.equal(named.preferences.greeting_name, name);
    assert.ok(named.ui_text.includes(`Good evening, ${name}.`));
    if (name.length > 30) {
      assert.ok(
        named.render.capture.max_logical_font_px < 90.4,
        'Long greetings must shrink instead of wrapping into a cropped second line',
      );
    }
    assert.equal(
      JSON.parse(await fs.readFile(preferences, 'utf8')).greeting_name,
      name,
      'Keyboard completion must save without waiting for another SDL input event',
    );
  }
  const greeting = await scenario(
    'greeting-settings',
    'settings,down,enter,down,down,down,down,enter,down',
  );
  assert.equal(greeting.subpage, 'Greeting');
  assert.equal(greeting.focus, 'GreetingNameClear');
  assert.ok(greeting.ui_text.includes('Clear name'));

  const appNamesEditor = 'settings,down,enter,enter';
  const appNames = await scenario('app-name-settings', appNamesEditor);
  assert.equal(appNames.subpage, 'AppNames');
  assert.ok(appNames.ui_text.includes('Show on Home'));
  assert.ok(appNames.ui_text.includes('Show on Apps'));
  assert.equal(appNames.preferences.show_home_names, false);
  assert.equal(appNames.preferences.show_app_names, true);
  const customNames = await scenario(
    'home-names-enabled',
    `${appNamesEditor},enter,down,enter,back,home`,
  );
  assert.equal(customNames.preferences.show_home_names, true);
  assert.equal(customNames.preferences.show_app_names, false);
  assert.ok(customNames.ui_text.includes('Netflix'));
  const hiddenNames = await scenario('app-names-disabled', 'apps');
  assert.equal(hiddenNames.preferences.show_app_names, false);
  assert.ok(!hiddenNames.ui_text.includes('Netflix'));

  const renamedInput = await scenario(
    'input-rename-focus',
    'settings,down,down,down,down,enter,down,enter,down,type:Console,back',
  );
  assert.equal(renamedInput.focus, 'InputRename');
  assert.equal(renamedInput.preferences.input_names['com.webos.app.hdmi2'], 'Console');
  assert.ok(renamedInput.ui_text.includes('Clear name'));
  const clockEditor = 'settings,down,enter,down,enter';
  const timeFormat = await scenario('clock-time-format', `${clockEditor},down,enter`);
  assert.equal(timeFormat.subpage, 'TimeFormat');
  assert.deepEqual(timeFormat.focus, { Choice: 0 });
  assert.ok(timeFormat.ui_text.includes('HH:MM'));
  assert.ok(timeFormat.ui_text.includes('HH:MM:SS'));
  const withSeconds = await scenario(
    'clock-time-format-saved',
    `${clockEditor},down,enter,down,enter`,
  );
  assert.equal(withSeconds.subpage, 'Clock');
  assert.equal(withSeconds.focus, 'ClockTimeFormat');
  assert.equal(withSeconds.preferences.show_seconds, true);
  assert.equal(withSeconds.preferences.hour_24, false);
  const clockFocus = await scenario('clock-format-focus', `${clockEditor},enter,down`);
  assert.equal(clockFocus.subpage, 'ClockFormat');
  assert.deepEqual(clockFocus.focus, { Choice: 1 });
  const system = await scenario(
    'system-settings',
    'settings,down,down,down,down,down,right,down,down',
  );
  assert.deepEqual(system.focus, { Setting: 5 });
  assert.ok(system.ui_text.includes('Refresh apps & inputs'));
  assert.ok(system.ui_text.includes('Quit app'));
  assert.ok(system.ui_text.includes('Text rendering'));
  const textOptions = 'settings,down,down,down,down,down,right,down,enter';
  const textRendering = await scenario('text-rendering-options', textOptions);
  assert.equal(textRendering.subpage, 'TextRendering');
  assert.deepEqual(textRendering.focus, { Choice: 0 });
  assert.ok(textRendering.ui_text.includes('MSAA (default)'));
  assert.ok(textRendering.ui_text.includes('Analytic'));
  assert.ok(textRendering.ui_text.includes('Restart Luma to apply changes.'));
  const analyticSaved = await scenario('text-rendering-saved', `${textOptions},down,enter`);
  assert.equal(analyticSaved.preferences.text_rendering, 'analytic');
  assert.deepEqual(analyticSaved.focus, { Setting: 7 });
  assert.equal(analyticSaved.render.text_rendering.mode, 'msaa', 'Mode changes apply next launch');
  const analytic = await scenario('text-rendering-analytic', 'home,down');
  assert.equal(analytic.render.text_rendering.mode, 'analytic');
  assert.equal(analytic.render.text_rendering.embolden, 0.25);
  assert.equal(analytic.render.last_presented.multisample_samples, 0);
  const msaaSaved = await scenario('text-rendering-msaa-saved', `${textOptions},up,enter`);
  assert.equal(msaaSaved.preferences.text_rendering, 'msaa');
  const msaa = await scenario('text-rendering-msaa', 'home,down');
  assert.equal(msaa.render.text_rendering.mode, 'msaa');
  assert.equal(msaa.render.text_rendering.embolden, 0);
  // The exposed card fragments are real pointer targets, including after remote scrolling.
  const edgeFavorites = [
    'netflix',
    'youtube.leanback.v4',
    'com.apple.appletv',
    'com.disney.disneyplus-prod',
    'amazon',
    'com.arte.tv',
    'de.zdf.app.zdfm3',
    'ard.mediathek',
    'plex',
  ];
  await fs.writeFile(
    preferences,
    JSON.stringify({ favorites: edgeFavorites, show_weather: false }),
  );
  const edgeRight = await scenario('favorites-edge-right', 'home,click:1860:600');
  assert.deepEqual(edgeRight.focus, { App: edgeFavorites[6] });
  assert.equal(edgeRight.home_scroll, 0, 'Clicking the fragment must not move it');
  const edgeLeft = await scenario(
    'favorites-edge-left',
    `home,down,${Array(6).fill('right').join(',')},click:40:600`,
  );
  assert.deepEqual(edgeLeft.focus, { App: edgeFavorites[0] });
  assert.ok(
    Math.abs(edgeLeft.home_scroll - 281.6) < 0.01,
    'Pointer focus preserves strip position',
  );
  await fs.writeFile(preferences, JSON.stringify({ show_weather: false }));
  const cycleEnabled = await scenario(
    'wallpaper-cycle-enabled',
    'settings,down,down,enter,up,up,left,enter',
  );
  assert.equal(cycleEnabled.subpage, null);
  assert.equal(cycleEnabled.focus, 'WallpaperCycle');
  assert.equal(cycleEnabled.preferences.wallpaper_cycle, true);
  assert.ok(cycleEnabled.ui_text.includes('Rotation'));
  const cycledAfterRestart = await scenario('wallpaper-cycle-restart', 'down');
  assert.equal(
    cycledAfterRestart.preferences.background,
    'san-francisco',
    'enabled rotation advances to the next wallpaper when Luma restarts',
  );
  const cycleDisabled = await scenario(
    'wallpaper-cycle-disabled',
    'settings,down,down,enter,up,up,left,enter',
  );
  assert.equal(cycleDisabled.preferences.wallpaper_cycle, false);
  assert.ok(cycleDisabled.ui_text.includes('Rotation'));
  console.log(
    'Native smoke checks passed: rendering/alpha, navigation, persistence, SDL pointer input, DPI-aware glyphs, 4K capture and presentation profiling.',
  );
} finally {
  await fs.rm(temporary, { recursive: true, force: true });
}
