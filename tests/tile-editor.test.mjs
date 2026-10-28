import assert from 'node:assert/strict';
import test from 'node:test';
import { readFile } from 'node:fs/promises';
import {
  checkListing,
  describe,
  loadListing,
  normalizeUrl,
  post,
} from '../assets/tile-editor/tiles.js';

const listing = {
  tiles: [{ id: 'custom.lights', title: 'Lights', url: 'http://ha.local/hook', method: 'POST' }],
  icons: [{ id: 'weather-moon', name: 'Moon', svg: '<svg></svg>' }],
  limit: 64,
};

test('addresses typed without a scheme use http, like the TV keyboard', () => {
  assert.equal(
    normalizeUrl('  ha.local:8123/api/webhook/x '),
    'http://ha.local:8123/api/webhook/x',
  );
  assert.equal(normalizeUrl('https://ha.example/hook'), 'https://ha.example/hook');
  assert.equal(normalizeUrl('ftp://ha.example'), 'ftp://ha.example');
  assert.equal(normalizeUrl('   '), '');
  assert.equal(describe(listing.tiles[0]), 'POST · http://ha.local/hook');
});

test('the tile list is read without caching and rejected when malformed', async () => {
  assert.deepEqual(
    await loadListing(async (url, options) => {
      assert.equal(url, 'tiles.json');
      assert.deepEqual(options, { cache: 'no-store' });
      return { ok: true, json: async () => listing };
    }),
    listing,
  );
  await assert.rejects(
    loadListing(async () => ({ ok: false })),
    /unavailable/,
  );
  for (const invalid of [null, {}, { ...listing, icons: [] }, { ...listing, limit: '64' }]) {
    assert.throws(() => checkListing(invalid), /Invalid shortcut list/);
  }
});

test('changes are posted as JSON with the session token and report the TV’s error', async () => {
  const result = await post(
    async (url, options) => {
      assert.equal(url, 'save');
      assert.equal(options.method, 'POST');
      assert.deepEqual(options.headers, {
        'Content-Type': 'application/json',
        'X-Luma-Token': 'AbCd1234',
      });
      assert.deepEqual(JSON.parse(options.body), { title: 'Gate' });
      return { ok: true, json: async () => listing };
    },
    'AbCd1234',
    'save',
    { title: 'Gate' },
  );
  assert.equal(result, listing);
  await assert.rejects(
    post(async () => ({ ok: false, json: async () => ({ error: 'Add a name' }) }), 't', 'save', {}),
    /Add a name/,
  );
  await assert.rejects(
    post(
      async () => ({ ok: false, json: async () => Promise.reject(new Error('html')) }),
      't',
      'save',
      {},
    ),
    /could not complete/,
  );
});

test('the page loads only its own scripts and uses no inline styles or handlers', async () => {
  const html = await readFile(new URL('../assets/tile-editor/index.html', import.meta.url), 'utf8');
  const script = await readFile(new URL('../assets/tile-editor/app.js', import.meta.url), 'utf8');
  assert.match(html, /<script type="module" src="app.js"><\/script>/);
  assert.doesNotMatch(html, /\sstyle=|\son[a-z]+=|https?:\/\/(?!homeassistant\.local)/);
  assert.doesNotMatch(script, /innerHTML|eval\(|new Function/);
  assert.match(script, /post\(fetch, token, route/);
});
