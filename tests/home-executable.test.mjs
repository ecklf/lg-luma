import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import path from 'node:path';
import test from 'node:test';
import vm from 'node:vm';

const script = await fs.readFile(new URL('../tv/home.cjs', import.meta.url), 'utf8');
const exe = '/usr/bin/com.webos.app.home';
const appDir = '/media/developer/apps/usr/palm/applications/com.ecklf.luma';
const marker = '/var/run/luma-home';
const lgHome = {
  id: 'com.webos.app.home',
  type: 'native',
  main: `file://${exe}`,
  nativeLifeCycleInterfaceVersion: 2,
};

// Runs home.cjs in a sandbox. Each test replaces the system-facing helpers it reaches.
function load() {
  const unexpected = () => assert.fail('Tests must not invoke any system or TV command');
  const context = vm.createContext({
    require(name) {
      if (name === 'fs') return { existsSync: () => false };
      if (name === 'path') return path.posix;
      assert.equal(name, 'child_process');
      return { execFileSync: unexpected, spawnSync: unexpected };
    },
    process: { argv: ['node', 'home.cjs', 'status'] },
    console: { log() {}, error() {} },
  });
  vm.runInContext(script, context, { timeout: 2000 });
  return context;
}

// A TV with Luma's executable mountable over LG Home's; `front` lists foreground replies.
function tv({ root = null, same = false, front = ['com.webos.app.home'] } = {}) {
  const context = load();
  const calls = [];
  const state = { root, same, fronts: [...front], marker: false };
  Object.assign(context, {
    fs: {
      existsSync: (file) => file === `${appDir}/luma` || (file === marker && state.marker),
      writeFileSync(file, data) {
        assert.equal(file, marker);
        assert.equal(String(data), 'com.ecklf.luma\n');
        calls.push('mark');
        state.marker = true;
      },
      unlinkSync(file) {
        assert.equal(file, marker);
        calls.push('unmark');
        state.marker = false;
      },
    },
    exeMount: () => state.root,
    exeMounted: () => state.same,
    run(file, args) {
      calls.push([path.posix.basename(file), ...args].join(' '));
      if (file === '/bin/mount') {
        state.root = '/developer/apps/usr/palm/applications/com.ecklf.luma/luma';
        state.same = true;
      }
      if (file === '/bin/umount') {
        state.root = null;
        state.same = false;
      }
      return '';
    },
    foreground: () => (state.fronts.length > 1 ? state.fronts.shift() : state.fronts[0]),
    stopHome: () => calls.push('stop'),
    lastInput: (command) => calls.push(command),
    launchHome: () => calls.push('launch'),
    pause() {},
  });
  return { context, calls, state };
}

test('webOS 5/6 with a native lifecycle-v2 LG Home uses the executable method', () => {
  const { homeMethod } = load();
  assert.equal(homeMethod('Rockhopper release 5.6.2-21 (jhericurl-jimna)', lgHome), 'executable');
  assert.equal(homeMethod('Rockhopper release 6.5.3-10', lgHome), 'executable');
  assert.equal(homeMethod('Rockhopper release 10.2.2-58', null), 'manifest');
  assert.equal(homeMethod('Rockhopper release 11.2.0-1', null), 'manifest');
  assert.throws(() => homeMethod('Rockhopper release 9.2.1', lgHome), /targets webOS/);
  assert.throws(
    () => homeMethod('Rockhopper release 5.6.2', { ...lgHome, nativeLifeCycleInterfaceVersion: 1 }),
    /targets webOS/,
  );
  assert.throws(
    () => homeMethod('Rockhopper release 6.1.0', { ...lgHome, type: 'web' }),
    /targets webOS/,
  );
});

test('only a mount of Luma executable counts as this integration, also after an update', () => {
  const { exeOwned } = load();
  assert.ok(exeOwned('/developer/apps/usr/palm/applications/com.ecklf.luma/luma'));
  assert.ok(exeOwned('/cryptofs/apps/usr/palm/applications/com.ecklf.luma/luma//deleted'));
  assert.ok(!exeOwned('/var/lib/other/launcher'));
  assert.ok(!exeOwned(null));
});

test('apply mounts the executable, stops the old Home and opens Luma', () => {
  const { context, calls } = tv({ front: ['some.app', 'com.webos.app.home'] });
  context.applyExecutable({ method: 'executable', appDir }, true);
  assert.deepEqual(calls, [`mount --bind ${appDir}/luma ${exe}`, 'mark', 'stop', 'pin', 'launch']);
});

test('a boot apply does not take the screen from another app', () => {
  const { context, calls } = tv({ front: ['com.webos.app.livetv'] });
  context.applyExecutable({ method: 'executable', appDir }, false);
  assert.deepEqual(calls, [`mount --bind ${appDir}/luma ${exe}`, 'mark', 'stop', 'pin']);
});

test('a boot apply opens Home over an empty screen, as lastinput no longer opens Live TV', () => {
  const { context, calls } = tv({ front: ['', 'com.webos.app.home'] });
  context.applyExecutable({ method: 'executable', appDir }, false);
  assert.deepEqual(calls, [`mount --bind ${appDir}/luma ${exe}`, 'mark', 'stop', 'pin', 'launch']);
});

