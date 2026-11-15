const element = (id) => document.getElementById(id);
const status = element('status');
let state;
let strings = {};
let tab = 'general';
let queue = Promise.resolve();
let statusTimer;

try {
  tab = localStorage.getItem('luma-tab') || tab;
} catch {
  // Storage can be unavailable in private windows; the first tab is fine then.
}

function message(text, error = false, sticky = false) {
  clearTimeout(statusTimer);
  status.textContent = text;
  status.classList.toggle('error', error);
  if (text && !error && !sticky) statusTimer = setTimeout(() => (status.textContent = ''), 2200);
}

class PairingNeeded extends Error {}

async function call(route, body) {
  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), 15000);
  try {
    const response = await fetch(route, {
      method: body === undefined ? 'GET' : 'POST',
      cache: 'no-store',
      credentials: 'same-origin',
      // The custom header is what the TV accepts writes with; other sites cannot send it.
      headers:
        body === undefined ? {} : { 'Content-Type': 'application/json', 'X-Luma-Request': '1' },
      body: body === undefined ? undefined : JSON.stringify(body),
      signal: controller.signal,
    });
    const result = await response.json().catch(() => ({}));
    if (result.pair) throw new PairingNeeded(result.error);
    if (!response.ok) throw new Error(result.error || strings.offline);
    return result;
  } catch (error) {
    if (error instanceof PairingNeeded) throw error;
    if (error.name === 'AbortError') throw new Error(strings.timeout);
    if (error instanceof TypeError) throw new Error(strings.offline);
    throw error;
  } finally {
    clearTimeout(timeout);
  }
}

// A readable default name for the TV's device list.
function deviceName() {
  const agent = navigator.userAgent;
  const system = /iPhone/.test(agent)
    ? 'iPhone'
    : /iPad/.test(agent) || (/Macintosh/.test(agent) && navigator.maxTouchPoints > 1)
      ? 'iPad'
      : /Android/.test(agent)
        ? 'Android'
        : /Macintosh/.test(agent)
          ? 'Mac'
          : /Windows/.test(agent)
            ? 'Windows'
            : /Linux/.test(agent)
              ? 'Linux'
              : 'Browser';
  const browser = /Edg\//.test(agent)
    ? 'Edge'
    : /Firefox\//.test(agent)
      ? 'Firefox'
      : /Chrome\//.test(agent)
        ? 'Chrome'
        : /Safari\//.test(agent)
          ? 'Safari'
          : '';
  return browser ? `${system} · ${browser}` : system;
}

/* ---------- Live preview ---------- */

// Long polling: the TV holds each request until its screen changes, so a new picture arrives as
// soon as Luma has drawn it. One request is open at a time, only while the page is in view.
const screen = document.querySelector('.screen');
const previewImage = element('preview');
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
let previewUrl;
let previewFrame = '';
let previewRunning = false;
let previewOnScreen = true;

async function showFrame(response) {
  const frame = response.headers.get('X-Luma-Frame') || '';
  const onScreen = response.headers.get('X-Luma-On-Screen') !== 'false';
  const url = URL.createObjectURL(await response.blob());
  await new Promise((resolve) => {
    previewImage.onload = resolve;
    previewImage.onerror = resolve;
    previewImage.src = url;
  });
  if (previewUrl) URL.revokeObjectURL(previewUrl);
  previewUrl = url;
  previewFrame = frame;
  previewOnScreen = onScreen;
  element('preview-label').textContent = onScreen ? strings.preview : strings.previewBackground;
  screen.parentElement.classList.toggle('behind', !onScreen);
  previewImage.classList.add('ready');
  element('preview-empty').hidden = true;
}

async function refreshPreview() {
  if (previewRunning) return;
  previewRunning = true;
  while (state && document.visibilityState === 'visible') {
    const controller = new AbortController();
    const timeout = setTimeout(() => controller.abort(), 30000);
    try {
      const wait = previewFrame ? '&wait=1' : '';
      const response = await fetch(`api/preview.jpg?known=${previewFrame}${wait}`, {
        cache: 'no-store',
        signal: controller.signal,
      });
      if (response.status === 200) await showFrame(response);
      else if (response.status !== 204) throw new Error();
      screen.classList.remove('stale');
    } catch {
      if (previewUrl) {
        screen.classList.add('stale');
      } else {
        element('preview-empty').textContent = strings.previewUnavailable;
        element('preview-empty').hidden = false;
      }
      await sleep(2000);
    } finally {
      clearTimeout(timeout);
    }
  }
  previewRunning = false;
}

