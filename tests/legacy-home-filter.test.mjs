import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import test from 'node:test';
import vm from 'node:vm';

const script = await fs.readFile(new URL('../tv/legacy-home-filter.js', import.meta.url), 'utf8');

function television() {
  const calls = [];
  const subscriptions = {};
  const timers = [];
  let ribbon;
  const context = vm.createContext({
    JSON,
    console,
    KeyPolicy: { NextPolicy: 0, Accepted: 1, NotAccepted: 2 },
    Qt: { Key_Super_L: 100, Key_Menu: 101 },
    WebOS: { Key_webOS_Recent: 102, Key_webOS_Back: 103 },
    modalAlert: { visible: false },
    fullscreenView: { activeFocus: true },
    global: { hideHome: () => calls.push('hide'), goHome: () => calls.push('oem') },
    superRibbon: {
      visible: false,
      stateChanged: { connect() {}, disconnect() {} },
      visibleChanged: {
        connect(callback) {
          ribbon = callback;
        },
        disconnect(callback) {
          assert.equal(callback, ribbon);
          ribbon = undefined;
        },
      },
    },
    genericTimer: { set: (ms, callback) => timers.push([ms, callback]) },
    lunaCall: {
      send(service, method, payload, callback) {
        var request = JSON.parse(payload);
        calls.push(method === '/test/sendKeyCode' ? 'key:' + request.keyCode : request.id);
        callback(JSON.stringify({ returnValue: !context.failLaunch }));
      },
      subscribe(service, method, payload, callback) {
        subscriptions[method] = callback;
      },
    },
  });
  vm.runInContext(script, context);
  context.init();
  const emit = (method, payload) => subscriptions[method](JSON.stringify(payload));
  return {
    context,
    calls,
    front: (appId) => emit('/getForegroundAppInfo', { returnValue: true, appId }),
    life: (appId, status, reason = 'undefined') =>
      emit('/getAppLifeStatus', { appId, status, reason }),
    tick: () => timers.splice(0).forEach(([, callback]) => callback()),
    home: (pressed = true, repeat = false) => context.lumaLegacyHomeKey(100, pressed, repeat),
    ribbon() {
      context.superRibbon.visible = true;
      ribbon?.();
    },
    disable: () => emit('/getConfigs', { configs: { 'com.webos.surfacemanager.keyFilters': [] } }),
  };
}

test('Home consumes both edges, ignores repeats, and leaves Prime/Voice to their mapper', () => {
  const tv = television();
  assert.equal(tv.home(), 1);
  assert.equal(tv.home(false), 1);
  assert.equal(tv.home(true, true), 1);
  assert.deepEqual(tv.calls, ['hide', 'com.ecklf.luma']);
  for (const key of [1038, 1086]) assert.equal(tv.context.lumaLegacyHomeKey(key, true, false), 0);
});

test('an OEM ribbon request opens Luma; disabling disconnects it and restores Home keys', () => {
  const tv = television();
  tv.ribbon();
  assert.deepEqual(tv.calls, ['hide', 'com.ecklf.luma']);
  tv.tick();
  tv.disable();
  tv.ribbon();
  assert.equal(tv.home(), 0);
  assert.equal(tv.calls.length, 2);
});

test('Back reaches focused Luma while LG overlays and other apps keep their own policy', () => {
  const tv = television();
  const back = () => tv.context.lumaLegacyHomeKey(103, true, false);
  tv.front('com.ecklf.luma');
  assert.equal(back(), 2);
  tv.context.fullscreenView.activeFocus = false;
  assert.equal(back(), 0);
  tv.context.fullscreenView.activeFocus = true;
  tv.context.modalAlert.visible = true;
  assert.equal(back(), 0);
  tv.context.modalAlert.visible = false;
  tv.front('org.example.video');
  assert.equal(back(), 0);
});

test('closing the foreground app returns to Home over the fallback input', () => {
  const tv = television();
  tv.front('org.example.video');
  tv.life('org.example.video', 'closing');
  tv.front('com.webos.app.hdmi1');
  tv.life('org.example.video', 'stop');
  tv.tick();
  assert.deepEqual(tv.calls, ['hide', 'com.ecklf.luma']);
});

test('background cleanup and memory reclaim do not interrupt playback', () => {
  const tv = television();
  tv.front('org.example.video');
  tv.life('netflix', 'closing');
  tv.life('netflix', 'stop');
  tv.life('org.example.video', 'closing', 'memoryReclaim');
  tv.life('org.example.video', 'stop', 'memoryReclaim');
  tv.tick();
  assert.deepEqual(tv.calls, []);
});

test('switching to another app during a close keeps the selected app in front', () => {
  const tv = television();
  tv.front('netflix');
  tv.life('netflix', 'closing');
  tv.life('netflix', 'stop');
  tv.front('org.example.video');
  tv.tick();
  assert.deepEqual(tv.calls, []);
});

test('a failed Luma launch falls back to OEM Home without a ribbon loop', () => {
  const tv = television();
  tv.context.failLaunch = true;
  tv.home();
  tv.ribbon();
  tv.tick();
  assert.deepEqual(tv.calls, ['hide', 'com.ecklf.luma', 'oem']);
  assert.equal(tv.home(), 0);
});

test('Home resets the current Luma page only after its actual surface is focused', () => {
  const tv = television();
  tv.front('org.example.video');
  tv.home();
  tv.tick();
  assert.deepEqual(tv.calls, ['hide', 'com.ecklf.luma']);
  tv.context.global.activeSurfaceAppId = 'com.ecklf.luma';
  tv.front('com.ecklf.luma');
  tv.tick();
  assert.deepEqual(tv.calls, ['hide', 'com.ecklf.luma', 'key:102']);
  tv.tick();
  assert.equal(tv.calls.filter((call) => call === 'key:102').length, 1);
});

test('a pending Home reset cannot send a key after restoring LG routing', () => {
  const tv = television();
  tv.home();
  tv.context.global.activeSurfaceAppId = 'com.ecklf.luma';
  tv.front('com.ecklf.luma');
  tv.disable();
  tv.tick();
  assert.equal(tv.calls.includes('key:102'), false);
});

test('another Home press during launch debounce still resets a focused Luma page', () => {
  const tv = television();
  tv.context.lumaLegacyOpening = true;
  tv.context.global.activeSurfaceAppId = 'com.ecklf.luma';
  tv.front('com.ecklf.luma');
  tv.home();
  tv.tick();
  assert.deepEqual(tv.calls, ['key:102']);
});
