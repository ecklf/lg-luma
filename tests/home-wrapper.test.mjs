import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';

const source = await fs.readFile(new URL('../tv/home.sh', import.meta.url), 'utf8');

// tv/home.sh in a temporary root: the copied state directory with a recording luma-helper,
// the cold-boot hook as a real symlink, and stubs for the root-only id/stat checks.
async function fixture({ helper = true, relative = false } = {}) {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), 'luma-home-wrapper-'));
  const state = path.join(root, 'state');
  const bin = path.join(root, 'bin');
  const hooks = path.join(root, 'init.d');
  const calls = path.join(root, 'calls');
  await Promise.all([state, bin, hooks].map((dir) => fs.mkdir(dir)));
  const line = 'STATE=/var/lib/luma-home\n';
  assert.ok(source.includes(line));
  await fs.writeFile(path.join(state, 'home.sh'), source.replace(line, `STATE='${state}'\n`), {
    mode: 0o755,
  });
  if (helper) {
    await fs.writeFile(
      path.join(state, 'luma-helper'),
      `#!/bin/sh\nfor a; do printf '[%s]' "$a"; done >>'${calls}'\necho >>'${calls}'\n`,
      { mode: 0o755 },
    );
  }
  for (const [name, body] of [
    ['id', 'echo 0'],
    ['stat', 'echo 0'],
    ['sleep', 'exit 0'],
  ]) {
    await fs.writeFile(path.join(bin, name), `#!/bin/sh\n${body}\n`, { mode: 0o755 });
  }
  const hook = path.join(hooks, '64-luma-home');
  await fs.symlink(relative ? '../state/home.sh' : path.join(state, 'home.sh'), hook);
  const run = (...args) =>
    spawnSync('/bin/sh', [hook, ...args], {
      encoding: 'utf8',
      env: { ...process.env, PATH: `${bin}:${process.env.PATH}` },
      timeout: 10000,
    });
  const recorded = async () => {
    // Long enough for a real 5 s sleep where the shell runs sleep as a builtin (busybox ash).
    for (let i = 0; i < 200; i++) {
      const text = await fs.readFile(calls, 'utf8').catch(() => '');
      if (text) return text.trim().split('\n');
      await new Promise((resolve) => setTimeout(resolve, 50));
    }
    return [];
  };
  return { root, state, run, recorded };
}

for (const relative of [false, true]) {
  test(`the ${relative ? 'relative' : 'absolute'} boot-hook symlink finds the copied helper`, async () => {
    const tv = await fixture({ relative });
    try {
      const status = tv.run('status');
      assert.equal(status.status, 0, status.stderr);
      assert.deepEqual(await tv.recorded(), [`[home][status][${tv.state}][]`]);
    } finally {
      await fs.rm(tv.root, { recursive: true, force: true });
    }
  });
}

test('a cold boot through the hook symlink schedules the copied apply', async () => {
  const tv = await fixture();
  try {
    const boot = tv.run();
    assert.equal(boot.status, 0, boot.stderr);
    assert.deepEqual(await tv.recorded(), [`[home][apply][${tv.state}][]`]);
  } finally {
    await fs.rm(tv.root, { recursive: true, force: true });
  }
});

test('a missing helper is reported before any change', async () => {
  const tv = await fixture({ helper: false });
  try {
    const status = tv.run('status');
    assert.equal(status.status, 1);
    assert.match(status.stderr, /native Luma TV helper is missing/);
  } finally {
    await fs.rm(tv.root, { recursive: true, force: true });
  }
});
