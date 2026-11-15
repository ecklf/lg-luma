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

const screen = document.querySelector('.screen');
const previewImage = element('preview');
let previewUrl;
let previewBusy = false;
let previewAgain = false;
let previewTimer;

async function refreshPreview() {
  if (previewBusy) {
    previewAgain = true;
    return;
  }
  previewBusy = true;
  screen.classList.add('updating');
  try {
    const response = await fetch(`api/preview.jpg?t=${Date.now()}`, { cache: 'no-store' });
    if (!response.ok) throw new Error();
    const url = URL.createObjectURL(await response.blob());
    await new Promise((resolve) => {
      previewImage.onload = resolve;
      previewImage.onerror = resolve;
      previewImage.src = url;
    });
    if (previewUrl) URL.revokeObjectURL(previewUrl);
    previewUrl = url;
    previewImage.classList.add('ready');
    element('preview-empty').hidden = true;
  } catch {
    if (!previewUrl) {
      element('preview-empty').textContent = strings.previewUnavailable;
      element('preview-empty').hidden = false;
    }
  } finally {
    screen.classList.remove('updating');
    previewBusy = false;
    if (previewAgain) {
      previewAgain = false;
      refreshPreview();
    }
  }
}

function schedulePreview(delay = 250) {
  clearTimeout(previewTimer);
  previewTimer = setTimeout(refreshPreview, delay);
}

// The clock on the TV moves; keep the picture current while the page is in view.
setInterval(() => {
  if (state && document.visibilityState === 'visible') refreshPreview();
}, 30000);

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
  element('preview-label').textContent = strings.preview;
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