// A change made here shows up through the open request; this only restarts a stopped loop.
function schedulePreview() {
  refreshPreview();
}

/* ---------- Controls ---------- */

function create(tag, className, text) {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}

function title(text) {
  return create('span', 'title', text);
}

function toggleRow(key, label) {
  const row = create('label', 'row inline');
  const holder = create('span', 'switch');
  const input = create('input');
  input.type = 'checkbox';
  input.role = 'switch';
  input.checked = state.settings[key];
  input.setAttribute('aria-label', label);
  input.addEventListener('change', () => change(key, input.checked));
  holder.append(input, create('span'));
  row.append(title(label), holder);
  return row;
}

function segmentRow(key, label) {
  const row = create('div', 'row stacked');
  const group = create('div', 'segments');
  group.role = 'radiogroup';
  group.setAttribute('aria-label', label);
  for (const choice of state.choices[key]) {
    const option = create('label');
    const input = create('input');
    input.type = 'radio';
    input.name = key;
    input.checked = choice.value === state.settings[key];
    input.addEventListener('change', () => change(key, choice.value));
    option.append(input, create('span', '', choice.label));
    group.append(option);
  }
  row.append(title(label), group);
  return row;
}

function pickerRow(key, label) {
  const row = create('label', 'row picker');
  const current = state.choices[key].find((choice) => choice.value === state.settings[key]);
  const select = create('select');
  select.setAttribute('aria-label', label);
  for (const choice of state.choices[key]) {
    const option = create('option', '', choice.label);
    option.value = JSON.stringify(choice.value);
    option.selected = choice === current;
    select.append(option);
  }
  select.addEventListener('change', () => change(key, JSON.parse(select.value)));
  row.append(title(label), create('span', 'value', current?.label ?? ''), select);
  return row;
}

function choiceRow(key, label) {
  return state.choices[key].length > 3 ? pickerRow(key, label) : segmentRow(key, label);
}

function textRow(key, label) {
  const row = create('label', 'row stacked');
  const field = create('input', 'text-field');
  field.type = 'text';
  field.maxLength = 40;
  field.autocomplete = 'off';
  field.enterKeyHint = 'done';
  field.value = state.settings[key];
  field.addEventListener('change', () => change(key, field.value.trim()));
  field.addEventListener('keydown', (event) => event.key === 'Enter' && field.blur());
  row.append(title(label), field);
  return row;
}

function wallpapers() {
  const grid = create('div', 'wallpapers');
  for (const background of state.backgrounds) {
    const button = create('button', 'wallpaper');
    button.type = 'button';
    button.setAttribute('aria-pressed', String(background.id === state.settings.background));
    const image = create('img');
    image.loading = 'lazy';
    image.alt = '';
    image.src = `api/thumbnail/${encodeURIComponent(background.id)}`;
    button.append(image, create('span', '', background.name || strings.custom));
    button.addEventListener('click', () => change('background', background.id));
    grid.append(button);
  }
  return grid;
}

/* ---------- Apps ---------- */

// Luma draws every app tile in sheets of 25 on a 1920×1080 surface; tiles are cut out with CSS,
// so they look exactly like on the TV, brand logos included.
const SHEET = {
  width: 1920,
  height: 1080,
  tile: [360, 202.5],
  origin: [24, 24],
  pitch: [376, 210],
};
const sheets = new Map();

async function loadSheet(page, again = true) {
  const known = sheets.get(page)?.frame ?? '';
  try {
    const response = await fetch(`api/apps.jpg?page=${page}&known=${known}`, { cache: 'no-store' });
    if (response.status === 200) {
      const old = sheets.get(page);
      const url = URL.createObjectURL(await response.blob());
      sheets.set(page, { url, frame: response.headers.get('X-Luma-Frame') || '' });
      if (old) URL.revokeObjectURL(old.url);
      for (const thumb of document.querySelectorAll(`[data-sheet="${page}"]`)) {
        thumb.style.backgroundImage = `url("${url}")`;
      }
    }
  } catch {
    // Tiles stay as plain cards; the next visit asks again.
  }
  // Icons decode in the background on the TV; pick up late ones.
  if (again) setTimeout(() => loadSheet(page, false), 2500);
}

