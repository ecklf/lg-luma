import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import test from 'node:test';
import sharp from 'sharp';
import icons from '../assets/app-icons.json' with { type: 'json' };
import colors from '../assets/icon-colours.json' with { type: 'json' };

const root = path.resolve(import.meta.dirname, '..');

test('bundled app icons decode in the supported formats and have valid card colours', async () => {
  for (const [id, relative] of Object.entries(icons)) {
    assert.match(relative, /^\.\/app-icons\/[a-zA-Z0-9._-]+\.(png|jpg)$/, id);
    const image = sharp(path.join(root, 'assets', relative));
    const metadata = await image.metadata();
    assert.ok(['png', 'jpeg'].includes(metadata.format), id);
    assert.ok(metadata.width > 0 && metadata.height > 0, id);
    await image.raw().toBuffer();
    assert.ok(
      Array.isArray(colors[id]) && colors[id].length === 3,
      `Missing card colour for ${id}`,
    );
    assert.ok(
      colors[id].every((channel) => Number.isInteger(channel) && channel >= 0 && channel <= 255),
      id,
    );
  }
});

test('bundled card colours match the icon colour generator', async (t) => {
  const scratch = path.join(root, '.build');
  await fs.mkdir(scratch, { recursive: true });
  const directory = await fs.mkdtemp(path.join(scratch, 'icon-colours-test-'));
  t.after(() => fs.rm(directory, { recursive: true, force: true }));
  await fs.mkdir(path.join(directory, 'assets'));
  await fs.writeFile(path.join(directory, 'assets/app-icons.json'), JSON.stringify(icons));
  await fs.symlink(
    path.join(root, 'assets/app-icons'),
    path.join(directory, 'assets/app-icons'),
    'dir',
  );
  const result = spawnSync(process.execPath, [path.join(root, 'tools/icon-colours.mjs')], {
    cwd: directory,
    encoding: 'utf8',
    timeout: 30000,
  });
  assert.equal(result.status, 0, `${result.stdout}\n${result.stderr}`);
  const generated = JSON.parse(
    await fs.readFile(path.join(directory, 'assets/icon-colours.json'), 'utf8'),
  );
  assert.deepEqual(colors, generated, 'Run node tools/icon-colours.mjs after changing app artwork');
});
