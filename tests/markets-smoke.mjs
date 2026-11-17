import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import path from 'node:path';
import http from 'node:http';
import { spawn } from 'node:child_process';
import sharp from 'sharp';

const root = path.resolve(import.meta.dirname, '..');
const out = path.join(root, '.build');
await fs.mkdir(out, { recursive: true });
const temporary = await fs.mkdtemp(path.join(out, 'markets-smoke-'));
const preferences = path.join(temporary, 'preferences.json');
const binary = path.resolve(root, process.env.CARGO_TARGET_DIR ?? 'target', 'debug/luma');
const requests = [];
let mode = 'ok';
const server = http.createServer((request, response) => {
  const url = new URL(request.url, 'http://127.0.0.1');
  requests.push(url.pathname);
  assert.equal(request.headers.authorization, undefined);
  assert.equal(request.headers.cookie, undefined);
  if (mode === 'redirect') {
    response.writeHead(302, { Location: '/unexpected' });
    response.end();
    return;
  }
  response.writeHead(mode === 'fail' ? 503 : 200, { 'Content-Type': 'application/json' });
  if (mode === 'fail') return response.end('{}');
  if (mode === 'oversized') return response.end(JSON.stringify({ padding: 'x'.repeat(70000) }));
  const crypto = url.pathname.match(/^\/v2\/prices\/([A-Z0-9]+)-([A-Z]+)\/spot$/);
  if (crypto) {
    const [, base, currency] = crypto;
    const current = currency === 'EUR' ? 60000.12 : base === 'ETH' ? 2100.5 : 65000.25;
    const history = url.searchParams.get('date');
    if (history) {
      assert.match(history, /^\d{4}-\d{2}-\d{2}$/);
      if (mode === 'history-fail') return response.end('{}');
    }
    response.end(
      JSON.stringify({
        data: {
          base,
          currency,
          amount: (history ? current / (mode === 'flat' ? 1 : 1.04) : current).toFixed(8),
        },
      }),
    );
    return;
  }
  const stock = url.pathname.match(/^\/v8\/finance\/chart\/([A-Z0-9.-]+)$/);
  assert.ok(stock, `Unexpected market endpoint ${url.pathname}`);
  assert.equal(url.searchParams.get('range'), '1d');
  assert.equal(url.searchParams.get('interval'), '1d');
  response.end(
    JSON.stringify({
      chart: {
        error: null,
        result: [
          {
            meta: {
              symbol: mode === 'wrong-symbol' ? 'WRONG' : stock[1],
              instrumentType: 'EQUITY',
              currency: 'USD',
              regularMarketPrice: stock[1] === 'MSFT' ? 476.55 : 123.45,
              chartPreviousClose: mode === 'flat' ? (stock[1] === 'MSFT' ? 476.55 : 123.45) : 125,
              regularMarketTime: Math.floor(Date.now() / 1000) - 900,
            },
          },
        ],
      },
    }),
  );
});
await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
const origin = `http://127.0.0.1:${server.address().port}`;
async function run(command, args) {
  await new Promise((resolve, reject) => {
    const child = spawn(command, args, { cwd: root, stdio: 'inherit' });
    const timeout = setTimeout(() => {
      child.kill('SIGKILL');
      reject(new Error('Market smoke timed out'));
    }, 60000);
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
const configured = {
  reduced_motion: true,
  show_weather: false,
  crypto: { enabled: true, symbol: 'BTC', currency: 'USD', position: { column: 0, row: 2 } },
  stocks: { enabled: true, symbol: 'AAPL', position: { column: 1, row: 2 } },
};
async function seed(value = configured) {
  await fs.writeFile(preferences, JSON.stringify(value));
}
async function scenario(name, script = 'home', mock = true) {
  const screenshot = path.join(out, `markets-${name}.png`);
  const dump = path.join(temporary, `${name}.json`);
  const diagnostics = path.join(out, `markets-${name}-render.json`);
  await run(binary, [
    '--preferences',
    preferences,
    '--script',
    script,
    '--frames',
    '240',
    '--time',
    '2026-11-17T12:00:00Z',
    '--screenshot',
    screenshot,
    '--dump-state',
    dump,
    '--diagnostics',
    diagnostics,
    ...(mock ? ['--market-test-server', origin] : []),
  ]);
  const state = JSON.parse(await fs.readFile(dump, 'utf8'));
  const render = JSON.parse(await fs.readFile(diagnostics, 'utf8'));
  assert.equal(render.capture.missing_glyphs, 0);
  assert.equal(render.capture.glyph_failures, 0);
  assert.equal(render.capture.missing_images, 0);
  assert.equal((await sharp(screenshot).stats()).channels[3].min, 255);
  return state;
}
const grid = 'settings,down,down,down,down,enter';
try {
  if (process.env.LUMA_SMOKE_SKIP_BUILD !== '1')
    await run('cargo', ['build', '--locked', '--package', 'luma']);
  await seed({});
  await scenario('default-disabled');
  assert.equal(requests.length, 0);
  await seed({ crypto: { enabled: true }, stocks: { enabled: true } });
  const unconfigured = await scenario('unconfigured');
  assert.equal(unconfigured.ui_text.filter((label) => label === 'Not set').length, 2);
  assert.equal(unconfigured.ui_text.filter((label) => label === 'Configure in Settings').length, 2);
  assert.equal(requests.length, 0, 'Unconfigured widgets must never request prices');
  await seed();
  const quotes = await scenario('home');
  for (const label of ['BTC', 'AAPL', '65000.25 USD', '123.45 USD'])
    assert.ok(quotes.ui_text.includes(label), label);
  assert.ok(
    !quotes.ui_text.some((label) => label.includes('Coinbase') || label.includes('Yahoo Finance')),
  );
  assert.ok(!quotes.ui_text.includes('Delayed quote'));
  assert.ok(quotes.ui_text.includes('+4.00%'));
  assert.ok(quotes.ui_text.includes('-1.24%'));
  assert.ok(quotes.ui_text.includes('15m ago'));
  assert.ok(quotes.ui_text.includes('Just now'));
  assert.equal(
    requests.length,
    3,
    'One current and one historic crypto quote, plus one stock quote, not polling every frame',
  );
  await seed({
    ...configured,
    extra_crypto: [
      { enabled: true, symbol: 'ETH', currency: 'USD', position: { column: 2, row: 2 } },
    ],
    extra_stocks: [{ enabled: true, symbol: 'MSFT', position: { column: 3, row: 2 } }],
  });
  const multiple = await scenario('multiple');
  for (const label of [
    'BTC',
    'ETH',
    'AAPL',
    'MSFT',
    '65000.25 USD',
    '2100.50 USD',
    '123.45 USD',
    '476.55 USD',
  ])
    assert.ok(multiple.ui_text.includes(label), label);
  const info = await scenario('info-card', `${grid},click:720:560,enter`);
  assert.ok(info.ui_text.includes('Coinbase · BTC, ETH'));
  assert.ok(info.ui_text.includes('Updates every 5 minutes on Home. Quotes may be delayed.'));
  await scenario('move-toolbar', `${grid},click:720:560,down,enter`);
  await seed();
  const changed = await scenario(
    'edit-coin',
    `${grid},click:720:560,enter,enter,type:eth,back,home`,
  );
  assert.equal(changed.preferences.crypto.symbol, 'ETH');
  assert.ok(changed.ui_text.includes('2100.50 USD'));
  const currency = await scenario('currency', `${grid},click:720:560,enter,down,enter,home`);
  assert.equal(currency.preferences.crypto.currency, 'EUR');
  assert.ok(currency.ui_text.includes('60000.12 EUR'));
  const stock = await scenario(
    'edit-stock',
    `${grid},click:900:560,enter,enter,type:msft,back,home`,
  );
  assert.equal(stock.preferences.stocks.symbol, 'MSFT');
  assert.ok(stock.ui_text.includes('476.55 USD'));
  const restarted = await scenario('restart');
  assert.equal(restarted.preferences.crypto.symbol, 'ETH');
  assert.equal(restarted.preferences.stocks.symbol, 'MSFT');
  mode = 'history-fail';
  const noHistory = await scenario('history-unavailable');
  assert.ok(noHistory.ui_text.includes('60000.12 EUR'));
  assert.ok(noHistory.ui_text.includes('Change unavailable'));
  mode = 'flat';
  const flat = await scenario('flat');
  assert.ok(flat.ui_text.includes('0.00%'));
  for (const failure of ['fail', 'oversized', 'redirect', 'wrong-symbol']) {
    mode = failure;
    await seed();
    const failed = await scenario(failure);
    assert.ok(failed.ui_text.includes('Quote unavailable'));
    assert.ok(!failed.ui_text.includes('123.45 USD'));
  }
  assert.ok(!requests.includes('/unexpected'), 'Never follow redirects');
  const before = requests.length;
  await scenario('script-offline', 'home', false);
  assert.equal(
    requests.length,
    before,
    'Scripted previews without an explicit mock origin stay offline',
  );
  console.log(
    'Market smoke checks passed: both widgets, symbol editing, currency, restart, relative ages, clean labels, bounded replies, HTTP failures, redirect refusal and offline previews.',
  );
} finally {
  server.closeAllConnections();
  await new Promise((resolve) => server.close(resolve));
  await fs.rm(temporary, { recursive: true, force: true });
}
