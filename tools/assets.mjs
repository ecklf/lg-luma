import fs from 'node:fs/promises';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import sharp from 'sharp';
import { generateVectorAssets } from './vector-assets.mjs';

const root = path.resolve(import.meta.dirname, '..');
const source = path.join(root, 'assets');
const out = path.join(root, '.build/assets');
const highResolution = process.argv.includes('--4k');
// Retire generated dumps from older asset caches as well as the source assets.
for (const legacy of ['app-icons', 'icons.json', 'colors.json']) {
  await fs.rm(path.join(out, legacy), { recursive: true, force: true });
}
for (const directory of ['wallpapers', 'thumbnails', 'logos', 'ui-icons', 'surfaces']) {
  await fs.mkdir(path.join(out, directory), { recursive: true });
}

// Decode the existing licensed font, rather than introducing a different font.
const font = path.join(out, 'font.woff2');
await fs.copyFile(path.join(source, 'fonts/inter-latin.woff2'), font);
const decoded = spawnSync('woff2_decompress', [font], { encoding: 'utf8' });
if (decoded.error || decoded.status !== 0) {
  throw new Error('Font conversion needs woff2_decompress. Run pnpm assets inside nix develop.', {
    cause: decoded.error ?? new Error(decoded.stderr),
  });
}
await fs.rm(font);
const cjk = path.join(source, 'fonts/noto-sans-sc.ttf');
const hangul = path.join(source, 'fonts/noto-sans-kr.ttf');
try {
  await fs.access(cjk);
  await fs.access(hangul);
} catch {
  throw new Error('Prepare the bundled Chinese and Korean fonts with pnpm assets:fonts first.');
}
const weights = spawnSync(
  'python3',
  [path.join(import.meta.dirname, 'font-weights.py'), out, cjk, hangul],
  {
    encoding: 'utf8',
  },
);
if (weights.error || weights.status !== 0) {
  throw new Error('Font weights need Python/fontTools. Run pnpm assets inside nix develop.', {
    cause: weights.error ?? new Error(weights.stderr),
  });
}
await fs.copyFile(path.join(source, 'fonts/ofl.txt'), path.join(out, 'font-license.txt'));
await fs.copyFile(path.join(source, 'fonts/noto-ofl.txt'), path.join(out, 'font-cjk-license.txt'));

for (const file of await fs.readdir(path.join(source, 'wallpapers'))) {
  if (!/\.(jpg|svg)$/.test(file)) continue;
  const id = path.parse(file).name;
  const input = path.join(source, 'wallpapers', file);
  for (const scale of highResolution ? [1, 2] : [1]) {
    const width = 1920 * scale;
    const height = 1080 * scale;
    // Match background-size:cover and each CSS background-position exactly.
    const photo = await sharp(input, { density: 72 * scale })
      .resize(width, height, { fit: 'outside' })
      .png()
      .toBuffer({ resolveWithObject: true });
    const position = id === 'dunes' || id === 'summit' ? 0.4 : id === 'san-francisco' ? 0.42 : 0.45;
    let image = sharp(photo.data).extract({
      left: Math.round((photo.info.width - width) / 2),
      top: Math.round((photo.info.height - height) * position),
      width,
      height,
    });
    const saturation =
      id === 'san-francisco'
        ? 0.75
        : id === 'summit'
          ? 0.7
          : ['forest', 'yosemite', 'northern-lights'].includes(id)
            ? 0.8
            : 1;
    const brightness = id === 'san-francisco' ? 0.82 : id === 'summit' ? 0.8 : 1;
    if (saturation !== 1) {
      const s = saturation;
      image = image.recomb([
        [0.213 + 0.787 * s, 0.715 - 0.715 * s, 0.072 - 0.072 * s],
        [0.213 - 0.213 * s, 0.715 + 0.285 * s, 0.072 - 0.072 * s],
        [0.213 - 0.213 * s, 0.715 - 0.715 * s, 0.072 + 0.928 * s],
      ]);
    }
    const data = await image.linear(brightness).png().toBuffer();
    const shaded = `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}" viewBox="0 0 1920 1080"><defs>
    <linearGradient id="v" x2="0" y2="1"><stop stop-color="#0d1013" stop-opacity=".24"/><stop offset=".19" stop-color="#0e1115" stop-opacity=".08"/><stop offset=".35" stop-color="#0f1114" stop-opacity=".35"/><stop offset=".63" stop-color="#111317" stop-opacity=".86"/><stop offset=".85" stop-color="#17191d" stop-opacity=".92157"/><stop offset="1" stop-color="#17191d" stop-opacity=".96078"/></linearGradient>
    <linearGradient id="h"><stop stop-color="#111417" stop-opacity=".42"/><stop offset=".75" stop-color="#111417" stop-opacity="0"/></linearGradient></defs>
    <path fill="#17191c" d="M0 0h1920v1080H0z"/><image href="data:image/png;base64,${data.toString('base64')}" width="1920" height="1080" opacity=".92"/><path fill="url(#h)" d="M0 0h1920v1080H0z"/><path fill="url(#v)" d="M0 0h1920v1080H0z"/></svg>`;
    // This generated SVG embeds a trusted bundled photograph; its data URL can
    // exceed libxml's default 10 MiB attribute limit at 4K.
    await sharp(Buffer.from(shaded), { unlimited: true })
      .jpeg({ quality: 96 })
      .toFile(path.join(out, 'wallpapers', `${id}${scale === 2 ? '@2x' : ''}.jpg`));
  }
  await sharp(input)
    .resize(480, 270, { fit: 'cover' })
    .jpeg({ quality: 82 })
    .toFile(path.join(out, 'thumbnails', `${id}.jpg`));
}
await fs.writeFile(
  path.join(out, 'version.txt'),
  JSON.parse(await fs.readFile(path.join(root, 'appinfo.json'), 'utf8')).version,
);

