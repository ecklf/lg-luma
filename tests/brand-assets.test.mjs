import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import test from 'node:test';
import sharp from 'sharp';
import { createHash } from 'node:crypto';
import { sources, prepareSvg } from '../tools/brand-assets.mjs';
import {
  sources as inputSources,
  sourceUrl,
  prepareSvg as prepareInputSvg,
} from '../tools/input-trademark-assets.mjs';

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

test('input trademark marks retain pinned artwork and render without distortion', async () => {
  const hashes = {
    sony: 'd71dc296ee3e376ff1082776d0ba7b9a779ae13909b696635ce87e112d2f70c7',
    dvd: '23cbe414dcf8eec711d1ca58cce9319a1ce13b5095a69b16e85c35c05fba71ec',
    'blu-ray': '28fcf3afb168cafdc8be275ec8b0d3f06475b56a5a8f6eeb82413434da1ecb18',
  };
  for (const [name, file, originalHash] of inputSources) {
    assert.match(originalHash, /^[0-9a-f]{64}$/);
    const svg = await fs.readFile(
      new URL(`../assets/ui-icons/input-${name}.svg`, import.meta.url),
      'utf8',
    );
    assert.equal(createHash('sha256').update(svg).digest('hex'), hashes[name]);
    assert.ok(svg.includes(sourceUrl(file)));
    assert.doesNotMatch(svg, /<image\b|<text\b|&ns_/);
    const { data, info } = await sharp(Buffer.from(svg))
      .resize(128, 128)
      .ensureAlpha()
      .raw()
      .toBuffer({ resolveWithObject: true });
    assert.ok(
      data.some((value, index) => index % info.channels === 3 && value > 0),
      name,
    );
    assert.equal(info.width, info.height);
  }
  const mark =
    '<svg width="512" height="128" viewBox="0 0 512 128"><path d="M0 0h512v128H0z"/></svg>';
  const prepared = prepareInputSvg(mark, 'test.svg');
  assert.ok(prepared.includes('viewBox="0 0 512 128"'));
  assert.ok(prepared.includes('<path d="M0 0h512v128H0z"/>'));
  assert.match(prepared, /width="24" height="24"/);
});

test('disk input uses a generic disc icon rather than the Compact Disc wordmark', async () => {
  assert.ok(!inputSources.some(([name]) => name === 'disk'));
  const svg = await fs.readFile(
    new URL('../assets/ui-icons/input-disk.svg', import.meta.url),
    'utf8',
  );
  assert.doesNotMatch(svg, /Compact_Disc|<image\b|<text\b/);
  assert.match(svg, /<circle/);
  assert.match(svg, /color="#fff"/);
  const { data } = await sharp(Buffer.from(svg))
    .resize(128, 128)
    .ensureAlpha()
    .raw()
    .toBuffer({ resolveWithObject: true });
  assert.ok(data.some((value, index) => index % 4 === 3 && value > 0));
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
