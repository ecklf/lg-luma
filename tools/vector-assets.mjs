import fs from 'node:fs/promises';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { pathToFileURL } from 'node:url';

const root = path.resolve(import.meta.dirname, '..');
const out = path.join(root, '.build/assets');
const sources = path.join(root, '.build/vector-sources');

export async function generateVectorAssets() {
  await fs.mkdir(sources, { recursive: true });
  const assets = [];
  for (const file of await fs.readdir(path.join(root, 'assets/logos'))) {
    if (!file.endsWith('.svg')) continue;
    const name = path.parse(file).name;
    const expanded = ![
      'apple-tv',
      'disney-plus',
      'prime-video',
      'das-erste',
      'netflix',
      'youtube',
      'arte',
      'zdf',
      'jellyfin',
    ].includes(name);
    assets.push({
      source: path.join(root, 'assets/logos', file),
      output: path.join(out, 'logos', `${name}.lvec`),
      monochrome: [
        'apple-tv',
        'disney-plus',
        'prime-video',
        'das-erste',
        'plex',
        'hbo-max',
        'paramount-plus',
        'twitch',
        'dazn',
        'rakuten-tv',
        'apple-music',
        'amazon-music',
        'bbc-iplayer',
        'itvx',
        'channel-4',
        'now-tv',
        'joyn',
        'waipu-tv',
        'zattoo',
        'skyshowtime',
        'movistar-plus',
      ].includes(name),
      light_ink: expanded && !['live-tv', 'browser'].includes(name),
      trim: expanded,
      reference_size: 1536,
    });
  }

  // Standalone SVG sources preserve the shipped Lucide and input artwork.
  // No React renderer, JSX loader or JavaScript icon dependency is needed.
  for (const file of await fs.readdir(path.join(root, 'assets/ui-icons'))) {
    if (!file.endsWith('.svg')) continue;
    const name = path.parse(file).name;
    assets.push({
      source: path.join(root, 'assets/ui-icons', file),
      output: path.join(out, 'ui-icons', `${name}.lvec`),
      reference_size: 256,
    });
  }

  const manifest = path.join(sources, 'manifest.json');
  await fs.writeFile(manifest, JSON.stringify(assets));
  const result = spawnSync(
    'cargo',
    [
      'run',
      '--locked',
      '--release',
      '--manifest-path',
      path.join(root, 'tools/vector-assets/Cargo.toml'),
      '--',
      manifest,
    ],
    {
      cwd: root,
      env: {
        ...process.env,
        CARGO_TARGET_DIR: path.join(root, '.build/vector-assets-target'),
      },
      stdio: 'inherit',
      timeout: 120000,
    },
  );
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error('Native SVG geometry compilation failed.');
  await fs.writeFile(
    path.join(out, 'vector-artwork.json'),
    JSON.stringify({
      version: 1,
      assets: assets.map((asset) => path.relative(out, asset.output)),
    }),
  );
  // These directories contain generated artwork only. Do not package stale raster
  // versions alongside the meshes when reusing an older native asset cache.
  for (const directory of ['logos', 'ui-icons']) {
    for (const file of await fs.readdir(path.join(out, directory))) {
      if (file.endsWith('.png')) await fs.rm(path.join(out, directory, file));
    }
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href) {
  await generateVectorAssets();
}
