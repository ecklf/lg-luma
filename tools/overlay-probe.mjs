// Package a separate diagnostic app. Never install or launch it implicitly.
import fs from 'node:fs/promises';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import sharp from 'sharp';

const root = path.resolve(import.meta.dirname, '..');
const stage = path.join(root, '.build/overlay-probe/app');
const output = path.join(root, '.build/overlay-probe/ipk');
await fs.mkdir(stage, { recursive: true });
await fs.mkdir(output, { recursive: true });
for (const name of ['appinfo.json', 'main.qml']) {
  await fs.copyFile(path.join(root, 'tools/overlay-probe', name), path.join(stage, name));
}
await sharp(path.join(root, 'assets/app-icon.svg'))
  .resize(80, 80)
  .png()
  .toFile(path.join(stage, 'icon.png'));
const cli = path.join(root, 'node_modules/@webos-tools/cli/bin/ares-package.js');
const result = spawnSync(process.execPath, [cli, stage, '--outdir', output, '--no-minify'], {
  cwd: root,
  stdio: 'inherit',
  timeout: 120000,
});
if (result.error) throw result.error;
if (result.status !== 0) process.exit(result.status ?? 1);
console.log(`Probe IPK: ${output}`);
