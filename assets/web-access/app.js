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
    // Only the server's flag; the strings themselves have a "pair" label.
    if (result.pair === true) throw new PairingNeeded(result.error);
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
  // With the TV's remote interface every button works in any app; otherwise inside Luma only.
  const whole = state?.tvRemote === 'ready';
  for (const button of element('remote').querySelectorAll('.dpad button, .remote-side button')) {
    button.disabled = !previewOnScreen && !whole;
  }
  element('tv-keys').hidden = !whole;
  element('tv-text').hidden = !whole;
  element('tv-volume').hidden = !whole || !tvReading;
  element('tv-sleep').hidden = !whole;
  element('key-mute').textContent = state?.labels.mute ?? '';
  element('key-screen').textContent = tvReading?.screen_off
    ? (state?.labels.screenOn ?? '')
    : (state?.labels.screenOff ?? '');
  element('key-screen').title = state?.labels.screenOffHint ?? '';
  element('keyboard-hint').textContent = state?.labels.keyboardHint ?? '';
  showInputs();
  showSleep();
  if (whole && remoteOpen) watchTv();
  element('key-power').textContent = state?.labels.powerOff ?? '';
  element('tv-text-send').textContent = state?.labels.sendText ?? '';
  element('tv-text-field').placeholder = state?.labels.textPlaceholder ?? '';
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

/* ---------- Whole TV: inputs, volume, picture, sleep timer ---------- */

// The TV's inputs open like on Home; that works without the TV's remote interface too.
function showInputs() {
  const holder = element('tv-inputs');
  const inputs = state?.inputs ?? [];
  holder.hidden = !inputs.length;
  holder.replaceChildren(
    ...inputs.map((input) => {
      const button = create('button', 'pill-button input-chip');
      button.type = 'button';
      const mark = create('span', 'glyph');
      mark.style.setProperty('--icon', `url('icon/input-${input.icon || input.auto_icon}.svg')`);
      button.append(mark, create('span', '', input.name || input.port));
      button.addEventListener('click', () => launch(input.id));
      return button;
    }),
  );
}

// The TV's volume and picture state, read while the remote is open.
let tvReading = null;
let sleepLeft = null;
let tvWatch = null;
let volumeHeld = false;

async function tv(change = {}) {
  try {
    const result = await call('api/tv', change);
    tvReading = result.reading;
    sleepLeft = result.sleepLeft ?? null;
  } catch (error) {
    if (error instanceof PairingNeeded) return startPairing(error.message);
    tvReading = null;
  }
  if (!volumeHeld && tvReading) {
    const slider = element('volume');
    slider.max = String(tvReading.max_volume);
    slider.value = String(tvReading.volume);
    showVolume();
  }
  showRemote();
}

// Polls only while someone looks at the open remote of a TV that allows it.
function watchTv() {
  if (tvWatch) return;
  tv();
  tvWatch = setInterval(() => {
    const whole = state?.tvRemote === 'ready';
    if (!whole || !remoteOpen || document.visibilityState !== 'visible') {
      clearInterval(tvWatch);
      tvWatch = null;
      return;
    }
    if (!volumeHeld) tv();
  }, 5000);
}

function showVolume() {
  const slider = element('volume');
  const muted = tvReading?.muted;
  element('volume-value').textContent = muted ? '🔇' : slider.value;
  slider.setAttribute('aria-label', state?.labels.volume ?? '');
  slider.style.setProperty(
    '--fill',
    `${(Number(slider.value) / Number(slider.max || 100)) * 100}%`,
  );
}

let volumeTimer = null;
element('volume').addEventListener('input', () => {
  volumeHeld = true;
  showVolume();
  // Follows the finger without sending every step.
  clearTimeout(volumeTimer);
  volumeTimer = setTimeout(() => tv({ volume: Number(element('volume').value) }), 120);
});
element('volume').addEventListener('change', () => {
  clearTimeout(volumeTimer);
  tv({ volume: Number(element('volume').value) }).finally(() => (volumeHeld = false));
});
for (const key of ['volume_up', 'volume_down', 'mute']) {
  element('remote')
    .querySelector(`[data-key="${key}"]`)
    .addEventListener('click', () => setTimeout(() => tv(), 350));
}

