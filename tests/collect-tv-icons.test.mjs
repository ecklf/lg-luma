import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import path from 'node:path';
import test from 'node:test';
import vm from 'node:vm';
import visibilityCases from './fixtures/app-visibility.json' with { type: 'json' };

const source = await fs.readFile(new URL('../tools/collect-tv-icons.js', import.meta.url), 'utf8');
const roots = [
  '/media/developer/apps/usr/palm/applications',
  '/media/cryptofs/apps/usr/palm/applications',
  '/usr/palm/applications',
];
const out = '/tmp/luma-artwork';

function collect({
  apps = [],
  installations = apps.map((app, index) => ({
    directory: `${roots[0]}/app-${index}`,
    manifest: { ...app, icon: 'icon.png' },
    icons: { 'icon.png': `icon-${index}` },
  })),
  existingOutput = false,
  inventoryError = false,
  reply = { returnValue: true, apps },
} = {}) {
  const files = new Map();
  const directories = new Map();
  for (const { directory, manifest, icons = {} } of installations) {
    const root = path.posix.dirname(directory);
    if (!directories.has(root)) directories.set(root, []);
    directories.get(root).push(path.posix.basename(directory));
    files.set(`${directory}/appinfo.json`, JSON.stringify(manifest));
    for (const [name, data] of Object.entries(icons)) {
      files.set(`${directory}/${name}`, Buffer.from(data));
    }
  }
  const logs = [];
  const errors = [];
  const mockFs = {
    mkdirSync(directory) {
      assert.equal(directory, out);
      if (existingOutput) throw Object.assign(new Error('Already exists'), { code: 'EEXIST' });
    },
    existsSync: (file) => files.has(file),
    statSync: () => ({ isFile: () => true }),
    readdirSync(directory) {
      if (directories.has(directory)) return directories.get(directory);
      throw Object.assign(new Error('Missing install root'), { code: 'ENOENT' });
    },
    readFileSync(file) {
      assert.ok(files.has(file), `Unexpected file read: ${file}`);
      return files.get(file);
    },
    writeFileSync(file, content) {
      assert.ok(file.startsWith(`${out}/`));
      files.set(file, content);
    },
  };
  vm.runInNewContext(
    source,
    {
      require(name) {
        if (name === 'fs') return mockFs;
        if (name === 'path') return path.posix;
        assert.equal(name, 'child_process');
        return {
          execSync(command) {
            assert.equal(
              command,
              "/usr/bin/luna-send -n 1 -w 4000 luna://com.webos.applicationManager/listApps '{}'",
            );
            if (inventoryError) throw new Error('Mock inventory failure');
            return Buffer.from(typeof reply === 'string' ? reply : JSON.stringify(reply));
          },
        };
      },
      console: { log: (message) => logs.push(message), error: (message) => errors.push(message) },
    },
    { timeout: 1000 },
  );
  return { map: JSON.parse(files.get(`${out}/app-icons.json`)), files, logs, errors };
}

test('the collector and launcher share app visibility and exclusion cases', () => {
  const result = collect({ apps: visibilityCases.map(({ app }) => app) });
  const expected = [
    ...new Set(visibilityCases.filter((entry) => entry.include).map(({ app }) => app.id)),
  ];
  assert.deepEqual(Object.keys(result.map).sort(), expected.sort());
  assert.equal(result.errors.length, 0);
  for (const relative of Object.values(result.map)) {
    assert.match(relative, /^\.\/app-icons\/\d+\.png$/);
    assert.ok(result.files.has(`${out}/${path.posix.basename(relative)}`));
  }
});

test('Live TV is collected for every visibility flag', () => {
  for (const visible of [true, false, 'false', 'true', null, undefined]) {
    const { map } = collect({ apps: [{ id: 'com.webos.app.livetv', visible }] });
    assert.ok(map['com.webos.app.livetv']);
  }
});

test('live inventory visibility takes precedence over the installed manifest', () => {
  const result = collect({
    apps: [
      { id: 'coupangplay', visible: false },
      { id: 'com.webos.app.igallery', visible: true },
    ],
    installations: [
      {
        directory: `${roots[0]}/coupang`,
        manifest: { id: 'coupangplay', visible: true, icon: 'icon.png' },
        icons: { 'icon.png': 'hidden' },
      },
      {
        directory: `${roots[2]}/gallery`,
        manifest: { id: 'com.webos.app.igallery', visible: false, icon: 'icon.png' },
        icons: { 'icon.png': 'gallery' },
      },
    ],
  });
  assert.deepEqual(Object.keys(result.map), ['com.webos.app.igallery']);
});

test('existing output, missing install roots and duplicate installations are supported', () => {
  const options = {
    apps: [{ id: 'coupangplay', visible: true }],
    installations: [
      {
        directory: `${roots[0]}/coupang`,
        manifest: { id: 'coupangplay', icon: 'icon.png' },
        icons: { 'icon.png': 'developer version' },
      },
      {
        directory: `${roots[1]}/coupang`,
        manifest: { id: 'coupangplay', icon: 'icon.png' },
        icons: { 'icon.png': 'store version' },
      },
    ],
  };
  const fresh = collect(options);
  const repeated = collect({ ...options, existingOutput: true });
  assert.deepEqual(fresh.map, { coupangplay: './app-icons/0.png' });
  assert.deepEqual(repeated.map, fresh.map);
  assert.equal(fresh.files.get(`${out}/0.png`).toString(), 'developer version');
  assert.deepEqual(fresh.errors, []);
});

test('large icons are preferred and missing ones fall back to regular icons', () => {
  const result = collect({
    apps: [{ id: 'large' }, { id: 'fallback' }, { id: 'default' }],
    installations: [
      {
        directory: `${roots[0]}/large`,
        manifest: { id: 'large', largeIcon: 'large.png', icon: 'icon.png' },
        icons: { 'large.png': 'large icon', 'icon.png': 'small icon' },
      },
      {
        directory: `${roots[0]}/fallback`,
        manifest: { id: 'fallback', largeIcon: 'missing.png', icon: 'icon.png' },
        icons: { 'icon.png': 'fallback icon' },
      },
      {
        directory: `${roots[0]}/default`,
        manifest: { id: 'default' },
        icons: { 'icon.png': 'default icon' },
      },
    ],
  });
  for (const [id, expected] of [
    ['large', 'large icon'],
    ['fallback', 'fallback icon'],
    ['default', 'default icon'],
  ]) {
    const file = path.posix.basename(result.map[id]);
    assert.equal(result.files.get(`${out}/${file}`).toString(), expected);
  }
});

test('failed or invalid inventory replies report the problem and exclude unconfirmed system apps', () => {
  const apps = [
    { id: 'coupangplay' },
    { id: 'com.webos.app.igallery', visible: true },
    { id: 'com.webos.app.livetv' },
  ];
  for (const failure of [
    { inventoryError: true },
    { reply: 'invalid JSON' },
    { reply: { returnValue: false, apps } },
    { reply: { returnValue: true, apps: 'invalid' } },
    { reply: { returnValue: true, apps: Array.from({ length: 1001 }, () => apps[1]) } },
  ]) {
    const result = collect({ apps, ...failure });
    assert.deepEqual(Object.keys(result.map).sort(), ['com.webos.app.livetv', 'coupangplay']);
    assert.equal(result.errors.length, 1);
    assert.match(result.errors[0], /listApps failed/);
  }
});
