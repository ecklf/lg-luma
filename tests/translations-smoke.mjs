import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import path from 'node:path';
import http from 'node:http';
import { spawn } from 'node:child_process';
import messages from '../assets/messages.json' with { type: 'json' };

const root = path.resolve(import.meta.dirname, '..');
const out = path.join(root, '.build');
await fs.mkdir(out, { recursive: true });
const temporary = await fs.mkdtemp(path.join(out, 'translations-smoke-'));
const preferences = path.join(temporary, 'preferences.json');
const binary = path.resolve(root, process.env.CARGO_TARGET_DIR ?? 'target', 'debug/luma');
const requests = [];
const places = {
  en: { name: 'Vienna', region: 'Vienna', country: 'Austria' },
  de: { name: 'Wien', region: 'Wien', country: 'Österreich' },
  zh: { name: '维也纳', region: '维也纳', country: '奥地利' },
  pt: { name: 'Viena', region: 'Viena', country: 'Áustria' },
};
const server = http.createServer((request, response) => {
  const url = new URL(request.url, 'http://127.0.0.1');
  requests.push({ path: url.pathname, query: Object.fromEntries(url.searchParams) });
  response.setHeader('Content-Type', 'application/json');
  if (url.pathname === '/v1/search') {
    const place = places[url.searchParams.get('language')];
    assert.ok(place, 'Geocoding must use a supported UI language');
    response.end(
      JSON.stringify({
        results: [{ ...place, admin1: place.region, latitude: 48.2085, longitude: 16.3721 }],
      }),
    );
  } else if (url.pathname === '/v1/forecast') {
    assert.equal(url.searchParams.get('temperature_unit'), 'celsius');
    response.end(JSON.stringify({ current: { temperature_2m: 14.2, weather_code: 2, is_day: 1 } }));
  } else {
    response.writeHead(404);
    response.end('{}');
  }
});
await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
const origin = `http://127.0.0.1:${server.address().port}`;
async function run(command, args) {
  await new Promise((resolve, reject) => {
    const child = spawn(command, args, { cwd: root, stdio: 'inherit' });
    const timer = setTimeout(() => {
      child.kill('SIGKILL');
      reject(new Error('Translation smoke child timed out'));
    }, 45000);
    child.on('error', (error) => {
      clearTimeout(timer);
      reject(error);
    });
    child.on('exit', (code) => {
      clearTimeout(timer);
      if (code === 0) resolve();
      else reject(new Error(`${command} exited ${code}`));
    });
  });
}
async function scenario(name, script, extra = []) {
  const dump = path.join(temporary, `${name}.json`);
  const report = path.join(out, `language-${name}-render.json`);
  await run(binary, [
    '--preferences',
    preferences,
    '--weather-test-server',
    origin,
    '--script',
    script,
    '--frames',
    '120',
    '--time',
    '2026-10-21T20:41:00+02:00',
    '--screenshot',
    path.join(out, `language-${name}.png`),
    '--dump-state',
    dump,
    '--diagnostics',
    report,
    ...extra,
  ]);
  const state = JSON.parse(await fs.readFile(dump, 'utf8'));
  const render = JSON.parse(await fs.readFile(report, 'utf8'));
  assert.equal(state.error, null);
  assert.equal(
    render.capture.missing_glyphs,
    0,
    'Translated text must not become missing-character boxes',
  );
  assert.equal(render.capture.glyph_failures, 0);
  assert.equal(
    render.capture.missing_images,
    0,
    'Settings icons and other translated-screen artwork must be present',
  );
  return state;
}
function contains(state, key, language) {
  assert.ok(state.ui_text.includes(messages[key][language]), `Missing rendered ${key}/${language}`);
}
const picker = 'settings,down,enter,down,down,down,enter';
const location = 'settings,down,down,down,enter,down,enter';
try {
  await run('cargo', ['build', '--locked', '--package', 'luma']);
  await fs.writeFile(
    preferences,
    JSON.stringify({ language: 'en', show_home_names: true, reduced_motion: true }),
  );
  const languages = await scenario('picker-en', picker);
  assert.equal(languages.subpage, 'Language');
  for (const name of ['English', 'Deutsch', '简体中文', 'Español', 'Português (Brasil)'])
    assert.ok(languages.ui_text.includes(name));

  const german = await scenario('de-home', `${picker},down,enter,home,down`);
  assert.equal(german.preferences.language, 'de');
  for (const key of [
    'home',
    'settings',
    'favorites',
    'inputsHeading',
    'evening',
    'setWeatherLocation',
  ])
    contains(german, key, 'de');
  assert.ok(german.ui_text.includes('Netflix'), 'Installed app names stay intact');
  const persisted = await scenario('de-settings', 'settings,down');
  assert.equal(persisted.preferences.language, 'de');
  contains(persisted, 'language', 'de');

  const chinese = await scenario('zh-home', `${picker},down,enter,home,down`);
  assert.equal(chinese.preferences.language, 'zh');
  for (const key of [
    'home',
    'apps',
    'settings',
    'favorites',
    'inputsHeading',
    'evening',
    'setWeatherLocation',
  ])
    contains(chinese, key, 'zh');
  assert.ok(chinese.ui_text.includes('Netflix'));
  assert.ok(chinese.ui_text.includes('10月21日星期三'));
  const chineseSettings = await scenario('zh-settings', 'settings,down');
  contains(chineseSettings, 'language', 'zh');
  contains(chineseSettings, 'appNames', 'zh');

  const selected = await scenario(
    'zh-weather',
    `${location},type:Vienna,back,enter,await-search,down,enter,home,await-weather`,
  );
  assert.equal(selected.preferences.weather_location.name, '维也纳');
  contains(selected, 'partlyCloudy', 'zh');
  assert.equal(requests.find((request) => request.path === '/v1/search').query.language, 'zh');
  const forecasts = requests.filter((request) => request.path === '/v1/forecast').length;
  const changed = await scenario('de-weather', `${picker},up,enter,home,await-weather`);
  assert.equal(changed.preferences.language, 'de');
  contains(changed, 'partlyCloudy', 'de');
  assert.ok(changed.ui_text.includes('维也纳'), 'The saved city name is data, not a UI label');
  assert.equal(
    requests.filter((request) => request.path === '/v1/forecast').length,
    forecasts,
    'Language changes reuse weather cache',
  );
  const germanPlace = await scenario(
    'de-search',
    `${location},type:Wien,back,enter,await-search,down,enter`,
  );
  assert.equal(germanPlace.preferences.weather_location.name, 'Wien');
  assert.equal(
    requests.filter((request) => request.path === '/v1/search').at(-1).query.language,
    'de',
  );

  const germanDialog = await scenario('de-dialog', 'apps,down,menu');
  contains(germanDialog, 'removeFavorite', 'de');
  contains(germanDialog, 'hideApp', 'de');
  const saved = JSON.parse(await fs.readFile(preferences, 'utf8'));
  await fs.writeFile(preferences, JSON.stringify({ ...saved, language: 'zh' }));
  const chineseDialog = await scenario('zh-dialog', 'apps,down,menu');
  contains(chineseDialog, 'removeFavorite', 'zh');
  contains(chineseDialog, 'categoryStreaming', 'zh');
  const input = await scenario('zh-input-options', 'settings,down,down,down,down,enter,enter');
  for (const key of ['stateOn', 'stateOff', 'stateAuto', 'inputWhenConnected'])
    contains(input, key, 'zh');
  const background = await scenario('zh-background', 'settings,down,down,enter');
  for (const key of ['slate', 'midnight', 'monumentValley', 'northernLights'])
    contains(background, key, 'zh');
  const keyboard = await scenario('zh-keyboard', 'apps,type:Netflix');
  for (const key of ['findApp', 'space', 'delete', 'clear', 'done']) contains(keyboard, key, 'zh');
  const weatherSettings = await scenario('zh-weather-settings', 'settings,down,down,down,enter');
  for (const key of ['showWidget', 'location', 'temperature']) contains(weatherSettings, key, 'zh');
  assert.ok(
    !weatherSettings.ui_text.some((label) => label.includes('Open-Meteo')),
    'Weather credit is not shown in the UI',
  );
  await scenario('zh-4k', 'home,down', ['--capture-scale', '2']);

  await fs.writeFile(
    preferences,
    JSON.stringify({ language: 'en', show_home_names: true, reduced_motion: true }),
  );
  const portuguese = await scenario('pt-home', `${picker},down,down,down,down,enter,home,down`);
  assert.equal(portuguese.preferences.language, 'pt');
  for (const key of [
    'home',
    'apps',
    'settings',
    'favorites',
    'inputsHeading',
    'evening',
    'setWeatherLocation',
  ])
    contains(portuguese, key, 'pt');
  assert.ok(portuguese.ui_text.includes('QUARTA-FEIRA, 21 DE OUTUBRO'));
  assert.ok(
    portuguese.ui_text.includes('Netflix'),
    'Installed app names stay intact in Portuguese',
  );
  const portugueseSettings = await scenario('pt-settings', 'settings,down');
  assert.equal(
    portugueseSettings.preferences.language,
    'pt',
    'Portuguese is restored after restart',
  );
  assert.ok(portugueseSettings.ui_text.includes('Português (Brasil)'));
  for (const key of ['language', 'appNames', 'clock']) contains(portugueseSettings, key, 'pt');
  assert.equal(JSON.parse(await fs.readFile(preferences, 'utf8')).language, 'pt');

  const portugueseDialog = await scenario('pt-dialog', 'apps,down,menu');
  for (const key of ['removeFavorite', 'hideApp', 'categoryStreaming'])
    contains(portugueseDialog, key, 'pt');
  const portugueseKeyboard = await scenario('pt-keyboard', 'apps,type:Netflix');
  for (const key of ['findApp', 'space', 'delete', 'clear', 'done'])
    contains(portugueseKeyboard, key, 'pt');
  const portugueseInputs = await scenario(
    'pt-input-options',
    'settings,down,down,down,down,enter,enter',
  );
  for (const key of ['stateOn', 'stateOff', 'stateAuto', 'inputWhenConnected'])
    contains(portugueseInputs, key, 'pt');

  const portugueseWeather = await scenario(
    'pt-weather',
    `${location},type:Viena,back,enter,await-search,down,enter,home,await-weather`,
  );
  assert.equal(portugueseWeather.preferences.language, 'pt');
  assert.equal(portugueseWeather.preferences.weather_location.name, 'Viena');
  assert.equal(portugueseWeather.preferences.weather_location.country, 'Áustria');
  contains(portugueseWeather, 'partlyCloudy', 'pt');
  assert.equal(
    requests.filter((request) => request.path === '/v1/search').at(-1).query.language,
    'pt',
    'Geocoding must use the Portuguese locale',
  );
  console.log(
    'Native translation checks passed: saved language selection, shared labels, Portuguese locale formatting and accented glyphs, localized geocoding/weather, unchanged app names, Chinese glyphs and 4K rendering.',
  );
} finally {
  server.closeAllConnections();
  await new Promise((resolve) => server.close(resolve));
  await fs.rm(temporary, { recursive: true, force: true });
}
