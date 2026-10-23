// One-time native font preparation; normal asset builds and TV use are offline.
import fs from 'node:fs/promises';
import path from 'node:path';
import { createHash } from 'node:crypto';

const directory = path.resolve(import.meta.dirname, '../assets/fonts');
const base =
  'https://raw.githubusercontent.com/google/fonts/a85815a42757630ce188fdad368c2dfc444d4773/ofl';
// Google Fonts ships each CJK language as its own family: Noto Sans SC has no
// Hangul syllables, so Korean needs Noto Sans KR. Both share this OFL text.
const files = [
  [
    'noto-sans-sc.ttf',
    'notosanssc/NotoSansSC%5Bwght%5D.ttf',
    'a3041811a78c361b1de50f953c805e0244951c21c5bd412f7232ef0d899af0da',
  ],
  [
    'noto-sans-kr.ttf',
    'notosanskr/NotoSansKR%5Bwght%5D.ttf',
    '194018e6b2b293a7964f037b25c0249ce1418bc9ab3c971060a03aa57861e252',
  ],
  [
    'noto-ofl.txt',
    'notosanssc/OFL.txt',
    '1c05c68c34f9708415aada51f17e1b0092d2cea709bf4a94cd38114f9e73d7d9',
  ],
];
await fs.mkdir(directory, { recursive: true });
for (const [name, source, hash] of files) {
  const destination = path.join(directory, name);
  let bytes;
  try {
    bytes = await fs.readFile(destination);
  } catch (error) {
    if (error.code !== 'ENOENT') throw error;
  }
  if (!bytes) {
    const response = await fetch(`${base}/${source}`, { signal: AbortSignal.timeout(60000) });
    if (!response.ok) throw new Error(`Font download failed: ${response.status}`);
    bytes = Buffer.from(await response.arrayBuffer());
    if (createHash('sha256').update(bytes).digest('hex') !== hash)
      throw new Error(`Font checksum mismatch: ${name}`);
    await fs.writeFile(destination, bytes);
  } else if (createHash('sha256').update(bytes).digest('hex') !== hash) {
    throw new Error(`Bundled font differs from the pinned source: ${name}`);
  }
  console.log(`Bundled native font: ${name}`);
}