element('key-screen').addEventListener('click', () => {
  navigator.vibrate?.(8);
  tv({ screen: Boolean(tvReading?.screen_off) });
});

const SLEEP_CHOICES = [0, 30, 60, 90];
function showSleep() {
  const l = state?.labels;
  if (!l) return;
  element('sleep-label').textContent = l.sleepTimer;
  const minutesLeft = sleepLeft === null ? null : Math.max(1, Math.ceil(sleepLeft / 60));
  // The running timer marks the choice it came from, rounded up to the next one.
  const current =
    minutesLeft === null ? 0 : (SLEEP_CHOICES.find((value) => value >= minutesLeft) ?? 90);
  const choices = element('sleep-choices');
  choices.setAttribute('aria-label', l.sleepTimer);
  choices.replaceChildren(
    ...SLEEP_CHOICES.map((value) => {
      const option = create('label');
      const radio = create('input');
      radio.type = 'radio';
      radio.name = 'sleep';
      radio.checked = value === current;
      radio.addEventListener('change', () => tv({ sleep: value }));
      option.append(
        radio,
        create('span', '', value ? l.minutes.replace('{minutes}', value) : l.sleepOff),
      );
      return option;
    }),
  );
  element('sleep-left').textContent =
    minutesLeft === null ? '' : l.sleepLeft.replace('{minutes}', minutesLeft);
}

// Turning the TV off from a phone deserves a second tap.
let powerArmed = false;
element('key-power').addEventListener('click', () => {
  if (!powerArmed) {
    powerArmed = true;
    element('key-power').textContent = `${state.labels.powerOff}?`;
    setTimeout(() => {
      powerArmed = false;
      showRemote();
    }, 4000);
    return;
  }
  powerArmed = false;
  press('power');
});

element('tv-text').addEventListener('submit', (event) => {
  event.preventDefault();
  const field = element('tv-text-field');
  if (!field.value) return;
  call('api/text', { text: field.value, enter: true })
    .then(() => {
      field.value = '';
      message(strings.saved);
    })
    .catch((error) => message(error.message, true));
});

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
    h: 'home',
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

// Free text saved when the field is left or Enter is pressed, like the TV's keyboard.
function textRow(key, label, placeholder = '') {
  const row = create('label', 'row stacked');
  const field = create('input', 'text-field');
  field.type = 'text';
  field.maxLength = 40;
  field.autocomplete = 'off';
  field.enterKeyHint = 'done';
  field.placeholder = placeholder;
  field.value = state.settings[key];
  field.setAttribute('aria-label', label);
  field.addEventListener('change', () => change(key, field.value.trim()));
  field.addEventListener('keydown', (event) => event.key === 'Enter' && field.blur());
  row.append(title(label), field);
  return row;
}

// The same switch and choice rows for a value that is not a plain setting (a widget's own).
function switchControl(label, checked, onChange) {
  const row = create('label', 'row inline');
  const holder = create('span', 'switch');
  const input = create('input');
  input.type = 'checkbox';
  input.role = 'switch';
  input.checked = checked;
  input.setAttribute('aria-label', label);
  input.addEventListener('change', () => onChange(input.checked));
  holder.append(input, create('span'));
  row.append(title(label), holder);
  return row;
}

