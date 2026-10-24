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
  assert.deepEqual(calls, [`mount --bind ${appDir}/luma ${exe}`, 'mark', 'stop', 'launch']);
});

test('a boot apply does not take the screen from another app', () => {
  const { context, calls } = tv({ front: ['com.webos.app.livetv'] });
  context.applyExecutable({ method: 'executable', appDir }, false);
  assert.deepEqual(calls, [`mount --bind ${appDir}/luma ${exe}`, 'mark', 'stop']);
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
    'launch',
    'launch',
    'unmark',
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
    assert.deepEqual(calls, ['unmark', `umount ${exe}`, 'stop', ...(reopened ? ['launch'] : [])]);
  }
});
