const element = (id) => document.getElementById(id);
const status = element('status');
let state;
let busy = Promise.resolve();

function message(text, error = false) {
  status.textContent = text;
  status.classList.toggle('error', error);
}

function show(view) {
  element('loading').hidden = view !== 'loading';
  element('pair').hidden = view !== 'pair';
  element('settings').hidden = view !== 'settings';
  element('footer').hidden = view !== 'settings';
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
    if (!response.ok) throw new Error(result.error || 'Your TV could not complete this.');
    return result;
  } catch (error) {
    if (error instanceof PairingNeeded) throw error;
    if (error.name === 'AbortError') throw new Error('Your TV did not respond in time.');
    if (error instanceof TypeError) {
      throw new Error('Your TV is not reachable. Is it on, with web access turned on?');
    }
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

function toggle(key) {
  const label = document.createElement('label');
  label.className = 'switch';
  const input = document.createElement('input');
  input.type = 'checkbox';
  input.role = 'switch';
  input.checked = state.settings[key];
  input.addEventListener('change', () => change(key, input.checked));
  label.append(input, document.createElement('span'));
  return [label, input];
}

function segments(key, title) {
  const group = document.createElement('div');
  group.className = 'segments';
  group.role = 'radiogroup';
  group.setAttribute('aria-label', title);
  for (const choice of state.choices[key]) {
    const label = document.createElement('label');
    const input = document.createElement('input');
    input.type = 'radio';
    input.name = key;
    input.checked = choice.value === state.settings[key];
    input.addEventListener('change', () => change(key, choice.value));
    const text = document.createElement('span');
    text.textContent = choice.label;
    label.append(input, text);
    group.append(label);
  }
  return group;
}

function select(key, title) {
  const field = document.createElement('select');
  field.setAttribute('aria-label', title);
  for (const choice of state.choices[key]) {
    const option = document.createElement('option');
    option.value = JSON.stringify(choice.value);
    option.textContent = choice.label;
    option.selected = choice.value === state.settings[key];
    field.append(option);
  }
  field.addEventListener('change', () => change(key, JSON.parse(field.value)));
  return field;
}

function text(key, title) {
  const field = document.createElement('input');
  field.type = 'text';
  field.maxLength = 40;
  field.autocomplete = 'off';
  field.value = state.settings[key];
  field.setAttribute('aria-label', title);
  field.addEventListener('change', () => change(key, field.value.trim()));
  return field;
}

function row(title, control, inline = false) {
  const item = document.createElement('div');
  item.className = inline ? 'row inline' : 'row';
  const name = document.createElement('span');
  name.textContent = title;
  if (inline) {
    const [label, input] = control;
    input.setAttribute('aria-label', title);
    item.append(name, label);
  } else {
    item.append(name, control);
  }
  return item;
}

function render() {
  const l = state.labels;
  const choice = (key, title) =>
    state.choices[key].length > 3 ? select(key, title) : segments(key, title);
  const groups = [
    [
      l.clock,
      [
        row(l.clockFormat, choice('hour_24', l.clockFormat)),
        row(l.timeFormat, choice('show_seconds', l.timeFormat)),
      ],
    ],
    [
      l.greeting,
      [
        row(l.showWidget, toggle('show_greeting'), true),
        row(l.greetingName, text('greeting_name', l.greetingName)),
      ],
    ],
    [
      l.date,
      [
        row(l.showWidget, toggle('show_date'), true),
        row(l.dateFormat, choice('date_format', l.dateFormat)),
      ],
    ],
    [
      l.weather,
      [
        row(l.showWidget, toggle('show_weather'), true),
        row(l.temperature, choice('temperature_unit', l.temperature)),
        row(l.weatherStyle, choice('weather_style', l.weatherStyle)),
      ],
    ],
    [
      l.background,
      [
        row(l.wallpaperCycle, toggle('wallpaper_cycle'), true),
        row(l.wallpaperGradient, toggle('wallpaper_gradient'), true),
      ],
    ],
    [
      'Luma',
      [
        row(l.language, choice('language', l.language)),
        row(l.appSorting, choice('app_sorting', l.appSorting)),
        row(l.reducedMotion, toggle('reduced_motion'), true),
        row(l.uiSounds, toggle('ui_sounds'), true),
      ],
    ],
  ];
  element('groups').replaceChildren(
    ...groups.map(([title, rows]) => {
      const group = document.createElement('section');
      group.className = 'group';
      const heading = document.createElement('h2');
      heading.textContent = title;
      const list = document.createElement('div');
      list.className = 'rows';
      list.append(...rows);
      group.append(heading, list);
      return group;
    }),
  );
  element('device').textContent = `Paired as ${state.device}`;
  show('settings');
}

function apply(result) {
  const focused = document.activeElement;
  // Rebuilding while someone types would drop their cursor; the reply only matters after.
  if (focused?.type === 'text' && focused.closest('#groups')) {
    state = result;
    return;
  }
  state = result;
  render();
}

function change(key, value) {
  const next = { ...state.settings, [key]: value };
  state.settings = next;
  // Changes go out one at a time, in order, so the TV ends on the last one.
  busy = busy.then(async () => {
    message('Saving…');
    try {
      apply(await call('api/settings', next));
      message('Saved on your TV.');
    } catch (error) {
      if (error instanceof PairingNeeded) return startPairing(error.message);
      message(error.message, true);
      load();
    }
  });
}

function startPairing(note = '') {
  show('pair');
  message(note, Boolean(note));
  element('pin').focus();
}

async function pair(pin) {
  element('pair-button').disabled = true;
  message('Pairing…');
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
    if (status.textContent === 'Connecting to your TV…') message('');
  } catch (error) {
    if (error instanceof PairingNeeded) {
      startPairing();
      return;
    }
    show('loading');
    message(error.message, true);
  }
}

element('pair').addEventListener('submit', (event) => {
  event.preventDefault();
  pair(element('pin').value);
});

element('forget').addEventListener('click', async () => {
  try {
    await call('api/forget', {});
    state = undefined;
    startPairing('This device is no longer paired.');
  } catch (error) {
    message(error.message, true);
  }
});

// Settings changed with the remote show up when the page comes back into view.
document.addEventListener('visibilitychange', () => {
  if (document.visibilityState === 'visible' && state) load();
});

const pin = new URLSearchParams(location.hash.slice(1)).get('pin');
// Keep the PIN out of bookmarks and history: the saved address is the plain page.
if (location.hash) history.replaceState(null, '', location.pathname);
if (pin) {
  element('pin').value = pin;
  // An already paired device just opens its settings.
  call('api/settings')
    .then(apply)
    .catch((error) => (error instanceof PairingNeeded ? pair(pin) : message(error.message, true)));
} else {
  load();
}
