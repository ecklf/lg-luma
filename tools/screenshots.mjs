// Capture documentation from the actual native desktop renderer, not a browser.
import fs from 'node:fs/promises';
import path from 'node:path';
import { spawnSync } from 'node:child_process';

const root = path.resolve(import.meta.dirname, '..');
const docs = path.join(root, 'docs');
const scratch = path.join(root, '.build');
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
const sortingPreviews = process.argv.includes('--sorting');
const weatherPreviews = process.argv.includes('--weather-styles');
const captures = weatherPreviews
  ? [
      ['weather-style-standard', 'home'],
      ['weather-style-no-icon', 'home'],
      ['weather-style-compact', 'home'],
      ['weather-style-picker', 'settings,down,down,down,right,enter,down,down,down,enter'],
    ]
  : sortingPreviews
    ? [
        ['app-sorting-general', 'settings,right,down,down'],
        ['app-sorting-picker', 'settings,right,down,down,enter'],
        ['app-sorting-az', 'apps,down'],
        ['app-sorting-usage', 'apps,down'],
        ['disc-input', 'home'],
      ]
    : [
        ['home', 'home'],
        ['weather-setup', 'home,up,down'],
        ['apps', 'apps,down'],
        ['settings', 'settings'],
        ['update-card', 'settings,down,down,down,down,right'],
        ['inputs', 'home,up'],
        ['clock-menu', 'home,up,right,right,enter'],
        ['manage-apps', 'settings,down,enter'],
        ['app-actions', 'settings,down,enter,enter'],
      ];
try {
  for (const [name, script] of captures) {
    // Independent state makes screenshots stable regardless of capture order.
    await fs.writeFile(
      preferences,
      JSON.stringify(
        weatherPreviews
          ? {
              ...configuration,
              weather_style:
                name === 'weather-style-no-icon'
                  ? 'no_icon'
                  : name === 'weather-style-compact'
                    ? 'compact'
                    : 'standard',
            }
          : sortingPreviews
            ? {
                ...configuration,
                app_sorting: name === 'app-sorting-az' ? 'alphabetical' : 'usage',
                app_launch_counts: {
                  'org.jellyfin.webos': 42,
                  'cdp-30': 24,
                  'spotify-beehive': 18,
                },
                ...(name === 'disc-input'
                  ? { input_icons: { 'com.webos.app.hdmi1': 'disk' } }
                  : {}),
              }
            : name === 'weather-setup'
              ? { ...configuration, weather_location: null }
              : configuration,
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
} finally {
  await fs.rm(temporary, { recursive: true, force: true });
}