function thumb(index) {
  const node = create('span', 'thumb');
  const page = Math.floor(index / state.sheet.tiles);
  const slot = index % state.sheet.tiles;
  const column = slot % state.sheet.columns;
  const row = Math.floor(slot / state.sheet.columns);
  const x = SHEET.origin[0] + column * SHEET.pitch[0];
  const y = SHEET.origin[1] + row * SHEET.pitch[1];
  node.dataset.sheet = String(page);
  node.style.backgroundSize = `${(SHEET.width / SHEET.tile[0]) * 100}% auto`;
  node.style.backgroundPosition = `${(x / (SHEET.width - SHEET.tile[0])) * 100}% ${(y / (SHEET.height - SHEET.tile[1])) * 100}%`;
  const sheet = sheets.get(page);
  if (sheet) node.style.backgroundImage = `url("${sheet.url}")`;
  else loadSheet(page);
  return node;
}

function arrange(favorites, hidden) {
  state.library.favorites = favorites;
  state.library.hidden = hidden;
  render();
  queue = queue.then(async () => {
    message(strings.saving, false, true);
    try {
      apply(await call('api/apps', { favorites, hidden }));
      message(strings.saved);
    } catch (error) {
      if (error instanceof PairingNeeded) return startPairing(error.message);
      message(error.message, true);
      load();
    }
  });
}

function iconButton(icon, label, pressed, action) {
  const button = create('button', `icon-button icon-${icon}`);
  button.append(create('span', 'glyph'));
  button.type = 'button';
  button.title = label;
  button.setAttribute('aria-label', label);
  if (pressed !== undefined) button.setAttribute('aria-pressed', String(pressed));
  button.addEventListener('click', action);
  return button;
}

function appsSection() {
  const l = state.labels;
  const { apps, favorites, hidden } = state.library;
  const index = new Map(apps.map((app, position) => [app.id, position]));
  const title = (id) => apps[index.get(id)]?.title ?? id;

  const dock = create('div', 'rows');
  const shown = favorites.filter((id) => index.has(id));
  shown.forEach((id, position) => {
    const row = create('div', 'row favorite');
    const move = (offset) => {
      const next = [...favorites];
      const from = next.indexOf(id);
      const to = from + offset;
      if (to < 0 || to >= next.length) return;
      [next[from], next[to]] = [next[to], next[from]];
      arrange(next, hidden);
    };
    const up = iconButton('up', `${title(id)} ↑`, undefined, () => move(-1));
    const down = iconButton('down', `${title(id)} ↓`, undefined, () => move(1));
    up.disabled = position === 0;
    down.disabled = position === shown.length - 1;
    const remove = iconButton('heart-filled', l.removeFavorite, true, () =>
      arrange(
        favorites.filter((item) => item !== id),
        hidden,
      ),
    );
    row.append(thumb(index.get(id)), create('span', 'title', title(id)), up, down, remove);
    dock.append(row);
  });
  if (!shown.length) dock.append(create('p', 'hint', l.makeFavorites));

  const grid = create('div', 'app-grid');
  for (const app of apps) {
    const favorite = favorites.includes(app.id);
    const isHidden = hidden.includes(app.id);
    const card = create('div', isHidden ? 'app-card is-hidden' : 'app-card');
    const actions = create('div', 'app-actions');
    actions.append(
      iconButton(
        favorite ? 'heart-filled' : 'heart',
        favorite ? l.removeFavorite : l.addFavorite,
        favorite,
        () =>
          arrange(
            favorite ? favorites.filter((id) => id !== app.id) : [...favorites, app.id],
            hidden.filter((id) => id !== app.id),
          ),
      ),
      iconButton(isHidden ? 'eye-off' : 'eye', isHidden ? l.showApp : l.hideApp, isHidden, () =>
        arrange(
          favorites.filter((id) => id !== app.id),
          isHidden ? hidden.filter((id) => id !== app.id) : [...hidden, app.id],
        ),
      ),
    );
    const name = create('span', 'app-name', app.title);
    if (isHidden) name.append(create('small', '', ` · ${l.hidden}`));
    card.append(thumb(index.get(app.id)), name, actions);
    grid.append(card);
  }
  return [group(l.favorites, dock), group(l.apps, grid)];
}

