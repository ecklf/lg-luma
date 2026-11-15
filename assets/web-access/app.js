import { cropRect, panPosition } from './crop.js';

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
  showRemote();
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

/* ---------- Remote ---------- */

// Buttons, taps on the picture and keyboard keys go to Luma like the TV remote's presses.
let remoteOpen = false;
try {
  remoteOpen = localStorage.getItem('luma-remote') === '1';
} catch {
  // Remembering the remote is only a convenience.
}

function press(key) {
  navigator.vibrate?.(8);
  call('api/key', { key }).catch((error) => {
    if (error instanceof PairingNeeded) startPairing(error.message);
    else message(error.message, true);
  });
}

function showRemote() {
  element('remote').hidden = !remoteOpen;
  element('remote-toggle').setAttribute('aria-pressed', String(remoteOpen));
  element('remote-toggle').textContent = strings.remote ?? '';
  element('key-back').textContent = strings.back ?? '';
  element('key-home').textContent = strings.home ?? '';
  element('key-menu').textContent = strings.options ?? '';
  // Inside Luma only: while another app is in front these would not reach it.
  for (const button of element('remote').querySelectorAll('button')) {
    button.disabled = !previewOnScreen;
  }
  screen.classList.toggle('tappable', remoteOpen && previewOnScreen);
  screen.title = remoteOpen ? (strings.tapHint ?? '') : '';
}

element('remote-toggle').addEventListener('click', () => {
  remoteOpen = !remoteOpen;
  try {
    localStorage.setItem('luma-remote', remoteOpen ? '1' : '0');
  } catch {
    // Remembering the remote is only a convenience.
  }
  showRemote();
});

for (const button of element('remote').querySelectorAll('[data-key]')) {
  button.addEventListener('click', () => press(button.dataset.key));
}

// A tap on the picture clicks the same spot on the TV, like the Magic Remote's pointer.
previewImage.addEventListener('click', (event) => {
  if (!remoteOpen || !previewOnScreen) return;
  const box = previewImage.getBoundingClientRect();
  const x = ((event.clientX - box.left) / box.width) * 1920;
  const y = ((event.clientY - box.top) / box.height) * 1080;
  navigator.vibrate?.(8);
  call('api/tap', { x, y }).catch((error) => message(error.message, true));
});

// On a computer the keyboard works too, unless someone is typing in a field.
document.addEventListener('keydown', (event) => {
  if (!state || !remoteOpen || event.altKey || event.ctrlKey || event.metaKey) return;
  if (event.target.closest?.('input, select, textarea')) return;
  const key = {
    ArrowUp: 'up',
    ArrowDown: 'down',
    ArrowLeft: 'left',
    ArrowRight: 'right',
    Enter: 'ok',
    Backspace: 'back',
    Escape: 'back',
    Home: 'home',
  }[event.key];
  if (!key) return;
  event.preventDefault();
  press(key);
});

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

function wallpapers() {
  const grid = create('div', 'wallpapers');
  for (const background of state.backgrounds) {
    const cell = create('div', 'wallpaper-cell');
    const button = create('button', 'wallpaper');
    button.type = 'button';
    button.setAttribute('aria-pressed', String(background.id === state.settings.background));
    const image = create('img');
    image.loading = 'lazy';
    image.alt = '';
    image.src = `api/thumbnail/${encodeURIComponent(background.id)}`;
    button.append(image, create('span', '', background.name || strings.custom));
    button.addEventListener('click', () => change('background', background.id));
    cell.append(button);
    // The owner's own photos can be deleted, except the one showing.
    if (!background.name && background.id !== state.settings.background) {
      const remove = create('button', 'photo-delete');
      remove.type = 'button';
      remove.title = strings.deletePhoto;
      remove.setAttribute('aria-label', strings.deletePhoto);
      remove.textContent = '×';
      let armed = false;
      remove.addEventListener('click', () => {
        // A second press confirms, so a stray tap cannot delete a photo.
        if (!armed) {
          armed = true;
          remove.classList.add('armed');
          remove.textContent = strings.deletePhoto;
          return;
        }
        deletePhoto(background.id);
      });
      cell.append(remove);
    }
    grid.append(cell);
  }
  const add = create('label', 'wallpaper add-photo');
  const input = create('input');
  input.type = 'file';
  input.accept = 'image/jpeg,image/png,image/webp';
  input.addEventListener('change', () => {
    if (input.files[0]) openCropper(input.files[0]);
    input.value = '';
  });
  const plus = create('span', 'add-plus', '+');
  add.append(input, plus, create('span', '', strings.addPhoto));
  grid.append(add);
  return grid;
}

