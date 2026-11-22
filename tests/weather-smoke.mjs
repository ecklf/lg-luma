import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import path from 'node:path';
import http from 'node:http';
import { spawn } from 'node:child_process';
import sharp from 'sharp';

const root = path.resolve(import.meta.dirname, '..');
const out = path.join(root, '.build');
await fs.mkdir(out, { recursive: true });
const temporary = await fs.mkdtemp(path.join(out, 'weather-smoke-'));
const preferences = path.join(temporary, 'preferences.json');
const cache = path.join(temporary, 'preferences.weather.json');
const binary = path.resolve(root, process.env.CARGO_TARGET_DIR ?? 'target', 'debug/luma');
const vienna = {
  name: 'Vienna',
  region: 'Vienna',
  country: 'Austria',
  latitude: 48.2085,
  longitude: 16.3721,
};
const paris = {
  name: 'Paris',
  region: 'Île-de-France',
  country: 'France',
  latitude: 48.8534,
  longitude: 2.3488,
};
const reading = { temperature: 14.2, code: 2, is_day: true };
const requests = [];
let forecastMode = 'ok';
const server = http.createServer((request, response) => {
  const url = new URL(request.url, 'http://127.0.0.1');
  requests.push({ path: url.pathname, query: Object.fromEntries(url.searchParams) });
  assert.equal(request.headers.cookie, undefined);
  assert.equal(request.headers.referer, undefined);
  response.on('error', () => {});
  const json = (value, status = 200) => {
    response.writeHead(status, { 'Content-Type': 'application/json' });
    response.end(JSON.stringify(value));
  };
  if (url.pathname === '/v1/search') {
    const query = url.searchParams.get('name');
    if (query === 'fail') return json({}, 503);
    if (query === 'nowhere') return json({ results: [] });
    if (query === 'malformed') return json({ results: 'invalid' });
    if (query === 'oversized') return json({ results: [], padding: 'x'.repeat(300000) });
    if (query === 'redirect') {
      response.writeHead(302, { Location: '/unexpected' });
      response.end();
      return;
    }
    const place = query === 'Paris' ? paris : vienna;
    const send = () =>
      json({
        results: [
          { ...place, admin1: place.region },
          { ...place, admin1: place.region },
          { name: 'Invalid', latitude: 123, longitude: 16 },
        ],
      });
    if (query === 'slow') {
      const timer = setTimeout(send, 700);
      response.on('close', () => clearTimeout(timer));
    } else send();
    return;
  }
  if (url.pathname === '/v1/forecast') {
    assert.equal(url.searchParams.get('temperature_unit'), 'celsius');
    assert.equal(url.searchParams.get('current'), 'temperature_2m,is_day,weather_code');
    if (forecastMode === 'fail') return json({}, 503);
    if (forecastMode === 'malformed')
      return json({ current: { temperature_2m: 500, weather_code: 2, is_day: 1 } });
    const send = () => json({ current: { temperature_2m: 14.2, weather_code: 2, is_day: 1 } });
    if (forecastMode === 'slow') {
      const timer = setTimeout(send, 700);
      response.on('close', () => clearTimeout(timer));
    } else send();
    return;
  }
  json({}, 404);
});
await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
const origin = `http://127.0.0.1:${server.address().port}`;