/* ---------- Inputs ---------- */

function changeInput(input, changes) {
  const next = { ...input, ...changes };
  Object.assign(input, changes);
  if ('icon' in changes || 'mode' in changes) render();
  queue = queue.then(async () => {
    message(strings.saving, false, true);
    try {
      apply(
        await call('api/inputs', {
          id: next.id,
          name: next.name,
          mode: next.mode,
          icon: next.icon,
        }),
      );
      message(strings.saved);
    } catch (error) {
      if (error instanceof PairingNeeded) return startPairing(error.message);
      message(error.message, true);
      load();
    }
  });
}

function inputCard(input) {
  const l = state.labels;
  const card = create('div', 'row stacked input-card');
  const head = create('div', 'input-head');
  const mark = create('span', 'input-mark');
  mark.style.setProperty('--icon', `url('icon/input-${input.icon || input.auto_icon}.svg')`);
  head.append(mark, create('span', 'title', input.name || input.port));
  if (input.name) head.append(create('span', 'muted port', input.port));

  const name = create('input', 'text-field');
  name.type = 'text';
  name.maxLength = 40;
  name.autocomplete = 'off';
  name.enterKeyHint = 'done';
  name.placeholder = input.port;
  name.value = input.name;
  name.setAttribute('aria-label', l.inputName);
  name.addEventListener('change', () => changeInput(input, { name: name.value.trim() }));
  name.addEventListener('keydown', (event) => event.key === 'Enter' && name.blur());

  const modes = create('div', 'segments');
  modes.role = 'radiogroup';
  for (const choice of state.choices.input_mode) {
    const option = create('label');
    option.title = choice.hint;
    const radio = create('input');
    radio.type = 'radio';
    radio.name = `mode-${input.id}`;
    radio.checked = choice.value === input.mode;
    radio.addEventListener('change', () => changeInput(input, { mode: choice.value }));
    option.append(radio, create('span', '', choice.label));
    modes.append(option);
  }
  const hint = create(
    'p',
    'hint inline-hint',
    state.choices.input_mode.find((choice) => choice.value === input.mode)?.hint ?? '',
  );

  const picker = create('label', 'row picker nested');
  const current = state.choices.input_icon.find((choice) => choice.value === input.icon);
  const select = create('select');
  select.setAttribute('aria-label', l.inputIcon);
  for (const choice of state.choices.input_icon) {
    const option = create('option', '', choice.label);
    option.value = choice.value;
    option.selected = choice === current;
    select.append(option);
  }
  select.addEventListener('change', () => changeInput(input, { icon: select.value }));
  picker.append(
    create('span', 'title', l.inputIcon),
    create('span', 'value', current?.label ?? ''),
    select,
  );

  card.append(head, name, modes, hint, picker);
  return card;
}

function group(heading, ...rows) {
  const section = create('section', 'group');
  if (heading) section.append(create('h2', '', heading));
  const list = create('div', 'rows');
  list.append(...rows);
  section.append(list);
  return section;
}

function sections() {
  const l = state.labels;
  return {
    general: [
      group(l.clock, choiceRow('hour_24', l.clockFormat), choiceRow('show_seconds', l.timeFormat)),
      group(
        null,
        choiceRow('language', l.language),
        choiceRow('app_sorting', l.appSorting),
        toggleRow('reduced_motion', l.reducedMotion),
        toggleRow('ui_sounds', l.uiSounds),
      ),
    ],
    apps: appsSection(),
    background: [
      group(null, wallpapers()),
      group(
        null,
        toggleRow('wallpaper_gradient', l.wallpaperGradient),
        toggleRow('wallpaper_cycle', l.wallpaperCycle),
      ),
    ],
    widgets: [
      group(
        l.greeting,
        toggleRow('show_greeting', l.showWidget),
        textRow('greeting_name', l.greetingName),
      ),
      group(l.date, toggleRow('show_date', l.showWidget), choiceRow('date_format', l.dateFormat)),
      group(
        l.weather,
        toggleRow('show_weather', l.showWidget),
        choiceRow('temperature_unit', l.temperature),
        choiceRow('weather_style', l.weatherStyle),
      ),
      ...(state.inputs.length ? [group(l.inputs, ...state.inputs.map(inputCard))] : []),
    ],
    system: [group(null, choiceRow('text_rendering', l.textRendering))],
  };
}

