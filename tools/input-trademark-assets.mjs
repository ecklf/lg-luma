// Optional source refresh. Normal builds use the checked-in artwork offline.
import fs from 'node:fs/promises';
import path from 'node:path';
import { createHash } from 'node:crypto';

export const sources = [
  ['sony', 'Sony_logo.svg', 'f6191bc69f8df259c86a13dc9582ba9ed2e0101d4452ba3f1d33ec8e907ddda0'],
  ['dvd', 'DVD_logo.svg', 'a11fcc392c029e6496a76a493702c030f91f07bf112cccdf232d92b6b3fd065c'],
  [
    'blu-ray',
    'Blu-ray_Disc.svg',
    '575bfcc076b9b137c71b3aa039c1ac1aa2d0fcfb667774496250b82a0dc2d7b1',
  ],
  [
    'disk',
    'Compact_Disc_wordmark.svg',
    'fd0c6fa71b72b6887611511b99e25dca128ec46d0f62aaf246932bc3c29103b1',
  ],
];

export function sourceUrl(file) {
  const hash = createHash('md5').update(file).digest('hex');
  return `https://upload.wikimedia.org/wikipedia/commons/${hash[0]}/${hash.slice(0, 2)}/${encodeURIComponent(file)}`;
}

export function prepareSvg(original, file) {
  // Preserve the trademark paths and viewBox. A square viewport centers the
  // complete mark without stretching it; the mesh compiler applies white ink.
  const svg = original
    .slice(original.indexOf('<svg'), original.lastIndexOf('</svg>') + 6)
    .replaceAll('\r\n', '\n')
    .replaceAll('&ns_svg;', 'http://www.w3.org/2000/svg')
    .replaceAll('&ns_xlink;', 'http://www.w3.org/1999/xlink')
    .replace(/<svg\b[^>]*>/, (root) =>
      root.replace(/\swidth="[^"]*"/, ' width="24"').replace(/\sheight="[^"]*"/, ' height="24"'),
    );
  return `<!-- Source: ${sourceUrl(file)}; trademark artwork, see assets/licenses/input-trademarks.md -->\n${svg.trim()}\n`;
}

if (process.argv[1] && path.resolve(process.argv[1]) === import.meta.filename) {
  for (const [name, file, checksum] of sources) {
    const response = await fetch(sourceUrl(file), { signal: AbortSignal.timeout(60000) });
    if (!response.ok) throw new Error(`Cannot fetch ${file}: ${response.status}`);
    const original = await response.text();
    if (createHash('sha256').update(original).digest('hex') !== checksum) {
      throw new Error(`Source checksum mismatch: ${file}`);
    }
    await fs.writeFile(
      new URL(`../assets/ui-icons/input-${name}.svg`, import.meta.url),
      prepareSvg(original, file),
    );
    console.log(`Restored trademark artwork: ${name}`);
  }
}
