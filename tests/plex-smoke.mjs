import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import path from 'node:path';
import http from 'node:http';
import { spawn } from 'node:child_process';
import sharp from 'sharp';

// PlxNative widgets against a loopback mock of plex.tv and a Plex Media Server. Nothing here
// contacts plex.tv or a real server.
const root = path.resolve(import.meta.dirname, '..');
const out = path.join(root, '.build');
await fs.mkdir(out, { recursive: true });
const temporary = await fs.mkdtemp(path.join(out, 'plex-smoke-'));
const preferences = path.join(temporary, 'preferences.json');
const account = path.join(temporary, 'preferences.plex.json');
const binary = path.resolve(root, process.env.CARGO_TARGET_DIR ?? 'target', 'debug/luma');
const userToken = 'user-token-1234';
const serverToken = 'server-token-5678';
const machine = 'mock-machine-0001';
const requests = [];
let mode = 'ok';
let checks = 0;
const colors = ['#8a3b2e', '#2e5f8a', '#5a8a2e', '#7a2e8a', '#8a7a2e', '#2e8a80'];
const metadata = (count, kind) =>
  Array.from({ length: count }, (_, index) => {
    const key = String(100 + index);
    const thumb = `/library/metadata/${key}/thumb/1`;
    return kind === 'episode'
      ? {
          type: 'episode',
          ratingKey: key,
          grandparentTitle: `Show ${index + 1}`,
          title: 'Episode',
          parentIndex: 1,
          index: index + 1,
          grandparentThumb: thumb,
          duration: 1000,
          viewOffset: 100 * ((index % 9) + 1),
        }
      : { type: 'movie', ratingKey: key, title: `Film ${index + 1}`, year: 2020 + index, thumb };
  });
const server = http.createServer(async (request, response) => {
  const url = new URL(request.url, 'http://127.0.0.1');
  const token = request.headers['x-plex-token'];
  requests.push({ path: url.pathname, query: url.search, token, method: request.method });
  assert.ok(!/token/i.test(url.search), 'The token is only ever a header');
  assert.ok(request.headers['x-plex-client-identifier']?.startsWith('luma-'));
  const json = (status, value) => {
    response.writeHead(status, { 'Content-Type': 'application/json' });
    response.end(JSON.stringify(value));
  };
  if (mode === 'redirect') {
    response.writeHead(302, { Location: '/unexpected' });
    return response.end();
  }
  // plex.tv
  if (url.pathname === '/api/v2/pins' && request.method === 'POST') {
    return json(201, { id: 77, code: 'LUMA' });
  }
  if (url.pathname === '/api/v2/pins/77') {
    checks += 1;
    return json(200, { id: 77, authToken: mode === 'sign-in' ? userToken : null });
  }
  if (url.pathname === '/api/v2/user') {
    assert.equal(token, userToken);
    return json(200, { friendlyName: 'Living room owner' });
  }
  if (url.pathname === '/api/v2/resources') {
    assert.equal(token, userToken);
    const { port } = server.address();
    return json(200, [
      {
        name: 'Mock server',
        provides: 'server',
        clientIdentifier: machine,
        accessToken: serverToken,
        owned: true,
        connections: [
          { uri: 'https://plex.example.com:32400', local: true },
          { uri: `http://127.0.0.1:${port}`, local: true },
        ],
      },
    ]);
  }
  // the Plex server
  if (url.pathname === '/identity') {
    return json(200, { MediaContainer: { machineIdentifier: machine } });
  }
  assert.equal(token, serverToken, `${url.pathname} uses the server's own token`);
  if (mode === 'down') return json(503, {});
  if (url.pathname === '/hubs/continueWatching') {
    return json(200, { MediaContainer: { Metadata: metadata(16, 'episode') } });
  }
  if (url.pathname === '/library/recentlyAdded') {
    return json(200, { MediaContainer: { Metadata: metadata(14, 'movie') } });
  }
  if (url.pathname === '/photo/:/transcode') {
    const thumb = url.searchParams.get('url') ?? '';
    const index = Number(thumb.match(/metadata\/(\d+)/)?.[1] ?? 0) % colors.length;
    const image = await sharp({
      create: { width: 200, height: 300, channels: 3, background: colors[index] },
    })
      .jpeg()
      .toBuffer();
    response.writeHead(200, { 'Content-Type': 'image/jpeg' });
    return response.end(image);
  }
  json(404, {});
});
await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
const origin = `http://127.0.0.1:${server.address().port}`;