function choiceControl(label, name, choices, value, onChange) {
  const current = choices.find((choice) => choice.value === value);
  if (choices.length > 3) {
    const row = create('label', 'row picker');
    const select = create('select');
    select.setAttribute('aria-label', label);
    for (const choice of choices) {
      const option = create('option', '', choice.label);
      option.value = JSON.stringify(choice.value);
      option.selected = choice === current;
      select.append(option);
    }
    select.addEventListener('change', () => onChange(JSON.parse(select.value)));
    row.append(title(label), create('span', 'value', current?.label ?? ''), select);
    return row;
  }
  const row = create('div', 'row stacked');
  const group = create('div', 'segments');
  group.role = 'radiogroup';
  group.setAttribute('aria-label', label);
  for (const choice of choices) {
    const option = create('label');
    const input = create('input');
    input.type = 'radio';
    input.name = name;
    input.checked = choice === current;
    input.addEventListener('change', () => onChange(choice.value));
    option.append(input, create('span', '', choice.label));
    group.append(option);
  }
  row.append(title(label), group);
  return row;
}

function choiceRow(key, label) {
  return state.choices[key].length > 3 ? pickerRow(key, label) : segmentRow(key, label);
}

function wallpapers() {
  const grid = create('div', 'wallpapers');
  for (const background of state.backgrounds) {
    if (pendingRemoval.has(background.id)) continue;
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
      remove.addEventListener('click', () => {
        const id = background.id;
        pendingRemoval.add(id);
        render();
        undoable(
          state.labels.photoDeleted,
          () => deletePhoto(id),
          () => {
            pendingRemoval.delete(id);
            render();
          },
        );
      });
      cell.append(remove);
    }
    grid.append(cell);
  }
  const add = create('label', 'wallpaper add-photo');
  const input = create('input');
  input.type = 'file';
  input.accept = 'image/jpeg,image/png,image/webp';
  input.multiple = true;
  input.addEventListener('change', () => {
    const files = [...input.files];
    input.value = '';
    // One photo gets the cropper; several are centred and cropped like the cropper's start.
    if (files.length === 1) openCropper(files[0]);
    else if (files.length > 1) uploadMany(files);
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

function pillButton(label, kind, action) {
  const button = create('button', `pill-button ${kind}`.trim(), label);
  button.type = 'button';
  button.addEventListener('click', action);
  return button;
}

/* ---------- Undo ---------- */

// Removing something takes effect after a few seconds; until then the toast can bring it back.
// Items waiting for removal stay out of the lists meanwhile.
const pendingRemoval = new Set();
let pendingUndo = null;

function undoable(text, commit, restore) {
  finishUndo();
  const timer = setTimeout(finishUndo, 6000);
  pendingUndo = { commit, restore, timer };
  element('undo-text').textContent = text;
  element('undo-button').textContent = state.labels.undo;
  element('undo').hidden = false;
}

function finishUndo() {
  const pending = pendingUndo;
  if (!pending) return;
  pendingUndo = null;
  clearTimeout(pending.timer);
  element('undo').hidden = true;
  pending.commit();
}

element('undo-button').addEventListener('click', () => {
  const pending = pendingUndo;
  if (!pending) return;
  pendingUndo = null;
  clearTimeout(pending.timer);
  element('undo').hidden = true;
  navigator.vibrate?.(8);
  pending.restore();
});

// Leaving the page carries out what was removed rather than forgetting it.
addEventListener('pagehide', finishUndo);
document.addEventListener('visibilitychange', () => {
  if (document.visibilityState === 'hidden') finishUndo();
});

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

  const devices = state.devices
    .filter((device) => !pendingRemoval.has(device.id))
    .map((device) => {
      const row = create('div', 'row inline');
      const name = create('span', 'title', device.name);
      if (device.this) name.append(create('small', 'badge', l.thisDevice));
      const removeNow = () => {
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
      };
      // This device signs itself out, which no undo can bring back; it asks first instead.
      // Another device goes after a few seconds, with time to undo.
      const remove = device.this
        ? confirmButton(l.remove, `${l.remove}?`, removeNow)
        : pillButton(l.remove, 'danger', () => {
            pendingRemoval.add(device.id);
            render();
            undoable(
              l.deviceRemoved,
              () => {
                removeNow();
                pendingRemoval.delete(device.id);
              },
              () => {
                pendingRemoval.delete(device.id);
                render();
              },
            );
          });
      row.append(name, remove);
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
    group(
      l.webAccess,
      toggleRow('tv_remote', l.tvRemote),
      create(
        'p',
        'hint',
        state.settings.tv_remote
          ? ({
              confirm: l.tvRemoteConfirm,
              ready: l.tvRemoteReady,
              denied: l.tvRemoteDenied,
              unavailable: l.tvRemoteUnavailable,
            }[state.tvRemote] ?? l.tvRemoteHint)
          : l.tvRemoteHint,
      ),
      ...devices,
    ),
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
      pendingRemoval.delete(id);
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
  if (result.pair === true) throw new PairingNeeded(result.error);
  if (!response.ok) throw new Error(result.error || strings.offline);
  return result;
}

// The 1920×1080 JPEG the TV stores, cut from a photo with the cropper's math.
async function cropped(bitmap, zoom = 1, horizontal = 0.5, vertical = 0.5) {
  const output = create('canvas');
  output.width = 1920;
  output.height = 1080;
  const { x, y, w, h } = cropRect(bitmap.width, bitmap.height, zoom, horizontal, vertical);
  output.getContext('2d').drawImage(bitmap, x, y, w, h, 0, 0, 1920, 1080);
  return new Promise((resolve) => output.toBlob(resolve, 'image/jpeg', 0.94));
}

// Several photos at once, one after the other, so the TV is never asked to store two at a time.
function uploadMany(files) {
  queue = queue.then(async () => {
    let added = 0;
    for (const [index, file] of files.entries()) {
      message(
        state.labels.uploadingCount.replace('{n}', index + 1).replace('{total}', files.length),
        false,
        true,
      );
      try {
        const bitmap = await createImageBitmap(file, { imageOrientation: 'from-image' });
        const blob = await cropped(bitmap);
        bitmap.close?.();
        apply(await uploadPhoto(blob));
        added += 1;
      } catch (error) {
        if (error instanceof PairingNeeded) return startPairing(error.message);
        message(error.message || strings.offline, true);
        return;
      }
    }
    message(state.labels.photosAdded.replace('{count}', added));
  });
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
    const blob = await cropped(bitmap, zoom, horizontal, vertical);
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

// Drags a favorite by its tile; a line shows where it would go, the drop saves the order.
function dragFavorite(event, id, row, dock, hold) {
  let target = null;
  const marks = () => dock.querySelectorAll('.drop-before, .drop-after');
  const unmark = () =>
    marks().forEach((other) => other.classList.remove('drop-before', 'drop-after'));
  dragAfterPress(event, row, {
    hold,
    move(dx, dy, pointer) {
      row.classList.add('dragging');
      row.style.transform = `translateY(${dy}px)`;
      const others = [...dock.querySelectorAll('.row.favorite')].filter((other) => other !== row);
      // The first row whose middle is below the finger gets the line above it.
      const below = others.find((other) => {
        const box = other.getBoundingClientRect();
        return pointer.clientY < box.top + box.height / 2;
      });
      unmark();
      target = below ?? null;
      if (below) below.classList.add('drop-before');
      else others.at(-1)?.classList.add('drop-after');
    },
    drop() {
      unmark();
      row.classList.remove('dragging');
      row.style.transform = '';
      const { favorites, hidden } = state.library;
      const next = favorites.filter((item) => item !== id);
      const before = target ? [...dock.children].indexOf(target) : -1;
      const beforeId = before >= 0 ? shownFavorites()[before] : null;
      next.splice(beforeId ? next.indexOf(beforeId) : next.length, 0, id);
      if (next.join() === favorites.join()) return render();
      navigator.vibrate?.(8);
      arrange(next, hidden);
    },
    cancel() {
      unmark();
      row.classList.remove('dragging');
      row.style.transform = '';
      render();
    },
  });
}

function shownFavorites() {
  const known = new Set(state.library.apps.map((app) => app.id));
  return state.library.favorites.filter((id) => known.has(id));
}

// Takes an app out of the favorites at once; the toast puts it back in its old place.
function unfavorite(id, name) {
  const { favorites, hidden } = state.library;
  const before = [...favorites];
  arrange(
    favorites.filter((item) => item !== id),
    hidden,
  );
  undoable(
    state.labels.favoriteRemoved.replace('{app}', name),
    () => {},
    () => arrange(before, state.library.hidden),
  );
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
      unfavorite(id, title(id)),
    );
    const open = iconButton('open', l.open, undefined, () => launch(id));
    const handle = thumb(index.get(id));
    // The whole row drags, not its buttons. A finger holds briefly first, so a swipe over the
    // list still scrolls; the tile itself drags at once.
    row.addEventListener('pointerdown', (event) => {
      if (event.target.closest('button')) return;
      const hold = event.pointerType === 'touch' && !event.target.closest('.thumb') ? 280 : 0;
      dragFavorite(event, id, row, dock, hold);
    });
    row.append(handle, create('span', 'title', title(id)), open, up, down, remove);
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
          favorite
            ? unfavorite(app.id, app.title)
            : arrange(
                [...favorites, app.id],
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
// The weather widget the search is for; results show under that widget only.
let placeTarget = 'weather';

function placeLabel(place) {
  return [place.name, place.region, place.country].filter(Boolean).join(', ');
}

async function searchPlaces(query, widget = 'weather') {
  placeTarget = widget;
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

function choosePlace(index, widget = 'weather') {
  places = [];
  placeQuery = '';
  placeNote = '';
  queue = queue.then(async () => {
    message(strings.saving, false, true);
    try {
      apply(await call('api/location', { index, widget }));
      message(strings.saved);
    } catch (error) {
      if (error instanceof PairingNeeded) return startPairing(error.message);
      message(error.message, true);
      load();
    }
  });
}

// The first weather widget by default; a further one passes its entry.
function locationRows(entry = null) {
  const l = state.labels;
  const widget = entry ? entry.widget : 'weather';
  const location = entry ? entry.weather.location : state.weatherLocation;
  const own = widgetKey(placeTarget) === widgetKey(widget);
  const rows = [];
  const current = create('div', 'row inline');
  current.append(create('span', 'title', location ? placeLabel(location) : l.setWeatherLocation));
  if (location) {
    const remove = create('button', 'pill-button', l.removeWeatherLocation);
    remove.type = 'button';
    remove.addEventListener('click', () => choosePlace(null, widget));
    current.append(remove);
  }
  rows.push(current);

  const search = create('form', 'row stacked place-search');
  const field = create('input', 'text-field');
  field.type = 'search';
  field.enterKeyHint = 'search';
  field.autocomplete = 'off';
  field.placeholder = l.cityPostcode;
  field.value = own ? placeQuery : '';
  field.setAttribute('aria-label', l.cityPostcode);
  const go = create('button', 'pill-button strong', l.searchLocations);
  go.type = 'submit';
  search.append(field, go);
  search.addEventListener('submit', (event) => {
    event.preventDefault();
    field.blur();
    searchPlaces(field.value.trim(), widget);
  });
  rows.push(search);
  if (!own) return rows;
  if (placeNote) rows.push(create('p', 'hint', placeNote));
  places.forEach((place, index) => {
    const option = create('button', 'row place');
    option.type = 'button';
    option.append(
      create('span', 'title', place.name),
      create('span', 'muted', [place.region, place.country].filter(Boolean).join(', ')),
    );
    option.addEventListener('click', () => choosePlace(index, widget));
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

// The Home grid as on the TV: drag a widget to a free spot, or pick it and then a spot.
// The TV's placement rules decide either way.
let selectedWidget = null;
// Set while a finger or mouse carries something, so no refresh rebuilds the page under it.
let dragging = false;
// A drag ends with a click on the dragged element; that click must not also select it.
let swallowClick = false;

// Follows a press until it travels a few pixels, then hands every move and the drop to the
// callbacks. A press that stays put remains a plain click.
function dragAfterPress(event, element, { move, drop, cancel, hold = 0 }) {
  if (event.button !== 0) return;
  const start = { x: event.clientX, y: event.clientY };
  let active = false;
  // With `hold`, a finger must rest that long before anything moves; moving sooner scrolls.
  let ready = hold === 0;
  let timer = null;
  const capture = () => {
    try {
      element.setPointerCapture(event.pointerId);
    } catch {
      // The pointer is already gone; the next up or cancel ends the drag.
    }
  };
  if (ready) capture();
  else {
    timer = setTimeout(() => {
      ready = true;
      holding = true;
      navigator.vibrate?.(10);
      element.classList.add('lifted');
      capture();
    }, hold);
  }
  const onMove = (next) => {
    const distance = Math.hypot(next.clientX - start.x, next.clientY - start.y);
    if (!ready) {
      if (distance > 8) finish(next, false);
      return;
    }
    if (!active) {
      if (distance < 6) return;
      active = true;
      dragging = true;
      element.classList.add('dragging');
    }
    move(next.clientX - start.x, next.clientY - start.y, next);
  };
  const finish = (end, dropped) => {
    clearTimeout(timer);
    holding = false;
    element.classList.remove('lifted');
    element.removeEventListener('pointermove', onMove);
    element.removeEventListener('pointerup', onUp);
    element.removeEventListener('pointercancel', onCancel);
    if (!active) return;
    dragging = false;
    swallowClick = true;
    setTimeout(() => {
      swallowClick = false;
    });
    element.classList.remove('dragging');
    if (dropped) drop(end);
    else cancel();
  };
  const onUp = (end) => finish(end, true);
  const onCancel = (end) => finish(end, false);
  element.addEventListener('pointermove', onMove);
  element.addEventListener('pointerup', onUp);
  element.addEventListener('pointercancel', onCancel);
}

// Once a finger has picked something up, the page must not scroll under it.
let holding = false;
document.addEventListener(
  'touchmove',
  (event) => {
    if (holding || dragging) event.preventDefault();
  },
  { passive: false },
);

// The grid cell a dragged widget's top-left corner lands on, kept inside the grid.
function cellUnder(board, entry, left, top) {
  const box = board.getBoundingClientRect();
  const style = getComputedStyle(board);
  const pad = parseFloat(style.paddingLeft);
  const gapX = parseFloat(style.columnGap) || 0;
  const gapY = parseFloat(style.rowGap) || 0;
  const { columns, rows } = state.grid;
  const pitchX = (box.width - 2 * pad + gapX) / columns;
  const pitchY = (box.height - 2 * pad + gapY) / rows;
  const clamp = (value, max) => Math.min(Math.max(value, 0), max);
  return {
    column: clamp(Math.round((left - box.left - pad) / pitchX), columns - entry.width),
    row: clamp(Math.round((top - box.top - pad) / pitchY), rows - entry.height),
  };
}

function dragWidget(event, entry, block, board) {
  const origin = block.getBoundingClientRect();
  const shadow = create('div', 'grid-drop');
  let target = null;
  const clear = () => {
    block.style.transform = '';
    shadow.remove();
  };
  dragAfterPress(event, block, {
    move(dx, dy) {
      block.style.transform = `translate(${dx}px, ${dy}px)`;
      target = cellUnder(board, entry, origin.left + dx, origin.top + dy);
      const same = target.column === entry.column && target.row === entry.row;
      const free = same || fits(entry.widget, target.column, target.row);
      shadow.classList.toggle('blocked', !free);
      shadow.style.gridArea = `${target.row + 1} / ${target.column + 1} / span ${entry.height} / span ${entry.width}`;
      if (!shadow.isConnected) board.append(shadow);
    },
    drop() {
      const moved = target && (target.column !== entry.column || target.row !== entry.row);
      if (moved && fits(entry.widget, target.column, target.row)) {
        navigator.vibrate?.(8);
        // Show it where it landed right away; the TV's answer confirms or corrects it.
        entry.column = target.column;
        entry.row = target.row;
        selectedWidget = null;
        render();
        changeWidgets({ action: 'move', widget: entry.widget, ...target });
      } else {
        clear();
        render();
      }
    },
    cancel() {
      clear();
      render();
    },
  });
}

// Widgets are "weather" or, for further ones of a kind, { extra_weather: 0 }.
function widgetKey(widget) {
  return JSON.stringify(widget);
}

function sameWidget(a, b) {
  return a !== null && b !== null && widgetKey(a) === widgetKey(b);
}

function widgetKind(widget) {
  return typeof widget === 'string' ? widget : Object.keys(widget)[0].replace('extra_', '');
}

// "Weather", then "Weather 2", "Weather 3" for further ones, as the TV numbers them.
function widgetName(entry) {
  const l = state.labels;
  const kind = widgetKind(entry.widget);
  const name = { greeting: l.greeting, crypto: l.crypto, stocks: l.stocks }[kind] ?? l.weather;
  // A not yet created next widget does not count; it only shows as "Add: Weather 2".
  const more = state.widgets.some(
    (other) =>
      other !== entry &&
      widgetKind(other.widget) === kind &&
      other.number > 1 &&
      (other.present || other.weather || other.market),
  );
  return entry.number > 1 || (more && entry.number === 1) ? `${name} ${entry.number}` : name;
}

// A further weather widget: the same settings as the first one, kept per widget.
function weatherRows(entry) {
  const l = state.labels;
  const c = state.choices;
  const config = entry.weather;
  const name = widgetKey(entry.widget);
  const send = (field, value) => {
    entry.weather = { ...config, [field]: value };
    const { unit, style, small, show_location, show_icon, icons } = entry.weather;
    if (field === 'show_icon') render();
    changeWidgets({
      action: 'weather',
      widget: entry.widget,
      unit,
      style,
      small,
      show_location,
      show_icon,
      icons,
    });
  };
  return [
    ...locationRows(entry),
    choiceControl(l.temperature, `unit-${name}`, c.temperature_unit, config.unit, (value) =>
      send('unit', value),
    ),
    choiceControl(l.weatherStyle, `style-${name}`, c.weather_style, config.style, (value) =>
      send('style', value),
    ),
    choiceControl(l.widgetSize, `size-${name}`, c.weather_small, config.small, (value) =>
      send('small', value),
    ),
    switchControl(l.weatherShowLocation, config.show_location, (value) =>
      send('show_location', value),
    ),
    switchControl(l.weatherShowIcon, config.show_icon, (value) => send('show_icon', value)),
    ...(config.show_icon
      ? [
          choiceControl(
            l.weatherIconStyle,
            `icons-${name}`,
            c.weather_icons,
            config.icons,
            (value) => send('icons', value),
          ),
        ]
      : []),
  ];
}

// A crypto or stocks widget: its symbol (checked by the TV) and currency.
function marketRows(entry) {
  const l = state.labels;
  const config = entry.market;
  const send = (symbol, currency) =>
    changeWidgets({ action: 'market', widget: entry.widget, symbol, currency });
  const row = create('label', 'row stacked');
  const field = create('input', 'text-field');
  field.type = 'text';
  field.maxLength = 32;
  field.autocomplete = 'off';
  field.autocapitalize = 'characters';
  field.enterKeyHint = 'done';
  field.value = config.symbol;
  field.setAttribute('aria-label', l.marketSymbol);
  field.addEventListener('change', () => {
    const symbol = field.value.trim();
    if (symbol) send(symbol, config.currency);
  });
  field.addEventListener('keydown', (event) => event.key === 'Enter' && field.blur());
  row.append(title(l.marketSymbol), field);
  return [
    row,
    choiceControl(
      l.currency,
      `currency-${widgetKey(entry.widget)}`,
      state.choices.currency,
      config.currency,
      (value) => send(config.symbol, value),
    ),
  ];
}

// Below the grid: every crypto and stocks widget, and weather widgets after the first.
function widgetCards() {
  return state.widgets
    .filter((entry) => (entry.weather && entry.number > 1) || entry.market)
    .map((entry) =>
      group(widgetName(entry), ...(entry.weather ? weatherRows(entry) : marketRows(entry))),
    );
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
  const item = state.widgets.find((entry) => sameWidget(entry.widget, widget));
  if (column + item.width > state.grid.columns || row + item.height > state.grid.rows) return false;
  return state.widgets
    .filter((other) => !sameWidget(other.widget, widget) && other.present)
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
  const selected = state.widgets.find((entry) => sameWidget(entry.widget, selectedWidget));
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
    const block = create('button', 'grid-widget', widgetName(entry));
    block.type = 'button';
    block.style.gridArea = `${entry.row + 1} / ${entry.column + 1} / span ${entry.height} / span ${entry.width}`;
    block.setAttribute('aria-pressed', String(sameWidget(entry.widget, selectedWidget)));
    block.addEventListener('pointerdown', (event) => dragWidget(event, entry, block, board));
    block.addEventListener('click', () => {
      if (swallowClick) return;
      selectedWidget = sameWidget(selectedWidget, entry.widget) ? null : entry.widget;
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
    const add = create('button', 'pill-button', `${l.addWidget}: ${widgetName(entry)}`);
    add.type = 'button';
    add.setAttribute('aria-pressed', String(sameWidget(entry.widget, selectedWidget)));
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
  wrap.append(board, create('p', 'hint', selected ? l.widgetPlace : l.widgetPick), actions);
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
        toggleRow('reduced_motion', l.reducedMotion),
        toggleRow('ui_sounds', l.uiSounds),
        choiceRow('recent_apps_behavior', l.recentApps),
      ),
      group(
        l.appDrawer,
        choiceRow('app_drawer_style', l.drawerStyle),
        choiceRow('app_sorting', l.appSorting),
        toggleRow('show_drawer_app_names', l.showAppNames),
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
      group(
        l.greeting,
        textRow('greeting_name', l.greetingName),
        toggleRow('show_date', l.date),
        choiceRow('date_format', l.dateFormat),
      ),
      group(
        l.weather,
        ...locationRows(),
        choiceRow('temperature_unit', l.temperature),
        choiceRow('weather_style', l.weatherStyle),
        choiceRow('weather_small', l.widgetSize),
        toggleRow('weather_show_location', l.weatherShowLocation),
        toggleRow('weather_show_icon', l.weatherShowIcon),
        ...(state.settings.weather_show_icon
          ? [choiceRow('weather_icons', l.weatherIconStyle)]
          : []),
      ),
      ...widgetCards(),
      group(l.shortcuts, ...tileRows()),
    ],
    system: systemSection(),
  };
}

// One row while every label fits, otherwise two even rows; never a bar that hides tabs.
function fitTabs() {
  const tabs = element('tabs');
  tabs.style.setProperty('--tab-columns', String(Math.ceil(tabs.children.length / 2)));
  tabs.classList.remove('two-rows');
  if (tabs.scrollWidth > tabs.clientWidth + 1) tabs.classList.add('two-rows');
}

addEventListener('resize', () => {
  if (state) fitTabs();
});
// The page font arrives after the first layout and is wider than the fallback.
document.fonts?.ready.then(() => {
  if (state) fitTabs();
});

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
  fitTabs();
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
  // A hidden bar has no width to measure, so check again once it shows.
  if (view === 'app' && state) fitTabs();
}

function apply(result) {
  state = result;
  strings = result.strings;
  const focused = document.activeElement;
  // Rebuilding while someone types would drop their cursor.
  if (focused?.classList.contains('text-field')) return;
  // Nor may it pull the piece out from under a finger mid-drag; the drop renders.
  if (dragging) return;
  render();
  show('app');
}

function change(key, value) {
  const next = { ...state.settings, [key]: value };
  state.settings = next;
  if (key === 'background' || key === 'weather_show_icon') render();
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