function render() {
  const l = state.labels;
  const all = sections();
  if (!all[tab]) tab = 'general';
  const tabs = element('tabs');
  tabs.replaceChildren(
    ...Object.keys(all).map((key) => {
      const button = create('button', '', l[key]);
      button.type = 'button';
      button.role = 'tab';
      button.setAttribute('aria-selected', String(key === tab));
      button.addEventListener('click', () => {
        tab = key;
        try {
          localStorage.setItem('luma-tab', key);
        } catch {
          // Remembering the tab is only a convenience.
        }
        render();
      });
      return button;
    }),
  );
  element('sections').replaceChildren(...all[tab]);
  element('device').textContent = strings.pairedAs.replace('{device}', state.device);
  element('forget').textContent = strings.unpair;
  element('preview-label').textContent = previewOnScreen
    ? strings.preview
    : strings.previewBackground;
  element('backdrop').style.backgroundImage =
    `url("api/thumbnail/${encodeURIComponent(state.settings.background)}")`;
  document.documentElement.lang = strings.lang || 'en';
}

function show(view) {
  element('pair').hidden = view !== 'pair';
  element('app').hidden = view !== 'app';
}

function apply(result) {
  state = result;
  strings = result.strings;
  const focused = document.activeElement;
  // Rebuilding while someone types would drop their cursor.
  if (focused?.classList.contains('text-field')) return;
  render();
  show('app');
}

function change(key, value) {
  const next = { ...state.settings, [key]: value };
  state.settings = next;
  if (key === 'background') render();
  // Changes go out one at a time, in order, so the TV ends on the last one.
  queue = queue.then(async () => {
    message(strings.saving, false, true);
    try {
      apply(await call('api/settings', next));
      message(key === 'text_rendering' ? strings.restart : strings.saved);
      schedulePreview();
    } catch (error) {
      if (error instanceof PairingNeeded) return startPairing(error.message);
      message(error.message, true);
      load();
    }
  });
}

/* ---------- Pairing ---------- */

function startPairing(note = '') {
  element('pair-title').textContent = strings.pairTitle;
  element('pair-intro').textContent = strings.pairIntro;
  element('pair-button').textContent = strings.pair;
  show('pair');
  if (note) message(note, true);
  element('pin').focus();
}

async function pair(pin) {
  element('pair-button').disabled = true;
  message(strings.pairing, false, true);
  try {
    await call('api/pair', { pin, name: deviceName() });
    message('');
    await load();
  } catch (error) {
    startPairing(error.message);
  } finally {
    element('pair-button').disabled = false;
  }
}

async function load() {
  try {
    apply(await call('api/settings'));
    refreshPreview();
  } catch (error) {
    if (error instanceof PairingNeeded) {
      startPairing();
      return;
    }
    message(error.message, true);
  }
}

element('pair-form').addEventListener('submit', (event) => {
  event.preventDefault();
  pair(element('pin').value);
});

element('forget').addEventListener('click', async () => {
  try {
    await call('api/forget', {});
    state = undefined;
    startPairing();
    message(strings.unpaired);
  } catch (error) {
    message(error.message, true);
  }
});

// Settings changed with the remote show up when the page comes back into view.
document.addEventListener('visibilitychange', () => {
  if (document.visibilityState === 'visible' && state) load();
});

async function start() {
  try {
    strings = await call('api/strings');
  } catch (error) {
    message(error.message, true);
  }
  const pin = new URLSearchParams(location.hash.slice(1)).get('pin');
  // Keep the PIN out of bookmarks and history: the saved address is the plain page.
  if (location.hash) history.replaceState(null, '', location.pathname);
  if (!pin) return load();
  element('pin').value = pin;
  // An already paired device just opens its settings.
  try {
    apply(await call('api/settings'));
    refreshPreview();
  } catch (error) {
    if (error instanceof PairingNeeded) pair(pin);
    else message(error.message, true);
  }
}

start();
