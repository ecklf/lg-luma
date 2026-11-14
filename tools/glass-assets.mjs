import path from 'node:path';
import { spawnSync } from 'node:child_process';

const root = path.resolve(import.meta.dirname, '..');
const result = spawnSync(
  'cargo',
  [
    'run',
    '--locked',
    '--release',
    '--manifest-path',
    path.join(root, 'tools/vector-assets/Cargo.toml'),
    '--bin',
    'glass-assets',
    '--',
    path.join(root, '.build/assets/wallpapers'),
  ],
  {
    cwd: root,
    env: { ...process.env, CARGO_TARGET_DIR: path.join(root, '.build/vector-assets-target') },
    stdio: 'inherit',
    timeout: 600000,
  },
);
if (result.error) throw result.error;
if (result.status !== 0) throw new Error('Glass backdrop preparation failed.');
