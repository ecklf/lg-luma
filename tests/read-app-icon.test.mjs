import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import path from 'node:path';
import test from 'node:test';
import vm from 'node:vm';

const script = await fs.readFile(new URL('../tv/read-app-icon.cjs', import.meta.url), 'utf8');
const roots = [
  '/media/developer/apps/usr/palm/applications',
  '/media/cryptofs/apps/usr/palm/applications',
  '/usr/palm/applications',
];
const directory = `${roots[0]}/sample`;

function readIcon({ metadata = {}, installations = [], chunk = 16384 } = {}) {
  const files = new Map();
  for (const install of installations) {
    const dir = install.directory ?? directory;
    if (install.manifest !== undefined) {
      files.set(`${dir}/appinfo.json`, Buffer.from(JSON.stringify(install.manifest)));
    }
    for (const [name, bytes] of Object.entries(install.icons ?? {})) {
      files.set(path.posix.resolve(dir, name), Buffer.from(bytes));
    }
  }
  const opened = [];
  const descriptors = new Set();
  const logs = [];
  const errors = [];
  const mockFs = {
    openSync(file) {
      opened.push(file);
      if (!files.has(file)) throw Object.assign(new Error('Missing'), { code: 'ENOENT' });
      descriptors.add(file);
      return file;
    },
    fstatSync(file) {
      return { size: files.get(file).length, isFile: () => true };
    },
    readSync(file, buffer, offset, length, position) {
      return files.get(file).copy(buffer, offset, position, position + Math.min(length, chunk));
    },
    closeSync(file) {
      assert.ok(descriptors.delete(file));
    },
  };
  vm.runInNewContext(
    script,
    {
      require(name) {
        if (name === 'fs') return mockFs;
        assert.equal(name, 'path');
        return path.posix;
      },
      process: { argv: ['node', JSON.stringify({ id: 'sample', ...metadata })] },
      Buffer,
      console: { log: (line) => logs.push(line), error: (line) => errors.push(line) },
    },
    { timeout: 2000 },
  );
  assert.equal(descriptors.size, 0, 'all opened files must be closed');
  assert.equal(logs.length, 1);
  const reply = JSON.parse(logs[0]);
  assert.equal(reply.returnValue, true);
  return { icons: reply.icons.map((hex) => Buffer.from(hex, 'hex')), opened, errors };
}

test('runtime helper discovers manifest artwork without any bundled icon map', () => {
  const { icons } = readIcon({
    installations: [
      {
        manifest: { id: 'sample', largeIcon: 'resources/hu/large.png', icon: 'regular.png' },
        icons: { 'resources/hu/large.png': 'large', 'regular.png': 'regular' },
      },
    ],
    chunk: 3,
  });
  assert.deepEqual(
    icons.map((bytes) => bytes.toString()),
    ['large', 'regular'],
  );
});

test('live inventory installation and localized icon take precedence over other copies', () => {
  const installed = `${roots[1]}/actual-directory`;
  const { icons } = readIcon({
    metadata: { directory: installed, large_icon: `file://${installed}/localized.webp` },
    installations: [
      { icons: { 'icon.png': 'old developer install' } },
      { directory: installed, icons: { 'localized.webp': 'current installed artwork' } },
    ],
  });
  assert.equal(icons[0].toString(), 'current installed artwork');
});

test('missing, unsupported and oversized large icons fall back to regular or default artwork', () => {
  for (const [name, bytes] of [
    ['missing.png', null],
    ['large.svg', '<svg/>'],
    ['large.png', Buffer.alloc(512 * 1024 + 1)],
  ]) {
    const { icons } = readIcon({
      installations: [
        {
          manifest: { id: 'sample', largeIcon: name, icon: 'regular.jpeg' },
          icons: { ...(bytes ? { [name]: bytes } : {}), 'regular.jpeg': 'regular' },
        },
      ],
    });
    assert.deepEqual(
      icons.map((data) => data.toString()),
      ['regular'],
    );
  }
  const { icons } = readIcon({ installations: [{ icons: { 'icon.png': 'default' } }] });
  assert.equal(icons[0].toString(), 'default');
});

test('artwork responses have an aggregate byte limit and do not duplicate the same path', () => {
  const { icons } = readIcon({
    metadata: { large_icon: 'large.png', icon: 'regular.png' },
    installations: [
      {
        manifest: { id: 'sample', largeIcon: 'large.png', icon: 'regular.png' },
        icons: {
          'large.png': Buffer.alloc(400 * 1024),
          'regular.png': Buffer.alloc(200 * 1024),
          'icon.png': 'default',
        },
      },
    ],
  });
  assert.deepEqual(
    icons.map((data) => data.length),
    [400 * 1024, 7],
  );
});

test('invalid paths and unrelated manifests cannot select artwork outside app directories', () => {
  for (const icon of [
    '/etc/passwd.png',
    '../../secret.png',
    'https://example.com/icon.png',
    'bad\0.png',
  ]) {
    const { icons, opened } = readIcon({
      metadata: { directory: '/etc', large_icon: icon },
      installations: [
        {
          manifest: { id: 'another-app', largeIcon: 'wrong.png' },
          icons: { 'wrong.png': 'wrong' },
        },
      ],
    });
    assert.deepEqual(icons, []);
    assert.ok(opened.every((file) => roots.some((root) => file.startsWith(`${root}/`))));
    assert.ok(!opened.some((file) => file.endsWith('/wrong.png')));
  }
  assert.throws(() => readIcon({ metadata: { id: 'sample; command' } }), /Invalid application ID/);
  const { icons } = readIcon({
    installations: [{ manifest: null, icons: { 'icon.png': 'default' } }],
  });
  assert.equal(icons[0].toString(), 'default');
});

test('file names containing shell punctuation are treated as literal local paths', () => {
  const name = "it's; $(not-a-command).png";
  const { icons } = readIcon({
    metadata: { icon: name },
    installations: [{ icons: { [name]: 'literal file' } }],
  });
  assert.equal(icons[0].toString(), 'literal file');
});
