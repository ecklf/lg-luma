import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import fs from 'node:fs/promises';
import test from 'node:test';
import sharp from 'sharp';

const directory = new URL('../assets/weather/', import.meta.url);

test('additional weather sets retain pinned sources, editable artwork and licenses', async () => {
  const manifest = JSON.parse(await fs.readFile(new URL('sources.json', directory), 'utf8'));
  assert.equal(Object.keys(manifest).length, 27);
  for (const [file, { url, sha256 }] of Object.entries(manifest)) {
    assert.match(url, /^https:\/\/raw\.githubusercontent\.com\/[^/]+\/[^/]+\/[a-f0-9]{40}\//);
    const data = await fs.readFile(new URL(file, directory));
    assert.equal(createHash('sha256').update(data).digest('hex'), sha256, file);
    if (file.endsWith('.svg')) assert.ok((await sharp(data).stats()).channels[3].max > 0, file);
  }
  for (const set of ['kickstand', 'kuankuan']) {
    const license = await fs.readFile(new URL(`${set}/LICENSE`, directory), 'utf8');
    assert.match(license, set === 'kickstand' ? /SIL OPEN FONT LICENSE/ : /Mozilla Public License/);
    for (const condition of [
      'sun',
      'moon',
      'partly-cloudy',
      'partly-cloudy-night',
      'cloud',
      'fog',
      'drizzle',
      'rain',
      'snow',
      'storm',
    ]) {
      const file = `${set}/${set === 'kickstand' && condition === 'drizzle' ? 'rain' : condition}.svg`;
      const data = await fs.readFile(new URL(file, directory));
      assert.ok((await sharp(data).ensureAlpha().stats()).channels[3].max > 0, file);
    }
  }
  const credits = await fs.readFile(new URL('../assets/attributions.txt', import.meta.url), 'utf8');
  for (const name of ['Breezy Weather', 'Lucide', 'Kickstand', 'Kuankuan', 'Open-Meteo'])
    assert.ok(credits.includes(name));
  assert.ok(!credits.includes('Icons8'));
});

test('outline weather icons cover every day and night condition', async () => {
  for (const name of [
    'sun',
    'moon',
    'partly-cloudy',
    'partly-cloudy-night',
    'cloud',
    'fog',
    'drizzle',
    'rain',
    'snow',
    'storm',
  ]) {
    const svg = await fs.readFile(
      new URL(`../assets/ui-icons/weather-${name}.svg`, import.meta.url),
    );
    assert.match(svg.toString(), /stroke="currentColor"/);
    assert.match(svg.toString(), /fill="none"/);
    const { data, info } = await sharp(svg)
      .ensureAlpha()
      .raw()
      .toBuffer({ resolveWithObject: true });
    let visible = 0;
    for (let i = 0; i < data.length; i += info.channels) {
      if (!data[i + 3]) continue;
      visible++;
      assert.equal(data[i], data[i + 1], name);
      assert.equal(data[i + 1], data[i + 2], name);
    }
    assert.ok(visible > 100, name);
  }
});

test('Breezy weather artwork preserves pinned source bytes, color, alpha and licenses', async () => {
  const hashes = JSON.parse(await fs.readFile(new URL('checksums.json', directory), 'utf8'));
  assert.equal(Object.keys(hashes).length, 10);
  for (const [file, expected] of Object.entries(hashes)) {
    const data = await fs.readFile(new URL(file, directory));
    assert.equal(createHash('sha256').update(data).digest('hex'), expected, file);
    const metadata = await sharp(data).metadata();
    assert.equal(metadata.format, 'png');
    assert.ok(metadata.width >= 112 && metadata.height >= 112, file);
    assert.equal(metadata.hasAlpha, true, file);
    const rgba = await sharp(data).toColourspace('srgb').ensureAlpha().png().toBuffer();
    const stats = await sharp(rgba).stats();
    assert.equal(stats.channels[3].min, 0, file);
    assert.ok(
      stats.channels.slice(0, 3).some((channel) => channel.stdev > 10),
      file,
    );
  }
  for (const file of ['COPYING', 'COPYING.LESSER', 'LICENSE_ADDITIONAL', 'README.md']) {
    assert.ok((await fs.readFile(new URL(file, directory), 'utf8')).length > 100);
  }
});
