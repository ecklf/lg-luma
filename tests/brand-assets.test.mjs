import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import test from 'node:test';
import sharp from 'sharp';
import { sources, prepareSvg } from '../tools/brand-assets.mjs';

test('every expanded brand has standalone, renderable vector artwork and pinned provenance', async () => {
  assert.equal(sources.length, 27);
  for (const [name, url, hash] of sources) {
    assert.match(hash, /^[0-9a-f]{64}$/, name);
    const svg = await fs.readFile(new URL(`../assets/logos/${name}.svg`, import.meta.url), 'utf8');
    assert.ok(svg.includes(url), `Missing source attribution for ${name}`);
    assert.doesNotMatch(svg, /<image\b|<text\b|=\{/, name);
    const { data, info } = await sharp(Buffer.from(svg))
      .resize(256, 128, { fit: 'inside' })
      .ensureAlpha()
      .raw()
      .toBuffer({ resolveWithObject: true });
    assert.ok(
      data.some((value, index) => index % info.channels === 3 && value > 0),
      `Empty logo: ${name}`,
    );
  }
});

test('source preparation preserves paths and removes component syntax and redundant clipping', () => {
  const svg = prepareSvg(
    'coupang-play',
    'export default () => (<svg width={115} height={22} style={{ width: 115 }}><g clip-path="url(#bounds)"><path d="M0 0L1 1L1 0Z"/></g><defs><clipPath id="bounds"/></defs></svg>);',
    'https://example.com/source',
  );
  assert.match(svg, /width="115"/);
  assert.match(svg, /d="M0 0L1 1L1 0Z"/);
  assert.doesNotMatch(svg, /export|style=|clip-path|<defs>|=\{/);
  assert.doesNotMatch(
    prepareSvg('hulu', '<?xml version="1.0"?><svg/>', 'https://example.com/source'),
    /<\?xml/,
  );
});