async function run(command, args) {
  await new Promise((resolve, reject) => {
    const child = spawn(command, args, { cwd: root, stdio: 'inherit' });
    const timeout = setTimeout(() => {
      child.kill('SIGKILL');
      reject(new Error('PlxNative smoke timed out'));
    }, 90000);
    child.on('error', (error) => {
      clearTimeout(timeout);
      reject(error);
    });
    child.on('exit', (code) => {
      clearTimeout(timeout);
      if (code === 0) resolve();
      else reject(new Error(`${command} exited ${code}`));
    });
  });
}
async function seed(value, signedIn = true) {
  await fs.writeFile(preferences, JSON.stringify({ reduced_motion: true, ...value }));
  await fs.rm(account, { force: true });
  await fs.rm(path.join(temporary, 'preferences.plex-shelves.json'), { force: true });
  if (signedIn)
    await fs.writeFile(
      account,
      JSON.stringify({
        client_id: 'luma-0123456789abcdef',
        token: userToken,
        name: 'Living room owner',
        server: { id: machine, name: 'Mock server', uri: origin, token: serverToken },
      }),
    );
}
async function scenario(name, script, frames = 360, mock = true) {
  const screenshot = path.join(out, `plex-${name}.png`);
  const dump = path.join(temporary, `${name}.json`);
  const diagnostics = path.join(out, `plex-${name}-render.json`);
  await run(binary, [
    '--preferences',
    preferences,
    '--script',
    script,
    '--frames',
    String(frames),
    '--time',
    '2026-11-17T12:00:00Z',
    '--screenshot',
    screenshot,
    '--dump-state',
    dump,
    '--diagnostics',
    diagnostics,
    ...(mock ? ['--plex-test-server', origin] : []),
  ]);
  const state = JSON.parse(await fs.readFile(dump, 'utf8'));
  const render = JSON.parse(await fs.readFile(diagnostics, 'utf8'));
  assert.equal(render.capture.missing_glyphs, 0);
  assert.equal(render.capture.glyph_failures, 0);
  assert.equal(render.capture.missing_images, 0);
  return state;
}
const row = (column, row, wide = false) => ({ enabled: true, position: { column, row }, wide });
try {
  if (process.env.LUMA_SMOKE_SKIP_BUILD !== '1')
    await run('cargo', ['build', '--locked', '--package', 'luma']);
  // Not on Home: nothing is read and nothing is requested, even when signed in.
  await seed({ show_weather: false });
  await scenario('off', 'home,wait,wait');
  assert.equal(requests.length, 0, 'No PlxNative widget, no requests');

  // A wide Continue watching row under Greeting and Weather, focused from favorites.
  await seed({ plex: { continue_watching: row(0, 2, true) } });
  const home = await scenario('continue', 'home,await-plex,up');
  assert.ok(home.ui_text.includes('Continue watching'));
  assert.ok(home.ui_text.includes('Show 1'));
  assert.ok(home.ui_text.includes('S1 · E1'));
  assert.ok(requests.some((r) => r.path === '/hubs/continueWatching'));
  assert.ok(requests.some((r) => r.path === '/photo/:/transcode'));
  assert.ok(!requests.some((r) => r.path === '/library/recentlyAdded'), 'Only lists on Home');

  // Both half-width rows side by side, Greeting and Weather kept. Up from the last dock apps
  // reaches the right-hand row.
  await seed({ plex: { continue_watching: row(0, 2), recently_added: row(3, 2) } });
  const both = await scenario('both', 'home,await-plex,right,right,right,up,right');
  assert.ok(both.ui_text.includes('Recently added'));
  assert.ok(both.ui_text.includes('Film 1'));
  assert.ok(both.ui_text.includes('Show 1'), 'Continue watching shares the grid row');

  // A server that does not answer: no posters, a clear message, Home still works.
  mode = 'down';
  await seed({ plex: { continue_watching: row(0, 2) } });
  const down = await scenario('down', 'home,await-plex', 300);
  assert.ok(down.ui_text.includes('Plex server not reachable'));
  mode = 'ok';

  // Signed out: the row offers sign in, which opens the settings page.
  await seed({ plex: { continue_watching: row(0, 2) } }, false);
  const settings = await scenario('signed-out-settings', 'home,await-plex,up,enter');
  assert.ok(settings.ui_text.includes('Sign in to Plex'));
  const dialog = await scenario('sign-in-dialog', 'home,await-plex,up,enter,enter,await-plex', 400);
  assert.ok(dialog.ui_text.includes('LUMA'));
  assert.ok(checks > 0, 'The code is polled');

  // The whole sign-in: code entered, the only server chosen, rows filled.
  mode = 'sign-in';
  await seed({ plex: { continue_watching: row(0, 2) } }, false);
  const signedIn = await scenario(
    'signed-in',
    'home,await-plex,up,enter,enter,await-plex-sign-in,wait',
    900,
  );
  assert.ok(signedIn.ui_text.includes('Signed in as Living room owner'));
  assert.ok(signedIn.ui_text.includes('Mock server'));
  const saved = JSON.parse(await fs.readFile(account, 'utf8'));
  assert.equal(saved.server.uri, origin, 'The unsafe connection was skipped');
  assert.ok(!(await fs.readFile(preferences, 'utf8')).includes(userToken));

  // Scripted previews without an explicit mock origin stay offline.
  mode = 'ok';
  await seed({ plex: { continue_watching: row(0, 2) } });
  const before = requests.length;
  await scenario('offline', 'home,wait,wait', 120, false);
  assert.equal(requests.length, before);

  // Redirects are never followed.
  mode = 'redirect';
  await seed({ plex: { continue_watching: row(0, 2) } });
  await scenario('redirect', 'home,await-plex', 240);
  assert.ok(!requests.some((r) => r.path === '/unexpected'));
  console.log(
    'PlxNative smoke checks passed: lazy start, both rows, focus, posters, server failure, sign-in, token handling, offline previews and redirect refusal.',
  );
} finally {
  server.closeAllConnections();
  await new Promise((resolve) => server.close(resolve));
  await fs.rm(temporary, { recursive: true, force: true });
}