async function run(command, args) {
  await new Promise((resolve, reject) => {
    const child = spawn(command, args, { cwd: root, stdio: 'inherit' });
    const timeout = setTimeout(() => {
      child.kill('SIGKILL');
      reject(new Error('Weather smoke child timed out'));
    }, 30000);
    child.on('error', (error) => {
      clearTimeout(timeout);
      reject(error);
    });
    child.on('exit', (code) => {
      clearTimeout(timeout);
      if (code === 0) resolve();
      else reject(new Error(`${command} exited ${code}`));
    });
  });
}
async function writePreferences(patch = {}) {
  await fs.writeFile(
    preferences,
    JSON.stringify({
      background: 'midnight',
      reduced_motion: true,
      show_weather: true,
      weather_style: 'standard',
      weather_position: { column: 4, row: 0 },
      ...patch,
    }),
  );
}
async function writeCache(age = 0) {
  await fs.writeFile(
    cache,
    JSON.stringify({
      key: '48.2085,16.3721',
      updated_at: Math.floor(Date.now() / 1000) - age,
      reading,
    }),
  );
}
async function scenario(name, script, frames = 210) {
  const dump = path.join(temporary, `${name}.json`);
  const diagnostics = path.join(temporary, `${name}-render.json`);
  const screenshot = path.join(out, `weather-${name}.png`);
  await run(binary, [
    '--preferences',
    preferences,
    '--weather-test-server',
    origin,
    '--script',
    script,
    '--frames',
    String(frames),
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
  assert.equal(state.error, null);
  const rendered = JSON.parse(await fs.readFile(diagnostics, 'utf8'));
  assert.equal(rendered.capture.missing_images, 0, name);
  assert.equal(rendered.capture.missing_glyphs, 0, name);
  assert.equal((await sharp(screenshot).stats()).channels[3].min, 255);
  return state;
}
const forecastCount = () => requests.filter((request) => request.path === '/v1/forecast').length;
const searchCount = () => requests.filter((request) => request.path === '/v1/search').length;
const widgetsPage = 'settings,down,down,down,down,enter';
const weatherActions = `${widgetsPage},down,right,right,right,right,enter`;
const weatherPage = `${weatherActions},enter`;
const locationPage = `${weatherPage},enter`;
try {
  if (process.env.LUMA_SMOKE_SKIP_BUILD !== '1')
    await run('cargo', ['build', '--locked', '--package', 'luma']);
  await fs.writeFile(preferences, JSON.stringify({ reduced_motion: true }));
  const defaults = await scenario('unset-defaults', 'home');
  assert.equal(defaults.preferences.weather_style, 'one_line');
  assert.deepEqual(defaults.preferences.weather_position, { column: 0, row: 1 });
  await writePreferences();
  await scenario('unconfigured', 'home,down');
  assert.equal(requests.length, 0, 'No geolocation or automatic city discovery');
  const short = await scenario('short-query', `${locationPage},enter,back,enter`);
  assert.equal(short.subpage, 'WeatherLocation');
  assert.equal(short.focus, 'WeatherSearch');
  assert.equal(short.weather.searching, false);
  assert.ok(short.weather.message);
  assert.equal(searchCount(), 0);

  const chosen = await scenario(
    'choose-location',
    `${locationPage},type:1000 Wien & west/?,back,enter,await-search,down,enter,home,await-weather`,
  );
  assert.equal(chosen.preferences.weather_location.name, 'Vienna');
  assert.equal(chosen.weather.cache.reading.temperature, 14.2);
  assert.equal(chosen.weather.failed, false);
  assert.equal(searchCount(), 1);
  assert.equal(forecastCount(), 1);
  assert.equal(
    requests.find((request) => request.path === '/v1/search').query.name,
    '1000 Wien & west/?',
  );
  assert.equal(JSON.parse(await fs.readFile(cache, 'utf8')).reading.temperature, 14.2);

  await scenario('cache-reopen', 'home,await-weather');
  assert.equal(forecastCount(), 1);
  const hiddenLocation = await scenario(
    'hidden-location',
    `${weatherPage},down,down,enter,up,up,up,enter,home,await-weather`,
  );
  assert.equal(hiddenLocation.preferences.weather_show_location, false);
  assert.equal(hiddenLocation.preferences.weather_location.name, 'Vienna');
  assert.equal(hiddenLocation.weather.cache.reading.temperature, 14.2);
  assert.ok(!hiddenLocation.ui_text.some((label) => label.includes('Vienna')));
  assert.ok(hiddenLocation.ui_text.includes('14°C'));
  assert.equal(forecastCount(), 1, 'Hiding the city must not invalidate the forecast');
  const privateRestart = await scenario('hidden-location-restart', 'home,await-weather');
  assert.equal(privateRestart.preferences.weather_show_location, false);
  assert.ok(!privateRestart.ui_text.some((label) => label.includes('Vienna')));
  const iconsPage = `${weatherPage},down,down,enter,up,enter`;
  const picker = await scenario('icon-picker', iconsPage);
  assert.equal(picker.subpage, 'WeatherIcons');
  for (const name of ['Kuankuan', 'Breezy', 'Lucide', 'Kickstand'])
    assert.ok(picker.ui_text.includes(name));
  const toggleIcon = `${weatherPage},down,down,enter,up,up,enter,home,await-weather`;
  const noIcon = await scenario('show-icon-off', toggleIcon);
  assert.equal(noIcon.preferences.weather_show_icon, false);
  assert.equal(noIcon.preferences.weather_icons, 'kuankuan');
  const noIconRestart = await scenario('show-icon-off-restart', 'home,await-weather');
  assert.equal(noIconRestart.preferences.weather_show_icon, false);
  const showIcon = await scenario('show-icon-on', toggleIcon);
  assert.equal(showIcon.preferences.weather_show_icon, true);
  assert.equal(showIcon.preferences.weather_icons, 'kuankuan');
  assert.equal(forecastCount(), 1, 'Icon visibility must reuse the existing forecast');
  for (const [index, icons] of ['kuankuan', 'breezy', 'lucide', 'kickstand'].entries()) {
    const navigation = index ? `${Array(index).fill('down').join(',')},` : '';
    // Return to a known default so each choice is exercised by its stable picker index.
    const saved = JSON.parse(await fs.readFile(preferences, 'utf8'));
    await fs.writeFile(preferences, JSON.stringify({ ...saved, weather_icons: 'kuankuan' }));
    const selected = await scenario(
      `icons-${icons}`,
      `${iconsPage},${navigation}enter,home,await-weather`,
    );
    assert.equal(selected.preferences.weather_icons, icons);
    assert.equal(selected.preferences.weather_show_location, false);
    assert.ok(!selected.ui_text.some((label) => label.includes('Vienna')));
    assert.equal(forecastCount(), 1, 'Icon selection must reuse the existing forecast');
    const preview = await scenario(`preview-${icons}`, `${weatherPage},down,down,enter`);
    assert.equal(preview.subpage, 'WeatherStyle');
    assert.ok(!preview.ui_text.some((label) => label.includes('Vienna')));
  }
  const iconsHidden = await scenario(
    'icons-hidden',
    `${weatherPage},down,down,enter,up,up,enter,home,await-weather`,
  );
  assert.equal(iconsHidden.preferences.weather_show_icon, false);
  assert.equal(iconsHidden.preferences.weather_icons, 'kickstand');
  assert.equal(forecastCount(), 1, 'Hiding icons must reuse the existing forecast');
  const iconsRestored = await scenario(
    'icons-restored',
    `${weatherPage},down,down,enter,up,up,enter,home,await-weather`,
  );
  assert.equal(iconsRestored.preferences.weather_show_icon, true);
  assert.equal(iconsRestored.preferences.weather_icons, 'kickstand');
  const fahrenheit = await scenario(
    'fahrenheit',
    `${weatherPage},down,enter,down,enter,home,await-weather`,
  );
  assert.equal(fahrenheit.preferences.temperature_unit, 'fahrenheit');
  assert.equal(fahrenheit.weather.cache.reading.temperature, 14.2);
  assert.equal(forecastCount(), 1);
  const removedWidget = await scenario(
    'widget-removed',
    `${weatherActions},down,down,down,enter,home`,
  );
  assert.equal(removedWidget.preferences.show_weather, false);
  assert.equal(removedWidget.preferences.weather_location.name, 'Vienna');
  assert.equal(forecastCount(), 1);
  await writeCache(20 * 60);
  await scenario('removed-widget-stale-cache', 'home');
  assert.equal(forecastCount(), 1, 'A removed widget must not refresh even stale weather');

  await writePreferences({ weather_location: vienna });
  await writeCache(20 * 60);
  forecastMode = 'fail';
  const offline = await scenario('saved-offline', 'home,await-weather');
  assert.equal(offline.weather.failed, true);
  assert.equal(offline.weather.cache.reading.temperature, 14.2);
  assert.equal(forecastCount(), 2);
  await fs.rm(cache);
  const unavailable = await scenario('unavailable', 'home,await-weather');
  assert.equal(unavailable.weather.failed, true);
  assert.equal(unavailable.weather.cache, null);
  assert.equal(forecastCount(), 3);
  await writeCache(20 * 60);
  forecastMode = 'malformed';
  const invalid = await scenario('invalid-reading', 'home,await-weather');
  assert.equal(invalid.weather.failed, true);
  assert.equal(invalid.weather.cache.reading.temperature, 14.2);
  await fs.rm(cache);

  forecastMode = 'slow';
  const removed = await scenario('remove-in-flight', `home,${locationPage},click:1716:201,home`);
  assert.equal(removed.preferences.weather_location, null);
  assert.equal(removed.weather.cache, null);
  await assert.rejects(fs.access(cache));

  await writePreferences({ weather_location: vienna });
  await writeCache(7 * 60 * 60);
  forecastMode = 'fail';
  const expired = await scenario('expired-cache', 'home,await-weather');
  assert.equal(expired.weather.cache, null);
  assert.equal(expired.weather.failed, true);

  await writePreferences();
  for (const query of ['nowhere', 'fail', 'malformed', 'oversized', 'redirect']) {
    const state = await scenario(query, `${locationPage},type:${query},back,enter,await-search`);
    assert.ok(state.weather.message);
    assert.equal(state.weather.results.length, 0);
  }
  assert.ok(
    !requests.some((request) => request.path === '/unexpected'),
    'No redirects to another endpoint',
  );
  const latest = await scenario(
    'latest-search',
    `${locationPage},type:slow,back,enter,click:900:197,click:1364:726,type:Paris,back,enter,await-search,down,enter`,
  );
  assert.equal(latest.preferences.weather_location.name, 'Paris');
  assert.ok(latest.weather.results.length === 0);
  forecastMode = 'ok';
  await writePreferences({ weather_location: vienna });
  await writeCache();
  const small = await scenario('small', `${weatherActions},down,down,enter,home,await-weather`);
  assert.equal(small.preferences.weather_small, true);
  assert.equal(small.weather.cache.reading.temperature, 14.2);
  const sizePopup = await scenario('size-popup', `${weatherActions},down,down`);
  assert.equal(sizePopup.focus, 'WeatherSize');
  assert.ok(sizePopup.ui_text.includes('Size · 1 × 1'));
  const smallRestart = await scenario('small-restart', 'home,await-weather');
  assert.equal(smallRestart.preferences.weather_small, true);
  assert.equal(smallRestart.preferences.weather_location.name, 'Vienna');
  const beforeMultiple = forecastCount();
  await writePreferences({
    weather_location: vienna,
    weather_show_location: false,
    extra_weather: [
      {
        enabled: true,
        location: paris,
        unit: 'fahrenheit',
        small: true,
        position: { column: 0, row: 1 },
      },
    ],
  });
  const multiple = await scenario('multiple', 'home,await-weather');
  assert.ok(!multiple.ui_text.some((label) => label.includes('Vienna')));
  assert.ok(multiple.ui_text.includes('Paris · Partly cloudy'));
  assert.equal(multiple.preferences.temperature_unit, 'celsius');
  assert.equal(multiple.preferences.extra_weather[0].unit, 'fahrenheit');
  assert.equal(forecastCount(), beforeMultiple + 1);
  const extraCache = path.join(temporary, 'preferences.weather.widget-0.json');
  assert.equal(JSON.parse(await fs.readFile(extraCache, 'utf8')).key, '48.8534,2.3488');
  assert.equal(JSON.parse(await fs.readFile(cache, 'utf8')).key, '48.2085,16.3721');
  await scenario('multiple-restart', 'home,await-weather');
  assert.equal(forecastCount(), beforeMultiple + 1, 'Each weather instance reuses its own cache');
  console.log(
    'Native weather smoke checks passed: explicit search, request encoding, cached restart, units, widget removal, offline/expired data, removal races, response limits and stale-search suppression.',
  );
} finally {
  server.closeAllConnections();
  await new Promise((resolve) => server.close(resolve));
  await fs.rm(temporary, { recursive: true, force: true });
}
