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

function fixture({
  production = false,
  mounted = false,
  preferences = {},
  owners = {},
  symlinks = [],
} = {}) {
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
    lstatSync(directory) {
      if (!Array.from(files.keys()).some((file) => path.posix.dirname(file) === directory)) {
        throw missing();
      }
      return {
        uid: owners[directory] ?? 5000,
        isDirectory: () => !symlinks.includes(directory),
      };
    },
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

test('automatic rotation cannot replace the manual splash during boot restaging', () => {
  for (const production of [false, true]) {
    const preferences = {
      [internal]: { background: 'prism', splash_background: 'forest', wallpaper_cycle: true },
    };
    assert.equal(fixture({ production, preferences }).splash(), 'forest');
    preferences[internal].background = 'dunes';
    assert.equal(fixture({ production, preferences }).splash(), 'forest');
  }
});

test('root Home stages app-owned preferences before a root-owned takeover fallback', () => {
  const preferences = {
    [developer]: { background: 'forest' },
    [internal]: { background: 'prism' },
  };
  const production = fixture({
    production: true,
    preferences,
    owners: { [path.posix.dirname(internal)]: 0 },
  });
  assert.equal(production.splash(), 'forest');
  const development = fixture({
    preferences,
    owners: { [path.posix.dirname(developer)]: 0 },
  });
  assert.equal(development.splash(), 'prism');
  assert.equal(JSON.parse(production.files.get(internal)).background, 'prism');
  assert.equal(JSON.parse(development.files.get(developer)).background, 'forest');
});

test('splash selection rejects symlink storage directories just like native startup', () => {
  const mock = fixture({
    preferences: {
      [developer]: { background: 'forest' },
      [shared]: { background: 'prism' },
    },
    symlinks: [path.posix.dirname(developer)],
  });
  assert.equal(mock.splash(), 'prism');
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

function closeFixture({ running, closeReply = { returnValue: true } }) {
  const mock = fixture();
  const calls = [];
  let queries = 0;
  let pauses = 0;
  Object.assign(mock.context, {
    luna(method, payload) {
      calls.push([method, { ...payload }]);
      if (method === 'running') {
        return { returnValue: true, running: running[Math.min(queries++, running.length - 1)] };
      }
      // The old close({id: APP}) call causes "no app matched by pid" on TV.
      assert.equal(method, 'closeByAppId');
      assert.equal(payload.id, app);
      if (closeReply instanceof Error) throw closeReply;
      return closeReply;
    },
    pause() {
      pauses++;
    },
  });
  return { ...mock, calls, pauses: () => pauses };
}

test('takeover closes the normal native instance by app ID and waits for its exit', () => {
  const normal = { id: app, processId: '1234' };
  const other = { id: 'some.other.app', processId: '5678' };
  const mock = closeFixture({ running: [[normal, other], [normal, other], [other]] });
  mock.context.closeNormal();
  assert.deepEqual(mock.calls, [
    ['running', {}],
    ['closeByAppId', { id: app }],
    ['running', {}],
    ['running', {}],
  ]);
  assert.equal(mock.pauses(), 1);
});

test('takeover skips closing when the normal instance is not running', () => {
  const mock = closeFixture({ running: [[{ id: 'com.webos.app.home' }]] });
  mock.context.closeNormal();
  assert.deepEqual(mock.calls, [['running', {}]]);
  assert.equal(mock.pauses(), 0);
});

test('an instance exiting before the close reply does not block takeover', () => {
  for (const closeReply of [
    { returnValue: false, errorCode: 1, errorText: 'no app matched by pid' },
    new Error('Luna close request timed out'),
  ]) {
    const mock = closeFixture({ running: [[{ id: app }], []], closeReply });
    mock.context.closeNormal();
    assert.equal(mock.calls.length, 3);
    assert.equal(mock.pauses(), 0);
  }
});

test('close failures remain visible when the normal instance is still running', () => {
  const mock = closeFixture({
    running: [[{ id: app }]],
    closeReply: { returnValue: false, errorText: 'Permission denied' },
  });
  assert.throws(
    () => mock.context.closeNormal(),
    /Close the normal Luma instance.*Permission denied/,
  );
  assert.equal(mock.pauses(), 0);
});

test('takeover stops waiting after five seconds if the normal instance stays running', () => {
  const mock = closeFixture({ running: [[{ id: app }]] });
  assert.throws(() => mock.context.closeNormal(), /Close Luma before enabling Home integration/);
  assert.equal(mock.pauses(), 5);
});