test('after a package update the replaced file is unmounted before mounting the new one', () => {
  const { context, calls } = tv({
    root: '/developer/apps/usr/palm/applications/com.ecklf.luma/luma//deleted',
  });
  context.applyExecutable({ method: 'executable', appDir }, true);
  assert.deepEqual(calls, [
    `umount ${exe}`,
    `mount --bind ${appDir}/luma ${exe}`,
    'mark',
    'stop',
    'pin',
    'launch',
  ]);
});

test('another integration mounted over LG Home is left alone', () => {
  const { context, calls } = tv({ root: '/var/lib/other/launcher' });
  assert.throws(
    () => context.applyExecutable({ method: 'executable', appDir }, true),
    /Another integration/,
  );
  assert.deepEqual(calls, []);
});

test('Home is launched again once and LG Home restored when Luma never comes to the front', () => {
  const { context, calls, state } = tv({ front: ['com.webos.app.home', 'com.webos.app.livetv'] });
  assert.throws(
    () => context.applyExecutable({ method: 'executable', appDir }, true),
    /Home did not come to the front/,
  );
  assert.deepEqual(calls, [
    `mount --bind ${appDir}/luma ${exe}`,
    'mark',
    'stop',
    'pin',
    'launch',
    'launch',
    'unmark',
    'unpin',
    `umount ${exe}`,
    'stop',
  ]);
  assert.equal(state.root, null);
  assert.equal(state.marker, false);
});

test('restore unmounts, stops Luma and reopens LG Home only if Home was in front', () => {
  for (const [front, reopened] of [
    ['com.webos.app.home', true],
    ['youtube.leanback.v4', false],
  ]) {
    const { context, calls, state } = tv({
      root: '/developer/apps/usr/palm/applications/com.ecklf.luma/luma',
      same: true,
      front: [front],
    });
    state.marker = true;
    context.restore({ method: 'executable', appDir });
    assert.deepEqual(calls, [
      'unmark',
      'unpin',
      `umount ${exe}`,
      'stop',
      ...(reopened ? ['launch'] : []),
    ]);
  }
});

test('the early helper is a dynamic LS2 service only the key filter may call', () => {
  const { earlyFiles } = load();
  const files = earlyFiles();
  const ls2 = '/var/luna-service2';
  const name = 'com.webos.service.lumahome';
  assert.deepEqual(Object.keys(files).sort(), [
    `${ls2}/api-permissions.d/${name}.json`,
    `${ls2}/manifests.d/${name}.json`,
    `${ls2}/roles.d/${name}.json`,
    `${ls2}/services.d/${name}.service`,
  ]);
  assert.equal(
    files[`${ls2}/services.d/${name}.service`],
    `[D-BUS Service]\nName=${name}\nExec=/var/lib/luma-home/luma-early\nType=dynamic\n`,
  );
  const role = JSON.parse(files[`${ls2}/roles.d/${name}.json`]);
  // Not under /media/developer: ls-hubd would treat it as a developer-mode service.
  assert.equal(role.exeName, '/var/lib/luma-home/luma-early');
  assert.deepEqual(role.allowedNames, [name]);
  assert.deepEqual(JSON.parse(JSON.stringify(role.permissions)), [
    { service: name, inbound: ['com.webos.starfish.keyfilters'], outbound: [] },
  ]);
  // "all" is the only API group LG grants the key filter.
  assert.deepEqual(JSON.parse(files[`${ls2}/api-permissions.d/${name}.json`]), {
    all: [`${name}/prepare`],
  });
  const manifest = JSON.parse(files[`${ls2}/manifests.d/${name}.json`]);
  // ls-hubd ignores the whole manifest unless its version is x.y.z.
  assert.match(manifest.version, /^\d+\.\d+\.\d+$/);
  assert.deepEqual(
    [...manifest.serviceFiles, ...manifest.roleFiles, ...manifest.apiPermissionFiles].sort(),
    Object.keys(files)
      .filter((file) => !file.includes('/manifests.d/'))
      .sort(),
  );
});

test('the key filter is appended once to LG filter chain and removed again', () => {
  const { keyFilters } = load();
  const ours = { file: '/var/lib/luma-home/home-filter.js', handler: 'lumaHomeKey' };
  const lg = [
    { file: '/usr/lib/qml/filters/screensaver.js', handler: 'handleScreenSaver' },
    { file: '/usr/lib/qml/filters/magic.js', handler: 'handleMagic' },
  ];
  const plain = (value) => JSON.parse(JSON.stringify(value));
  assert.deepEqual(plain(keyFilters(lg, true)), [...lg, ours]);
  assert.deepEqual(plain(keyFilters([...lg, ours], true)), [...lg, ours]);
  assert.deepEqual(plain(keyFilters([ours, ...lg], false)), lg);
});

test('disable restores LG Home and removes the hook and helper even when LS2 cleanup fails', () => {
  const context = load();
  const removed = [];
  const steps = [];
  Object.assign(context, {
    fs: {
      unlinkSync: (file) => removed.push(file),
    },
    exists: () => true,
    hookOwned: () => true,
    writeJson: () => steps.push('save'),
    setKeyFilters() {
      throw new Error('config service unavailable');
    },
    run() {
      throw new Error('ls-control failed');
    },
    restore: () => steps.push('restore'),
  });
  context.disable({ enabled: true, method: 'executable', appDir });
  assert.deepEqual(steps, ['save', 'restore']);
  assert.ok(removed.includes('/var/lib/luma-home/luma-early'));
  assert.ok(removed.includes('/var/lib/webosbrew/init.d/64-luma-home'));
  assert.equal(removed.filter((file) => file.startsWith('/var/luna-service2/')).length, 4);
});
