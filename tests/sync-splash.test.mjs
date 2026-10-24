import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import path from 'node:path';
import test from 'node:test';
import vm from 'node:vm';

const script = await fs.readFile(new URL('../tv/sync-splash.cjs', import.meta.url), 'utf8');
const app = 'com.ecklf.luma';
const state = '/var/lib/luma-home';

function fixture({ production = false, config, failWrite = false, stage = true } = {}) {
  const directory = `/media/${production ? 'cryptofs' : 'developer'}/apps/usr/palm/applications/${app}`;
  const files = new Map([
    [
      `${directory}/appinfo.json`,
      Buffer.from(JSON.stringify({ id: app, type: 'native', main: 'luma' })),
    ],
    [`${directory}/splash.png`, Buffer.from('dunes')],
    [`${directory}/splash/forest.png`, Buffer.from('forest')],
    [`${directory}/splash/prism.png`, Buffer.from('prism')],
  ]);
  // The executable Home method creates no staged app directory.
  const directories = new Set([directory, `${directory}/splash`, state]);
  if (stage) {
    directories.add(`${state}/app`);
    files.set(`${state}/app/splash.png`, Buffer.from('old Home splash'));
  }
  if (config) files.set(`${state}/state.json`, Buffer.from(JSON.stringify(config(directory))));
  const writes = [];
  const renames = [];
  const missing = () => Object.assign(new Error('Missing'), { code: 'ENOENT' });
  const mockFs = {
    readFileSync(file, encoding) {
      if (!files.has(file)) throw missing();
      const data = files.get(file);
      return encoding ? data.toString(encoding) : data;
    },
    writeFileSync(file, data, options) {
      assert.ok(file.endsWith('/splash.png.new'));
      assert.equal(options.mode, 0o644);
      if (!directories.has(path.posix.dirname(file))) throw missing();
      if (failWrite) throw Object.assign(new Error('Read-only filesystem'), { code: 'EROFS' });
      writes.push(file);
      files.set(file, data);
    },
    renameSync(from, to) {
      assert.ok(files.has(from));
      renames.push(to);
      files.set(to, files.get(from));
      files.delete(from);
    },
  };
  return {
    directory,
    files,
    writes,
    renames,
    sync(background, overrides = {}) {
      const logs = [];
      vm.runInNewContext(
        script,
        {
          require(name) {
            if (name === 'fs') return mockFs;
            assert.equal(name, 'path', 'the helper must not invoke system or TV commands');
            return path.posix;
          },
          process: { argv: ['node', JSON.stringify({ directory, background, ...overrides })] },
          console: { log: (line) => logs.push(line) },
        },
        { timeout: 2000 },
      );
      assert.deepEqual(logs.map(JSON.parse), [{ returnValue: true, background }]);
    },
  };
}

test('normal developer and production tile launches use the saved selection', () => {
  for (const production of [false, true]) {
    const mock = fixture({ production });
    mock.sync('forest');
    assert.equal(mock.files.get(`${mock.directory}/splash.png`).toString(), 'forest');
    mock.sync('prism');
    assert.equal(mock.files.get(`${mock.directory}/splash.png`).toString(), 'prism');
    assert.deepEqual(mock.renames, [
      `${mock.directory}/splash.png`,
      `${mock.directory}/splash.png`,
    ]);
    assert.ok(!mock.files.has(`${mock.directory}/splash.png.new`));
    assert.equal(mock.files.get(`${state}/app/splash.png`).toString(), 'old Home splash');
  }
});

test('only an enabled Home stage belonging to this installation is refreshed', () => {
  for (const [enabled, matching] of [
    [true, true],
    [false, true],
    [true, false],
  ]) {
    const mock = fixture({
      config: (directory) => ({ enabled, appDir: matching ? directory : '/another-installation' }),
    });
    mock.sync('prism');
    assert.equal(mock.files.get(`${mock.directory}/splash.png`).toString(), 'prism');
    assert.equal(
      mock.files.get(`${state}/app/splash.png`).toString(),
      enabled && matching ? 'prism' : 'old Home splash',
    );
  }
});

test('the executable Home method has no staged copy, so only the tile splash is updated', () => {
  for (const method of ['executable', 'manifest', undefined]) {
    const executable = method === 'executable';
    const mock = fixture({
      stage: !executable,
      config: (directory) => ({ enabled: true, method, appDir: directory }),
    });
    mock.sync('prism');
    assert.equal(mock.files.get(`${mock.directory}/splash.png`).toString(), 'prism');
    assert.equal(
      mock.files.get(`${state}/app/splash.png`)?.toString(),
      executable ? undefined : 'prism',
    );
  }
});

test('startup repairs a replaced package splash without rewriting an unchanged Home image', () => {
  const mock = fixture({ config: (directory) => ({ enabled: true, appDir: directory }) });
  mock.sync('forest');
  mock.writes.length = 0;
  mock.sync('forest');
  assert.deepEqual(mock.writes, []);
  mock.files.set(`${mock.directory}/splash.png`, Buffer.from('package update default'));
  mock.sync('forest');
  assert.deepEqual(mock.writes, [`${mock.directory}/splash.png.new`]);
  assert.equal(mock.files.get(`${mock.directory}/splash.png`).toString(), 'forest');
});

test('missing sources and failed writes preserve the previous launch splash', () => {
  for (const failWrite of [false, true]) {
    const mock = fixture({ failWrite });
    assert.throws(() => mock.sync(failWrite ? 'forest' : 'missing'), /Missing|Read-only/);
    assert.equal(mock.files.get(`${mock.directory}/splash.png`).toString(), 'dunes');
    assert.equal(mock.files.get(`${state}/app/splash.png`).toString(), 'old Home splash');
    assert.deepEqual(mock.renames, []);
  }
});

test('unrelated installations and invalid selections cannot update a launch splash', () => {
  const mock = fixture();
  for (const background of ['../forest', '', 'forest.png', null]) {
    assert.throws(() => mock.sync(background), /Invalid splash background/);
  }
  for (const directory of ['/etc', `${mock.directory}/..`, `${mock.directory}-other`]) {
    assert.throws(() => mock.sync('forest', { directory }), /Invalid Luma installation/);
  }
  mock.files.set(
    `${mock.directory}/appinfo.json`,
    Buffer.from(JSON.stringify({ id: 'another.app', type: 'native', main: 'luma' })),
  );
  assert.throws(() => mock.sync('forest'), /installed native Luma package/);
  assert.deepEqual(mock.writes, []);
});
