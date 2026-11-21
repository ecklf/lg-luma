import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import test from 'node:test';

const script = path.resolve(import.meta.dirname, '../tools/ci-install-deps.sh');

async function fixture(t, { mirror = true, failUpdate = false } = {}) {
  const directory = await fs.mkdtemp(path.join(os.tmpdir(), 'luma-ci-deps-'));
  t.after(() => fs.rm(directory, { recursive: true, force: true }));
  const mirrorList = path.join(directory, 'apt-mirrors.txt');
  const log = path.join(directory, 'commands');
  if (mirror) await fs.writeFile(mirrorList, 'http://unreachable.example/ubuntu/\n');
  await fs.writeFile(
    path.join(directory, 'sudo'),
    `#!/usr/bin/env bash
set -eu
printf '%s\\n' "$*" >> "$COMMAND_LOG"
if [ "$1" = tee ]; then
  exec "$@"
fi
if [ "$FAIL_UPDATE" = 1 ] && [[ "$*" == *" update" ]]; then
  exit 100
fi
`,
    { mode: 0o755 },
  );
  const result = spawnSync('bash', [script, 'libsdl2-2.0-0', 'xvfb', 'xauth'], {
    encoding: 'utf8',
    env: {
      ...process.env,
      PATH: `${directory}:${process.env.PATH}`,
      APT_MIRROR_LIST: mirrorList,
      COMMAND_LOG: log,
      FAIL_UPDATE: failUpdate ? '1' : '0',
    },
  });
  return { result, mirrorList, commands: (await fs.readFile(log, 'utf8')).trim().split('\n') };
}

test('CI dependency setup replaces regional mirrors before updating and installing', async (t) => {
  const { result, mirrorList, commands } = await fixture(t);
  assert.equal(result.status, 0, result.stderr);
  assert.equal(await fs.readFile(mirrorList, 'utf8'), 'https://archive.ubuntu.com/ubuntu/\n');
  assert.equal(commands[0], `tee ${mirrorList}`);
  const options =
    '-o Acquire::Retries=3 -o Acquire::http::Timeout=30 -o Acquire::https::Timeout=30 -o APT::Update::Error-Mode=any';
  assert.deepEqual(commands.slice(1), [
    `apt-get ${options} update`,
    `apt-get ${options} install -y --no-install-recommends libsdl2-2.0-0 xvfb xauth`,
  ]);
});

test('CI dependency setup also works without a hosted-runner mirror list', async (t) => {
  const { result, mirrorList, commands } = await fixture(t, { mirror: false });
  assert.equal(result.status, 0, result.stderr);
  await assert.rejects(fs.access(mirrorList), { code: 'ENOENT' });
  assert.equal(commands.length, 2);
  assert.match(commands[0], / update$/);
  assert.match(commands[1], / install /);
});

test('CI dependency setup stops when refreshing package indexes fails', async (t) => {
  const { result, commands } = await fixture(t, { failUpdate: true });
  assert.equal(result.status, 100, result.stderr);
  assert.equal(commands.length, 2);
  assert.match(commands[1], / update$/);
});
