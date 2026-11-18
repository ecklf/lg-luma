import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import fs from 'node:fs/promises';
import http from 'node:http';
import path from 'node:path';
import test from 'node:test';

test('SDK setup retries a transient 403 and reuses the installed toolchain', async (t) => {
  const root = path.resolve(import.meta.dirname, '..');
  const scratch = path.join(root, '.build');
  await fs.mkdir(scratch, { recursive: true });
  const directory = await fs.mkdtemp(path.join(scratch, 'sdk-test-'));
  t.after(() => fs.rm(directory, { recursive: true, force: true }));
  const sdkName = 'arm-webos-linux-gnueabi_sdk-buildroot';
  const sdk = path.join(directory, 'fixture', sdkName);
  await fs.mkdir(path.join(sdk, 'bin'), { recursive: true });
  await fs.writeFile(path.join(sdk, 'bin/arm-webos-linux-gnueabi-gcc'), 'fixture');
  await fs.writeFile(path.join(sdk, 'relocate-sdk.sh'), 'echo relocated\n');
  const archive = path.join(directory, 'sdk.tar.gz');
  const packed = spawnSync('tar', ['-czf', archive, '-C', path.dirname(sdk), sdkName]);
  assert.equal(packed.status, 0, packed.stderr?.toString());
  const bytes = await fs.readFile(archive);
  let requests = 0;
  const server = http.createServer((_request, response) => {
    requests += 1;
    response.writeHead(requests === 1 ? 403 : 200);
    response.end(requests === 1 ? 'Temporary release download failure' : bytes);
  });
  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
  t.after(() => new Promise((resolve) => server.close(resolve)));
  const url = `http://127.0.0.1:${server.address().port}/sdk.tar.gz`;
  const source = await fs.readFile(path.join(root, 'tools/setup-webos-sdk.mjs'), 'utf8');
  const tools = path.join(directory, 'tools');
  await fs.mkdir(tools);
  const script = path.join(tools, 'setup-webos-sdk.mjs');
  // Keep the real downloader/extractor, but never contact GitHub or download an SDK in tests.
  await fs.writeFile(script, source.replace(/https:\/\/github\.com\/[^']+/g, url));
  const run = () =>
    new Promise((resolve, reject) => {
      const child = spawn(process.execPath, [script], { timeout: 15000 });
      let output = '';
      child.stdout.on('data', (data) => (output += data));
      child.stderr.on('data', (data) => (output += data));
      child.on('error', reject);
      child.on('close', (status) => resolve({ status, output }));
    });
  const installed = await run();
  assert.equal(installed.status, 0, installed.output);
  assert.equal(requests, 2);
  assert.match(installed.output, /relocated/);
  assert.match(installed.output, /webOS SDK ready/);
  const cached = await run();
  assert.equal(cached.status, 0, cached.output);
  assert.equal(requests, 2, 'installed SDKs must not be downloaded again');
  assert.match(cached.output, /relocated/);
});
