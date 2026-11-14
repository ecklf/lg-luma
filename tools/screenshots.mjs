// Capture documentation from the actual native desktop renderer, not a browser.
import fs from 'node:fs/promises';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import sharp from 'sharp';

const root = path.resolve(import.meta.dirname, '..');
const docs = path.join(root, 'docs');
const scratch = path.join(root, '.build');
const widgetsOnly = process.argv.includes('--widgets');
await fs.mkdir(docs, { recursive: true });
await fs.mkdir(scratch, { recursive: true });

function run(command, args) {
  const result = spawnSync(command, args, { cwd: root, stdio: 'inherit', timeout: 120000 });
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(`${command} exited with status ${result.status}`);
}

// Force the desktop feature set even after a previous all-features lint/build.
run('cargo', ['build', '--locked', '--package', 'luma', '--no-default-features']);
const binary = path.resolve(root, process.env.CARGO_TARGET_DIR ?? 'target', 'debug/luma');
const temporary = await fs.mkdtemp(path.join(scratch, 'documentation-'));
const preferences = path.join(temporary, 'preferences.json');
const configuration = {
  language: 'en',
  background: 'dunes',
  hour_24: false,
  show_home_names: false,
  show_weather: true,
  weather_location: {
    name: 'Vienna',
    region: 'Vienna',
    country: 'Austria',
    latitude: 48.2085,
    longitude: 16.3721,
  },
  hidden: [],
  input_modes: {},
  reduced_motion: false,
};
const widgets = {
  'home-layout-row': {
    home_layout: 'row',
    home_widgets: ['weather', 'clock', 'date', 'empty', 'empty', 'empty'],
  },
  'home-layout-centered': {
    home_layout: 'centered',
    home_widgets: ['clock', 'empty', 'empty', 'empty', 'empty', 'empty'],
  },
  'home-layout-grid': {
    home_layout: 'grid',
    home_widgets: ['clock', 'date', 'greeting', 'weather', 'empty', 'empty'],
  },
  'home-layout-settings': {
    home_layout: 'grid',
    home_widgets: ['clock', 'date', 'greeting', 'weather', 'empty', 'empty'],
    home_clock_height: 250,
    home_clock_glass: true,
  },
  'home-clock-depth': {
    background: 'summit',
    home_layout: 'centered',
    home_widgets: ['clock', 'empty', 'empty', 'empty', 'empty', 'empty'],
  },
  'home-clock-glass': {
    background: 'summit',
    home_layout: 'centered',
    home_widgets: ['clock', 'empty', 'empty', 'empty', 'empty', 'empty'],
    home_clock_height: 250,
    home_clock_glass: true,
    home_clock_depth: false,
  },
  'home-clock-glass-depth': {
    background: 'summit',
    home_layout: 'centered',
    home_widgets: ['clock', 'empty', 'empty', 'empty', 'empty', 'empty'],
    home_clock_height: 250,
    home_clock_glass: true,
    home_clock_depth: true,
  },
};
try {
  for (const [name, script] of [
    ['home', 'home'],
    ['weather-setup', 'home,up,down'],
    ['apps', 'apps,down'],
    ['settings', 'settings'],
    ['update-card', 'settings,down,down,down,down,right'],
    ['inputs', 'home,up'],
    ['clock-menu', 'home,up,right,right,enter'],
    ['manage-apps', 'settings,down,enter'],
    ['app-actions', 'settings,down,enter,enter'],
    ...Object.keys(widgets).map((name) => [
      name,
      name === 'home-layout-settings'
        ? `settings,down,down,down,enter,${Array(12).fill('down').join(',')}`
        : 'home',
    ]),
  ].filter(([name]) => !widgetsOnly || widgets[name])) {
    // Independent state makes screenshots stable regardless of capture order.
    await fs.writeFile(
      preferences,
      JSON.stringify(
        name === 'weather-setup'
          ? { ...configuration, weather_location: null }
          : {
              ...configuration,
              ...(widgets[name] ? { hour_24: true, home_clock_depth: true, ...widgets[name] } : {}),
            },
      ),
    );
    await fs.writeFile(
      path.join(temporary, 'preferences.weather.json'),
      JSON.stringify({
        key: '48.2085,16.3721',
        updated_at: Math.floor(Date.now() / 1000),
        reading: { temperature: 14.2, code: 2, is_day: true },
      }),
    );
    run(binary, [
      '--assets',
      path.join(scratch, 'assets'),
      '--preferences',
      preferences,
      '--weather-test-server',
      'http://127.0.0.1:1',
      '--time',
      '2026-10-22T20:41:00+02:00',
      '--script',
      script,
      ...(name === 'update-card' ? ['--test-update-version', '99.0.0'] : []),
      '--frames',
      '90',
      '--capture-scale',
      '1',
      '--screenshot',
      path.join(docs, `${name}.png`),
    ]);
    console.log(`Native screenshot: docs/${name}.png (1920×1080)`);
  }
  await sharp(path.join(docs, 'home-clock-depth.png'))
    .extract({ left: 640, top: 280, width: 640, height: 320 })
    .png()
    .toFile(path.join(docs, 'home-clock-depth-detail.png'));
} finally {
  await fs.rm(temporary, { recursive: true, force: true });
}