/* ---------- System ---------- */

function runAction(action) {
  queue = queue.then(async () => {
    try {
      apply(await call('api/action', { action }));
    } catch (error) {
      if (error instanceof PairingNeeded) return startPairing(error.message);
      message(error.message, true);
    }
  });
}

function confirmButton(label, confirm, action) {
  const button = create('button', 'pill-button danger', label);
  button.type = 'button';
  let armed = false;
  button.addEventListener('click', () => {
    // A second press confirms.
    if (!armed) {
      armed = true;
      button.textContent = confirm;
      return;
    }
    action();
  });
  return button;
}

function systemSection() {
  const l = state.labels;
  const system = state.system;
  const update = create('div', 'row stacked update-card');
  update.append(
    create('span', 'title', system.update_title),
    create('span', 'muted', system.update_detail),
  );
  const updateButton = create('button', 'pill-button strong', system.update_action);
  updateButton.type = 'button';
  updateButton.disabled = system.update_busy;
  updateButton.addEventListener('click', () => runAction('update'));
  update.append(updateButton);

  const action = (label, name, busy) => {
    const row = create('button', 'row action-row', label);
    row.type = 'button';
    row.disabled = busy;
    row.addEventListener('click', () => {
      row.disabled = true;
      runAction(name);
      message(strings.saved);
    });
    return row;
  };

  const devices = state.devices.map((device) => {
    const row = create('div', 'row inline');
    const name = create('span', 'title', device.name);
    if (device.this) name.append(create('small', 'badge', l.thisDevice));
    row.append(
      name,
      confirmButton(l.remove, `${l.remove}?`, () => {
        queue = queue.then(async () => {
          try {
            apply(await call('api/devices/remove', { id: device.id }));
            message(strings.saved);
          } catch (error) {
            if (error instanceof PairingNeeded) {
              state = undefined;
              return startPairing(strings.unpaired);
            }
            message(error.message, true);
          }
        });
      }),
    );
    return row;
  });

  return [
    group(null, update),
    group(
      null,
      action(system.refreshing ? l.refreshNotice : l.refreshApps, 'refresh', system.refreshing),
      action(l.resetIconCache, 'reset_icons', false),
      choiceRow('text_rendering', l.textRendering),
    ),
    group(l.webAccess, ...devices),
  ];
}

// While System is open, follow update checks and refreshes as they run on the TV.
setInterval(() => {
  if (state && tab === 'system' && document.visibilityState === 'visible') {
    call('api/settings')
      .then(apply)
      .catch(() => {});
  }
}, 3000);

/* ---------- Own photos ---------- */

function deletePhoto(id) {
  queue = queue.then(async () => {
    message(strings.saving, false, true);
    try {
      apply(await call('api/photo/delete', { id }));
      message(strings.saved);
    } catch (error) {
      if (error instanceof PairingNeeded) return startPairing(error.message);
      message(error.message, true);
    }
  });
}

async function uploadPhoto(blob) {
  const response = await fetch('api/photo', {
    method: 'POST',
    credentials: 'same-origin',
    headers: { 'Content-Type': 'image/jpeg', 'X-Luma-Request': '1' },
    body: blob,
  });
  const result = await response.json().catch(() => ({}));
  if (result.pair) throw new PairingNeeded(result.error);
  if (!response.ok) throw new Error(result.error || strings.offline);
  return result;
}

