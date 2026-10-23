import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import path from 'node:path';
import test from 'node:test';
import vm from 'node:vm';

const script = await fs.readFile(new URL('../tv/home.cjs', import.meta.url), 'utf8');
const app = 'com.ecklf.luma';
const stage = '/var/lib/luma-home/app';
const target = '/usr/palm/applications/com.webos.app.home';
const developer = `/media/developer/${app}/preferences.json`;
const shared = `/media/developer/temp/${app}/preferences.json`;
const internal = `/media/internal/.${app}/preferences.json`;

function fixture({ production = false, mounted = false, preferences = {} } = {}) {
  const appDir = `/media/${production ? 'cryptofs' : 'developer'}/apps/usr/palm/applications/${app}`;
  const files = new Map([
    [stage, Buffer.alloc(0)],
    [`${stage}/appinfo.json`, Buffer.from('{}')],
    [`${appDir}/luma`, Buffer.from('mock executable')],
    [`${appDir}/splash.png`, Buffer.from('default')],
    ...['dunes', 'forest', 'prism'].map((name) => [
      `${appDir}/splash/${name}.png`,
      Buffer.from(name),
    ]),
    ...Object.entries(preferences).map(([file, value]) => [
      file,
      Buffer.from(typeof value === 'string' ? value : JSON.stringify(value)),
    ]),
  ]);
  const writes = [];
  const logs = [];
  const missing = () => Object.assign(new Error('Missing'), { code: 'ENOENT' });
  const mockFs = {
    existsSync: (file) => files.has(file),
    readFileSync(file, encoding) {
      if (!files.has(file)) throw missing();
      const data = files.get(file);
      return encoding ? data.toString(encoding) : data;
    },
    writeFileSync(file, data) {
      writes.push(file);
      files.set(file, data);
    },
    renameSync(from, to) {
      assert.ok(files.has(from));
      files.set(to, files.get(from));
      files.delete(from);
    },
    statSync(file) {
      assert.ok([stage, target].includes(file));
      return { dev: 1, ino: mounted || file === stage ? 1 : 2 };
    },
    readlinkSync() {
      throw missing();
    },
  };
  const context = vm.createContext({
    require(name) {
      if (name === 'fs') return mockFs;
      if (name === 'path') return path.posix;
      assert.equal(name, 'child_process');
      const unexpected = () => assert.fail('Tests must not invoke any system or TV command');
      return { execFileSync: unexpected, spawnSync: unexpected };
    },
    process: { argv: ['node', 'home.cjs', 'status'] },
    console: { log: (line) => logs.push(line), error: (line) => logs.push(line) },
  });
  vm.runInContext(script, context, { timeout: 2000 });
  assert.equal(context.process.exitCode, undefined);
  return {
    context,
    files,
    writes,
    appDir,
    splash() {
      context.stageSplash(appDir);
      assert.ok(!files.has(`${stage}/splash.png.new`));
      return files.get(`${stage}/splash.png`).toString();
    },
  };
}

test('production and developer installs follow native preference precedence', () => {
  const preferences = {
    [developer]: { background: 'forest' },
    [shared]: { background: 'dunes' },
    [internal]: { background: 'prism' },
  };
  assert.equal(fixture({ production: true, preferences }).splash(), 'prism');
  assert.equal(fixture({ preferences }).splash(), 'forest');
});

test('an existing persistent destination wins before installation-local migration', () => {
  const persistent = fixture({ preferences: { [internal]: { background: 'prism' } } });
  persistent.files.set(
    `${persistent.appDir}/preferences.json`,
    Buffer.from(JSON.stringify({ background: 'forest' })),
  );
  assert.equal(persistent.splash(), 'prism');
  const legacy = fixture();
  legacy.files.set(
    `${legacy.appDir}/preferences.json`,
    Buffer.from(JSON.stringify({ background: 'forest' })),
  );
  assert.equal(legacy.splash(), 'forest');
});

test('missing, malformed, invalid and unavailable backgrounds have a default splash', () => {
  for (const saved of [undefined, '{', { background: '../forest' }, { background: 'unknown' }]) {
    const mock = fixture({ preferences: saved === undefined ? {} : { [developer]: saved } });
    assert.equal(mock.splash(), saved?.background === 'unknown' ? 'default' : 'dunes');
  }
});

test('apply refreshes the staged image even while the Home mount is active', () => {
  const mock = fixture({ mounted: true, preferences: { [developer]: { background: 'prism' } } });
  mock.files.set(`${stage}/splash.png`, Buffer.from('old background'));
  const calls = [];
  Object.assign(mock.context, {
    foreignMount: () => false,
    foreground: () => 'some.other.app',
    reread: () => calls.push('reread'),
    stopHome: () => calls.push('stop'),
    launchHome: () => assert.fail('Applying must not steal focus from another app'),
  });
  mock.context.apply({ appDir: mock.appDir }, false);
  assert.equal(mock.files.get(`${stage}/splash.png`).toString(), 'prism');
  assert.deepEqual(mock.writes, [`${stage}/splash.png.new`]);
  assert.deepEqual(calls, ['reread', 'stop']);
});