const surfaces = {
  generic: ['#4e5c6b', '#243c4e'],
  'live-tv': ['#192a43', '#0d1628'],
  netflix: ['#141414', '#141414'],
  youtube: ['#0f0f0f', '#0f0f0f'],
  'apple-tv': ['#252525', '#080808'],
  'disney-plus': ['#0b244c', '#091936'],
  'prime-video': ['#0786bd', '#005ba7'],
  arte: ['#242323', '#121212'],
  zdf: ['#121b22', '#121b22'],
  'das-erste': ['#064c84', '#00254e'],
  plex: ['#343434', '#232323'],
  spotify: ['#000000', '#121212'],
  jellyfin: ['#25283f', '#101422'],
  homebrew: ['#7c6042', '#3f392d'],
  kodi: ['#0a749b', '#08445e'],
  moonlight: ['#435d8f', '#252e59'],
  gaming: ['#435d8f', '#252e59'],
  browser: ['#192a43', '#0d1628'],
  'hbo-max': ['#181818', '#080808'],
  'paramount-plus': ['#075bda', '#00298e'],
  peacock: ['#171717', '#080808'],
  hulu: ['#152d23', '#091710'],
  crunchyroll: ['#141414', '#141414'],
  twitch: ['#6330b8', '#32165c'],
  dazn: ['#252623', '#10110e'],
  'rakuten-tv': ['#bf143a', '#620c20'],
  'apple-music': ['#e9415e', '#a91c38'],
  'amazon-music': ['#183c4a', '#102128'],
  'bbc-iplayer': ['#4a162e', '#230b16'],
  itvx: ['#24341b', '#111a0c'],
  'channel-4': ['#20362c', '#0c1a14'],
  'now-tv': ['#064c45', '#001e1a'],
  'rtl-plus': ['#262029', '#121014'],
  joyn: ['#000000', '#000000'],
  'waipu-tv': ['#154e8e', '#0a2544'],
  zattoo: ['#353031', '#151314'],
  skyshowtime: ['#24202e', '#100d17'],
  'coupang-play': ['#143b4a', '#091d25'],
  tving: ['#3f131d', '#1a090e'],
  wavve: ['#10399e', '#0a1e53'],
  watcha: ['#3d1225', '#1e0912'],
  'movistar-plus': ['#163b35', '#091e1a'],
  filmin: ['#10372f', '#091c18'],
};
for (const [brand, [top, bottom]] of Object.entries(surfaces)) {
  const fill =
    brand === 'disney-plus'
      ? '<radialGradient id="base" cx="50%" cy="110%" r="110%"><stop stop-color="#16466b"/><stop offset=".6" stop-color="#0b244c"/><stop offset="1" stop-color="#091936"/></radialGradient>'
      : `<linearGradient id="base" x2="1" y2="1"><stop stop-color="${top}"/><stop offset="1" stop-color="${bottom}"/></linearGradient>`;
  await sharp(
    Buffer.from(
      `<svg xmlns="http://www.w3.org/2000/svg" width="540" height="309"><defs>${fill}<linearGradient id="sheen" x2="1" y2="1"><stop stop-color="white" stop-opacity=".04"/><stop offset=".48" stop-color="white" stop-opacity="0"/></linearGradient></defs><path fill="url(#base)" d="M0 0h540v309H0z"/><path fill="url(#sheen)" d="M0 0h540v309H0z"/></svg>`,
    ),
  )
    .png()
    .toFile(path.join(out, 'surfaces', `${brand}.png`));
}
await generateVectorAssets();
await sharp(path.join(source, 'app-icon.svg'))
  .resize(80, 80)
  .png()
  .toFile(path.join(out, 'app-icon.png'));
await sharp(path.join(source, 'app-icon.svg'))
  .resize(130, 130)
  .png()
  .toFile(path.join(out, 'app-icon-large.png'));
console.log(`Native artwork ready: ${path.relative(root, out)}`);
