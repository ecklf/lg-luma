import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import test from 'node:test';

const source = fs.readFileSync(new URL('../tv/home-toggle.sh', import.meta.url), 'utf8');

function run({
  enable = true,
  operation = true,
  status = true,
  mounted = enable,
  bootHook = enable,
  reportedEnabled = enable,
  busy = false,
} = {}) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'luma-home-toggle-'));
  const app = `${dir}/app`;
  const state = `${dir}/state`;
  const home = `${dir}/home`;
  const bin = `${dir}/bin`;
  const attempt = 'a'.repeat(32);
  for (const folder of [app, home, bin]) fs.mkdirSync(folder);
  const write = (file, text) => fs.writeFileSync(file, text, { mode: 0o755 });
  write(`${bin}/id`, '#!/bin/sh\necho 0\n');
  write(`${bin}/stat`, '#!/bin/sh\necho "0 700"\n');
  if (busy) write(`${bin}/flock`, '#!/bin/sh\nexit 1\n');
  const operationScript = `#!/bin/sh
echo operation >> '${dir}/calls'
exit ${operation ? 0 : 1}
`;
  write(`${app}/home.sh`, operationScript);
  write(`${home}/home.sh`, operationScript);
  write(`${app}/autostart.sh`, `#!/bin/sh\necho autostart-disabled >> '${dir}/calls'\n`);
  write(
    `${app}/luma-helper`,
    `#!/bin/sh
echo status >> '${dir}/calls'
echo '${JSON.stringify({ enabled: reportedEnabled, mounted, bootHook })}'
exit ${status ? 0 : 1}
`,
  );
  write(
    `${dir}/toggle.sh`,
    source
      .replaceAll('/media/developer/apps/usr/palm/applications/com.ecklf.luma', app)
      .replaceAll('/var/lib/luma-home-toggle', state)
      .replaceAll('/var/lib/luma-home/home.sh', `${home}/home.sh`),
  );
  try {
    const result = spawnSync('/bin/sh', [`${dir}/toggle.sh`, app, String(enable), attempt], {
      env: { ...process.env, PATH: `${bin}:${process.env.PATH}` },
      encoding: 'utf8',
      timeout: 10000,
    });
    assert.equal(result.signal, null, result.stderr);
    const completion = JSON.parse(fs.readFileSync(`${state}/result.${attempt}.json`, 'utf8'));
    const calls = fs.existsSync(`${dir}/calls`) ? fs.readFileSync(`${dir}/calls`, 'utf8') : '';
    return { completion, calls, exit: result.status };
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
}

test('enable retires autostart only after successful takeover and status verification', () => {
  const result = run();
  assert.equal(result.exit, 0);
  assert.equal(result.completion.returnValue, true);
  assert.equal(result.calls, 'operation\nstatus\nautostart-disabled\n');
});

test('failed or unverified enable preserves existing autostart and reports failure', () => {
  for (const options of [
    { operation: false },
    { status: false },
    { mounted: false },
    { bootHook: false },
    { reportedEnabled: false },
    { busy: true },
  ]) {
    const result = run(options);
    assert.equal(result.exit, 1);
    assert.equal(result.completion.returnValue, false);
    assert.doesNotMatch(result.calls, /autostart-disabled/);
  }
});

test('disable requires both saved state and integration to be removed', () => {
  const result = run({ enable: false });
  assert.equal(result.completion.returnValue, true);
  assert.doesNotMatch(result.calls, /autostart-disabled/);
  for (const options of [
    { operation: false },
    { mounted: true },
    { bootHook: true },
    { reportedEnabled: true },
  ]) {
    assert.equal(run({ ...options, enable: false }).completion.returnValue, false);
  }
});
