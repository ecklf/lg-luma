// Pinned host toolchains. SDL-webOS is prepared separately by webos-runtime.mjs.
import fs from 'node:fs/promises';
import path from 'node:path';
import { spawnSync } from 'node:child_process';

const root = path.resolve(import.meta.dirname, '..');
const parent = path.join(root, '.build/toolchain');
const sdk = path.join(parent, 'arm-webos-linux-gnueabi_sdk-buildroot');
const assets = {
  'darwin-arm64':
    'https://github.com/webosbrew/native-toolchain/releases/download/webos-d7ed7ee.6/arm-webos-linux-gnueabi_sdk-buildroot_darwin-arm64.tar.bz2',
  'darwin-x64':
    'https://github.com/webosbrew/native-toolchain/releases/download/webos-d7ed7ee.6/arm-webos-linux-gnueabi_sdk-buildroot_darwin-x86_64.tar.bz2',
  'linux-x64':
    'https://github.com/openlgtv/buildroot-nc4/releases/download/webos-b17b4cc/arm-webos-linux-gnueabi_sdk-buildroot.tar.gz',
  'linux-arm64':
    'https://github.com/webosbrew/native-toolchain/releases/download/webos-d7ed7ee.6/arm-webos-linux-gnueabi_sdk-buildroot_linux-aarch64.tar.bz2',
};
const url = assets[`${process.platform}-${process.arch}`];
if (!url) throw new Error(`No pinned webOS SDK for ${process.platform}/${process.arch}.`);
if (/[\s]/.test(sdk)) throw new Error('Extract the webOS SDK in a path without spaces.');
await fs.mkdir(parent, { recursive: true });
try {
  await fs.access(path.join(sdk, 'bin/arm-webos-linux-gnueabi-gcc'));
} catch {
  const archive = path.join(parent, path.basename(url));
  const downloaded = spawnSync(
    'curl',
    [
      '--fail',
      '--location',
      '--retry',
      '3',
      // GitHub release downloads can transiently return 403 as well as 5xx.
      '--retry-all-errors',
      '--silent',
      '--show-error',
      '--output',
      archive,
      url,
    ],
    { stdio: 'inherit', timeout: 300000 },
  );
  if (downloaded.error) throw downloaded.error;
  if (downloaded.status !== 0) throw new Error('webOS SDK download failed.');
  const extracted = spawnSync(
    'tar',
    [url.endsWith('.bz2') ? '-xjf' : '-xzf', archive, '-C', parent],
    { stdio: 'inherit', timeout: 120000 },
  );
  if (extracted.error) throw extracted.error;
  if (extracted.status !== 0) throw new Error('webOS SDK extraction failed.');
}
const relocated = spawnSync('sh', [path.join(sdk, 'relocate-sdk.sh')], {
  stdio: 'inherit',
  timeout: 120000,
});
if (relocated.error) throw relocated.error;
if (relocated.status !== 0) throw new Error('webOS SDK relocation failed.');
console.log(`webOS SDK ready. Set WEBOS_NDK=${sdk}`);
