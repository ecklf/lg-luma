import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import test from 'node:test';

const source = fs.readFileSync(new URL('../tv/self-update.sh', import.meta.url), 'utf8');
function run({
  install = true,
  home = false,
  reapply = true,
  recover = true,
  launch = true,
  checksum = true,
  method = 'executable',
  homeId = 'com.webos.app.home',
} = {}) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'luma-installer-'));
  const app = `${dir}/app`;
  const state = `${dir}/update`;
  const homeState = `${dir}/home`;
  const bin = `${dir}/bin`;
  for (const d of [app, homeState, bin]) fs.mkdirSync(d);
  const write = (file, contents) => fs.writeFileSync(file, contents, { mode: 0o755 });
  write(`${dir}/package.ipk`, 'test package');
  write(`${app}/appinfo.json`, JSON.stringify({ version: '9.0.0' }));
  write(`${homeState}/state.json`, JSON.stringify({ enabled: home, method, homeId }));
  write(`${dir}/mounts`, '');
  write(`${bin}/id`, '#!/bin/sh\necho 0\n');
  write(`${bin}/stat`, '#!/bin/sh\necho "0 700"\n');
  write(`${bin}/sleep`, '#!/bin/sh\n/bin/sleep 0.01\n');
  write(
    `${bin}/luna-send`,
    `#!/bin/sh
printf '%s\\n' "$*" >> '${dir}/calls'
case "$*" in
  *dev/install*) echo '${install ? '{"returnValue":true,"state":"installed"}' : '{"returnValue":false,"errorText":"disk full"}'}' ;;
  *) echo '{"returnValue":${launch}}' ;;
esac
`,
  );
  write(`${app}/home.sh`, `#!/bin/sh\necho refresh >> '${dir}/calls'\nexit ${reapply ? 0 : 1}\n`);
  write(
    `${homeState}/home.sh`,
    `#!/bin/sh\necho restore >> '${dir}/calls'\nexit ${recover ? 0 : 1}\n`,
  );
  const script = source
    .replaceAll('/var/lib/luma-update', state)
    .replaceAll('/var/lib/luma-home', homeState)
    .replaceAll('/media/developer/apps/usr/palm/applications/com.ecklf.luma', app)
    .replaceAll('/proc/mounts', `${dir}/mounts`)
    .replaceAll('/var/run/luma-home', `${dir}/marker`);
  write(`${dir}/installer.sh`, script);
  const attempt = 'a'.repeat(32);
  const sha = checksum ? createHash('sha256').update('test package').digest('hex') : '0'.repeat(64);
  try {
    const result = spawnSync(
      '/bin/sh',
      [`${dir}/installer.sh`, `${dir}/package.ipk`, sha, '9.0.0', app, attempt],
      {
        env: { ...process.env, PATH: `${bin}:${process.env.PATH}` },
        encoding: 'utf8',
        timeout: 10000,
      },
    );
    assert.equal(result.signal, null, result.stderr);
    const status = JSON.parse(fs.readFileSync(`${state}/result.${attempt}.json`, 'utf8'));
    assert.deepEqual(status, JSON.parse(fs.readFileSync(`${state}/result.json`, 'utf8')));
    const calls = fs.existsSync(`${dir}/calls`) ? fs.readFileSync(`${dir}/calls`, 'utf8') : '';
    return { status, calls, exit: result.status };
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
}

test('normal app update verifies the package and reports completion after launch', () => {
  const result = run();
  assert.equal(result.exit, 0);
  assert.equal(result.status.state, 'success');
  assert.match(result.calls, /launch.*com.ecklf.luma/);
});
test('checksum and install failures publish retryable failure, never launch', () => {
  for (const options of [{ checksum: false }, { install: false }]) {
    const result = run(options);
    assert.equal(result.exit, 1);
    assert.equal(result.status.state, 'failed');
    assert.doesNotMatch(result.calls, /launch/);
  }
});
test('both Home methods refresh from the new package and fail visibly when reapply fails', () => {
  for (const method of ['executable', 'manifest']) {
    const success = run({ home: true, method });
    assert.equal(success.status.state, 'success');
    assert.match(success.calls, /refresh/);
    const failed = run({ home: true, method, reapply: false });
    assert.equal(failed.exit, 1);
    assert.equal(failed.status.state, 'failed');
    assert.match(failed.status.message, /LG Home restored/);
    assert.match(failed.calls, /restore[\s\S]*launch.*com.webos.app.home/);
    const unrecovered = run({ home: true, method, reapply: false, recover: false });
    assert.equal(unrecovered.status.state, 'failed');
    assert.doesNotMatch(unrecovered.calls, /launch/);
  }
});
test('a rejected launch is not reported as a successful update', () => {
  assert.equal(run({ launch: false }).status.state, 'failed');
});

test('fullhome remains the launch and recovery target after an update', () => {
  const homeId = 'com.webos.app.fullhome';
  const success = run({ home: true, homeId });
  assert.equal(success.status.state, 'success');
  assert.match(success.calls, /launch.*com.webos.app.fullhome/);
  assert.doesNotMatch(success.calls, /launch.*com.webos.app.home/);
  const failed = run({ home: true, homeId, reapply: false });
  assert.match(failed.status.message, /LG Home restored/);
  assert.match(failed.calls, /restore[\s\S]*launch.*com.webos.app.fullhome/);
});
