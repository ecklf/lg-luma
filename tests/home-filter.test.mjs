import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import test from 'node:test';
import vm from 'node:vm';

const script = await fs.readFile(new URL('../tv/home-filter.js', import.meta.url), 'utf8');

// LG's compositor filter context: luna calls, a timer, and subscriptions the test drives.
function compositor(inputOverride = true) {
  const calls = [];
  const subscriptions = {};
  const prepares = [];
  const timers = [];
  const context = vm.createContext({
    JSON,
    console: { warn() {} },
    KeyPolicy: { NextPolicy: 0, Accepted: 1 },
    genericTimer: { set: (ms, fn) => timers.push([ms, fn]) },
    lunaCall: {
      send(service, method, payload, callback) {
        if (method === '/prepare') prepares.push(callback);
        else if (method === '/setSystemSettings')
          calls.push(`lastInputApp ${JSON.parse(payload).settings.lastInputApp}`);
        else calls.push(`${method} ${JSON.parse(payload).id}`);
      },
      subscribe(service, method, payload, callback) {
        subscriptions[method] = callback;
      },
    },
  });
  vm.runInContext(script, context);
  context.lumaHomeInputOverride = inputOverride;
  context.init();
  const emit = (method, value) => subscriptions[method](JSON.stringify(value));
  return {
    context,
    calls,
    prepares,
    timers,
    boot: (done) => emit('/getBootStatus', { returnValue: true, signals: { 'boot-done': done } }),
    front: (appId) => emit('/getForegroundAppInfo', { returnValue: true, appId }),
    lastInput: (id) =>
      emit('/getSystemSettings', { returnValue: true, settings: { lastInputApp: id } }),
    early: (ok) => prepares.at(-1)(JSON.stringify({ returnValue: ok, exitCode: ok ? 0 : 1 })),
  };
}

test('a cold boot asks luma-early to mount Luma and then opens Home', () => {
  const tv = compositor();
  tv.boot(false);
  tv.front('');
  assert.equal(tv.prepares.length, 1);
  assert.deepEqual(tv.calls, []);
  tv.early(true);
  assert.deepEqual(tv.calls, ['/launch com.webos.app.home']);
});

test('a running TV never takes the screen when the filter is reloaded', () => {
  const tv = compositor();
  tv.boot(true);
  tv.front('youtube.leanback.v4');
  assert.equal(tv.prepares.length, 0);
  assert.deepEqual(tv.calls, []);
});

test('luma-early is asked again on a timer until it answers', () => {
  const tv = compositor();
  tv.boot(false);
  tv.timers[0][1]();
  assert.equal(tv.prepares.length, 2);
  tv.early(true);
  tv.timers.find(([ms]) => ms === 1500)[1]();
  assert.equal(tv.prepares.length, 2);
});

test('a failed mount leaves the boot to the Homebrew hook', () => {
  const tv = compositor();
  tv.boot(false);
  tv.early(false);
  assert.deepEqual(tv.calls, []);
});

test('Home opens over Live TV, not over an app the owner opened, and at most twice', () => {
  const tv = compositor();
  tv.boot(false);
  tv.front('com.webos.app.livetv');
  tv.early(true);
  assert.deepEqual(tv.calls, ['/launch com.webos.app.home']);
  tv.timers.find(([ms]) => ms === 8000)[1]();
  assert.equal(tv.calls.length, 2);
  tv.timers
    .filter(([ms]) => ms === 8000)
    .at(-1)[1]();
  assert.equal(tv.calls.length, 2);

  const owner = compositor();
  owner.boot(false);
  owner.front('netflix');
  owner.early(true);
  assert.deepEqual(owner.calls, []);
});

test('Home opens when Live TV replaces the snapshot Home after the mount is ready', () => {
  const tv = compositor();
  tv.boot(false);
  tv.front('com.webos.app.home');
  tv.early(true);
  assert.deepEqual(tv.calls, []);
  tv.front('com.webos.app.livetv');
  assert.deepEqual(tv.calls, ['/launch com.webos.app.home']);
});

test('once Home was shown, later inputs and retries leave the owner alone', () => {
  const tv = compositor();
  tv.boot(false);
  tv.front('');
  tv.early(true);
  tv.front('com.webos.app.home');
  tv.front('com.webos.app.hdmi1');
  tv.timers.find(([ms]) => ms === 8000)[1]();
  assert.deepEqual(tv.calls, ['/launch com.webos.app.home']);
});

test('keys pass through unchanged', () => {
  const tv = compositor();
  assert.equal(tv.context.lumaHomeKey(1, true, false), 0);
});

test('an app in front points Back/Exit at Home, once', () => {
  const tv = compositor();
  tv.boot(true);
  tv.lastInput('com.webos.app.livetv');
  tv.front('youtube.leanback.v4');
  tv.front('netflix');
  assert.deepEqual(tv.calls, ['lastInputApp com.webos.app.home']);
});

test('an input in front keeps its own lastInputApp', () => {
  const tv = compositor();
  tv.boot(true);
  tv.front('com.webos.app.hdmi1');
  tv.lastInput('com.webos.app.hdmi1');
  assert.deepEqual(tv.calls, []);
});

test('lastInputApp is only written once the settings service has answered', () => {
  const tv = compositor();
  tv.boot(true);
  tv.front('youtube.leanback.v4');
  assert.deepEqual(tv.calls, []);
  tv.lastInput('com.webos.app.livetv');
  assert.deepEqual(tv.calls, ['lastInputApp com.webos.app.home']);
});

test('ordinary Home takeover never overrides Back/Exit', () => {
  const tv = compositor(false);
  tv.boot(true);
  tv.lastInput('com.webos.app.hdmi2');
  tv.front('netflix');
  assert.deepEqual(tv.calls, []);
});