// Crops to the TV's 1920×1080 like the wallpaper upload, with the same crop math.
async function openCropper(file) {
  let bitmap;
  try {
    bitmap = await createImageBitmap(file, { imageOrientation: 'from-image' });
  } catch {
    message(strings.offline, true);
    return;
  }
  let zoom = 1;
  let horizontal = 0.5;
  let vertical = 0.5;
  const overlay = create('div', 'cropper');
  const sheet = create('div', 'cropper-sheet');
  const canvas = create('canvas', 'crop-canvas');
  canvas.width = 960;
  canvas.height = 540;
  const context = canvas.getContext('2d');
  const draw = () => {
    const { x, y, w, h } = cropRect(bitmap.width, bitmap.height, zoom, horizontal, vertical);
    context.drawImage(bitmap, x, y, w, h, 0, 0, canvas.width, canvas.height);
  };
  let drag = null;
  canvas.addEventListener('pointerdown', (event) => {
    drag = { x: event.clientX, y: event.clientY };
    canvas.setPointerCapture(event.pointerId);
  });
  canvas.addEventListener('pointermove', (event) => {
    if (!drag) return;
    const crop = cropRect(bitmap.width, bitmap.height, zoom, horizontal, vertical);
    const box = canvas.getBoundingClientRect();
    horizontal = panPosition(horizontal, event.clientX - drag.x, bitmap.width, crop.w, box.width);
    vertical = panPosition(vertical, event.clientY - drag.y, bitmap.height, crop.h, box.height);
    drag = { x: event.clientX, y: event.clientY };
    draw();
  });
  for (const end of ['pointerup', 'pointercancel'])
    canvas.addEventListener(end, () => (drag = null));
  const slider = create('input', 'zoom');
  Object.assign(slider, { type: 'range', min: '1', max: '3', step: '0.01', value: '1' });
  slider.setAttribute('aria-label', strings.cropHint);
  slider.addEventListener('input', () => {
    zoom = Number(slider.value);
    draw();
  });
  const close = () => {
    overlay.remove();
    bitmap.close?.();
  };
  const actions = create('div', 'tile-actions');
  const cancel = create('button', 'pill-button', state.labels.cancel);
  cancel.type = 'button';
  cancel.addEventListener('click', close);
  const save = create('button', 'pill-button strong', state.labels.save);
  save.type = 'button';
  save.addEventListener('click', async () => {
    save.disabled = cancel.disabled = true;
    message(strings.uploading, false, true);
    const output = create('canvas');
    output.width = 1920;
    output.height = 1080;
    const { x, y, w, h } = cropRect(bitmap.width, bitmap.height, zoom, horizontal, vertical);
    output.getContext('2d').drawImage(bitmap, x, y, w, h, 0, 0, 1920, 1080);
    const blob = await new Promise((resolve) => output.toBlob(resolve, 'image/jpeg', 0.94));
    try {
      const result = await uploadPhoto(blob);
      apply(result);
      close();
      // Show the new photo right away, like choosing it in the gallery.
      change('background', result.id);
    } catch (error) {
      if (error instanceof PairingNeeded) return startPairing(error.message);
      message(error.message, true);
      save.disabled = cancel.disabled = false;
    }
  });
  actions.append(cancel, save);
  sheet.append(
    create('h2', 'crop-title', strings.cropTitle),
    canvas,
    slider,
    create('p', 'hint', strings.cropHint),
    actions,
  );
  overlay.append(sheet);
  document.body.append(overlay);
  draw();
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

function launch(id) {
  navigator.vibrate?.(8);
  call('api/launch', { id }).catch((error) => message(error.message, true));
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
    const open = iconButton('open', l.open, undefined, () => launch(id));
    row.append(thumb(index.get(id)), create('span', 'title', title(id)), open, up, down, remove);
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
      iconButton('open', l.open, undefined, () => launch(app.id)),
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

/* ---------- Weather location ---------- */

let places = [];
let placeQuery = '';
let placeNote = '';

function placeLabel(place) {
  return [place.name, place.region, place.country].filter(Boolean).join(', ');
}

async function searchPlaces(query) {
  placeQuery = query;
  placeNote = state.labels.searching;
  places = [];
  render();
  try {
    const result = await call('api/places', { query });
    places = result.places;
    placeNote = places.length ? '' : state.labels.noLocations;
  } catch (error) {
    if (error instanceof PairingNeeded) return startPairing(error.message);
    placeNote = error.message;
  }
  render();
}

function choosePlace(index) {
  places = [];
  placeQuery = '';
  placeNote = '';
  queue = queue.then(async () => {
    message(strings.saving, false, true);
    try {
      apply(await call('api/location', { index }));
      message(strings.saved);
    } catch (error) {
      if (error instanceof PairingNeeded) return startPairing(error.message);
      message(error.message, true);
      load();
    }
  });
}

function locationRows() {
  const l = state.labels;
  const rows = [];
  const current = create('div', 'row inline');
  current.append(
    create(
      'span',
      'title',
      state.weatherLocation ? placeLabel(state.weatherLocation) : l.setWeatherLocation,
    ),
  );
  if (state.weatherLocation) {
    const remove = create('button', 'pill-button', l.removeWeatherLocation);
    remove.type = 'button';
    remove.addEventListener('click', () => choosePlace(null));
    current.append(remove);
  }
  rows.push(current);

  const search = create('form', 'row stacked place-search');
  const field = create('input', 'text-field');
  field.type = 'search';
  field.enterKeyHint = 'search';
  field.autocomplete = 'off';
  field.placeholder = l.cityPostcode;
  field.value = placeQuery;
  field.setAttribute('aria-label', l.cityPostcode);
  const go = create('button', 'pill-button strong', l.searchLocations);
  go.type = 'submit';
  search.append(field, go);
  search.addEventListener('submit', (event) => {
    event.preventDefault();
    field.blur();
    searchPlaces(field.value.trim());
  });
  rows.push(search);
  if (placeNote) rows.push(create('p', 'hint', placeNote));
  places.forEach((place, index) => {
    const option = create('button', 'row place');
    option.type = 'button';
    option.append(
      create('span', 'title', place.name),
      create('span', 'muted', [place.region, place.country].filter(Boolean).join(', ')),
    );
    option.addEventListener('click', () => choosePlace(index));
    rows.push(option);
  });
  return rows;
}

/* ---------- Shortcuts ---------- */

// The tile being edited: an existing tile's id, 'new', or none.
let editingTile = null;
let tileDraft = null;

function tileGlyph(icon) {
  const mark = create('span', 'input-mark');
  mark.style.setProperty('--icon', `url('icon/tile-${icon || state.tileIcons[0].id}.svg')`);
  return mark;
}

// An address typed without a scheme defaults to http for local automation.
function normalizeUrl(text) {
  const url = text.trim();
  return url && !url.includes('://') ? `http://${url}` : url;
}

async function tileRequest(route, body, done) {
  queue = queue.then(async () => {
    message(strings.saving, false, true);
    try {
      const result = await call(route, body);
      if (route.endsWith('send')) {
        message(done);
        return;
      }
      editingTile = null;
      tileDraft = null;
      apply(result);
      message(done);
    } catch (error) {
      if (error instanceof PairingNeeded) return startPairing(error.message);
      message(error.message, true);
    }
  });
}

function tileEditor() {
  const l = state.labels;
  const form = create('form', 'row stacked tile-editor');
  const field = (label, value, attributes) => {
    const wrap = create('label', 'field');
    const input = create('input', 'text-field');
    Object.assign(input, { type: 'text', autocomplete: 'off', value }, attributes);
    wrap.append(create('span', 'muted', label), input);
    return [wrap, input];
  };
  const [titleField, title] = field(l.tileName, tileDraft.title, {
    maxLength: 40,
    placeholder: l.tileNameHint,
  });
  const [urlField, url] = field(l.tileUrl, tileDraft.url, {
    maxLength: 512,
    inputMode: 'url',
    autocapitalize: 'off',
    spellcheck: false,
    placeholder: l.tileUrlHint,
  });
  title.addEventListener('input', () => (tileDraft.title = title.value));
  url.addEventListener('input', () => (tileDraft.url = url.value));

  const methods = create('div', 'segments');
  for (const method of ['POST', 'GET']) {
    const option = create('label');
    const radio = create('input');
    radio.type = 'radio';
    radio.name = 'tile-method';
    radio.checked = tileDraft.method === method;
    radio.addEventListener('change', () => (tileDraft.method = method));
    option.append(radio, create('span', '', method));
    methods.append(option);
  }

  const icons = create('div', 'tile-icons');
  for (const icon of state.tileIcons) {
    const choice = create('button', 'tile-icon');
    choice.type = 'button';
    choice.title = icon.name;
    choice.setAttribute('aria-label', icon.name);
    choice.setAttribute(
      'aria-pressed',
      String((tileDraft.icon || state.tileIcons[0].id) === icon.id),
    );
    choice.append(tileGlyph(icon.id));
    choice.addEventListener('click', () => {
      tileDraft.icon = icon.id;
      for (const other of icons.children)
        other.setAttribute('aria-pressed', String(other === choice));
    });
    icons.append(choice);
  }

  const draft = () => ({
    id: editingTile === 'new' ? '' : editingTile,
    title: title.value.trim(),
    url: normalizeUrl(url.value),
    method: tileDraft.method,
    icon: tileDraft.icon,
  });
  const actions = create('div', 'tile-actions');
  const cancel = create('button', 'pill-button', l.cancel);
  cancel.type = 'button';
  cancel.addEventListener('click', () => {
    editingTile = null;
    tileDraft = null;
    render();
  });
  const save = create('button', 'pill-button strong', l.save);
  save.type = 'submit';
  actions.append(cancel, save);

  const extra = create('div', 'tile-actions');
  const test = create('button', 'pill-button', l.testShortcut);
  test.type = 'button';
  test.addEventListener('click', () => {
    const tile = draft();
    if (!tile.title || !tile.url) return message(l.tileIncomplete, true);
    tileRequest('api/tiles/send', tile, l.tileSent.replace('{tile}', tile.title));
  });
  extra.append(test);
  if (editingTile !== 'new') {
    const remove = create('button', 'pill-button danger', l.removeTile);
    remove.type = 'button';
    let armed = false;
    remove.addEventListener('click', () => {
      // A second press confirms, so a stray tap cannot remove a shortcut.
      if (!armed) {
        armed = true;
        remove.textContent = `${l.removeTile}?`;
        return;
      }
      tileRequest('api/tiles/remove', draft(), strings.saved);
    });
    extra.append(remove);
  }

  form.addEventListener('submit', (event) => {
    event.preventDefault();
    const tile = draft();
    if (!tile.title || !tile.url) return message(l.tileIncomplete, true);
    tileRequest('api/tiles/save', tile, strings.saved);
  });
  form.append(
    titleField,
    urlField,
    create('span', 'muted field-label', l.requestMethod),
    methods,
    create('span', 'muted field-label', l.icon),
    icons,
    actions,
    extra,
  );
  return form;
}

function tileRows() {
  const l = state.labels;
  const rows = [];
  if (!state.tiles.length && editingTile !== 'new') rows.push(create('p', 'hint', l.noTiles));
  for (const tile of state.tiles) {
    if (editingTile === tile.id) {
      rows.push(tileEditor());
      continue;
    }
    const row = create('button', 'row tile-row');
    row.type = 'button';
    const text = create('span', 'tile-text');
    text.append(
      create('span', 'title', tile.title),
      create('span', 'muted', `${tile.method} · ${tile.url}`),
    );
    row.append(tileGlyph(tile.icon), text);
    row.addEventListener('click', () => {
      editingTile = tile.id;
      tileDraft = { ...tile, icon: tile.icon || state.tileIcons[0].id };
      render();
    });
    rows.push(row);
  }
  if (editingTile === 'new') rows.push(tileEditor());
  else if (state.tiles.length < state.tileLimit) {
    const add = create('button', 'pill-button add-tile', l.addTile);
    add.type = 'button';
    add.addEventListener('click', () => {
      editingTile = 'new';
      tileDraft = { title: '', url: '', method: 'POST', icon: state.tileIcons[0].id };
      render();
    });
    rows.push(add);
  }
  return rows;
}

/* ---------- Widget layout ---------- */

// The Home grid as on the TV: pick a widget, then an empty spot; the TV's placement rules decide.
let selectedWidget = null;

function widgetName(widget) {
  return widget === 'greeting' ? state.labels.greeting : state.labels.weather;
}

function changeWidgets(change) {
  queue = queue.then(async () => {
    message(strings.saving, false, true);
    try {
      apply(await call('api/widgets', change));
      message(strings.saved);
    } catch (error) {
      if (error instanceof PairingNeeded) return startPairing(error.message);
      message(error.message, true);
    }
  });
}

function fits(widget, column, row) {
  const item = state.widgets.find((entry) => entry.widget === widget);
  if (column + item.width > state.grid.columns || row + item.height > state.grid.rows) return false;
  return state.widgets
    .filter((other) => other.widget !== widget && other.present)
    .every(
      (other) =>
        column >= other.column + other.width ||
        other.column >= column + item.width ||
        row >= other.row + other.height ||
        other.row >= row + item.height,
    );
}

function widgetGrid() {
  const l = state.labels;
  const { columns, rows } = state.grid;
  const wrap = create('div', 'row stacked widget-editor');
  const board = create('div', 'widget-grid');
  board.style.gridTemplateColumns = `repeat(${columns}, 1fr)`;
  board.style.gridTemplateRows = `repeat(${rows}, 1fr)`;
  const selected = state.widgets.find((entry) => entry.widget === selectedWidget);
  for (let cell = 0; cell < columns * rows; cell++) {
    const column = cell % columns;
    const row = Math.floor(cell / columns);
    const spot = create('button', 'grid-cell');
    spot.type = 'button';
    spot.style.gridArea = `${row + 1} / ${column + 1}`;
    const allowed = selected && fits(selected.widget, column, row);
    spot.disabled = !selected || !allowed;
    spot.classList.toggle('allowed', Boolean(allowed));
    spot.setAttribute('aria-label', `${l.widgetPosition} ${column + 1}, ${row + 1}`);
    spot.addEventListener('click', () => {
      const widget = selected.widget;
      selectedWidget = null;
      changeWidgets({ action: selected.present ? 'move' : 'add', widget, column, row });
    });
    board.append(spot);
  }
  for (const entry of state.widgets.filter((item) => item.present)) {
    const block = create('button', 'grid-widget', widgetName(entry.widget));
    block.type = 'button';
    block.style.gridArea = `${entry.row + 1} / ${entry.column + 1} / span ${entry.height} / span ${entry.width}`;
    block.setAttribute('aria-pressed', String(entry.widget === selectedWidget));
    block.addEventListener('click', () => {
      selectedWidget = selectedWidget === entry.widget ? null : entry.widget;
      render();
    });
    board.append(block);
  }
  const actions = create('div', 'tile-actions widget-actions');
  if (selected?.present) {
    const remove = create('button', 'pill-button danger', l.removeWidget);
    remove.type = 'button';
    remove.addEventListener('click', () => {
      selectedWidget = null;
      changeWidgets({ action: 'remove', widget: selected.widget });
    });
    actions.append(remove);
  }
  for (const entry of state.widgets.filter((item) => !item.present)) {
    const add = create('button', 'pill-button', `${l.addWidget}: ${widgetName(entry.widget)}`);
    add.type = 'button';
    add.setAttribute('aria-pressed', String(entry.widget === selectedWidget));
    add.addEventListener('click', () => {
      selectedWidget = entry.widget;
      render();
    });
    actions.append(add);
  }
  const reset = create('button', 'pill-button', l.resetWidgets);
  reset.type = 'button';
  reset.addEventListener('click', () => {
    selectedWidget = null;
    changeWidgets({ action: 'reset' });
  });
  actions.append(reset);
  wrap.append(board, create('p', 'hint', selected ? l.widgetPlacementHelp : l.move), actions);
  return wrap;
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
    ...(state.inputs.length
      ? { inputs: state.inputs.map(inputCard).map((card) => group(null, card)) }
      : {}),
    background: [
      group(null, wallpapers()),
      group(
        null,
        toggleRow('wallpaper_gradient', l.wallpaperGradient),
        toggleRow('wallpaper_cycle', l.wallpaperCycle),
      ),
    ],
    widgets: [
      group(l.widgetPosition, widgetGrid()),
      group(l.greeting, toggleRow('show_date', l.date), choiceRow('date_format', l.dateFormat)),
      group(
        l.weather,
        ...locationRows(),
        choiceRow('temperature_unit', l.temperature),
        choiceRow('weather_style', l.weatherStyle),
      ),
      group(l.shortcuts, ...tileRows()),
    ],
    system: systemSection(),
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
  showRemote();
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
