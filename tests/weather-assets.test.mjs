import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import fs from 'node:fs/promises';
import test from 'node:test';
import sharp from 'sharp';

const directory = new URL('../assets/weather/', import.meta.url);

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
