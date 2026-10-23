// Optional host-only source refresh. Normal builds use the checked-in SVGs and
// never contact these hosts. Hashes cover the original bytes before adaptation.
import fs from 'node:fs/promises';
import path from 'node:path';
import { createHash } from 'node:crypto';

const root = path.resolve(import.meta.dirname, '..');
const simple =
  'https://raw.githubusercontent.com/simple-icons/simple-icons/90bd1f3e7b77851fd0c56474f6fa43b816babdab';
const commons = (file) => {
  file = decodeURIComponent(file);
  const hash = createHash('md5').update(file).digest('hex');
  return `https://upload.wikimedia.org/wikipedia/commons/${hash[0]}/${hash.slice(0, 2)}/${encodeURIComponent(file)}`;
};
export const sources = [
  [
    'spotify',
    commons('Spotify_logo_with_text.svg'),
    'c0e57c534e7fce5e66fb419c269b97d436385a2c69b9f508edf480ef60dedf91',
  ],
  [
    'plex',
    `${simple}/icons/plex.svg`,
    '129500c2bda2175c95402d12e0a96642de61a2561d45a4d9719edc8abd43c8ea',
  ],
  [
    'hbo-max',
    commons('HBO_Max_%282025%29.svg'),
    'c8728242f74e388ef3a507745875335528a4980123f2c4cae9f08d666fbda63b',
  ],
  [
    'paramount-plus',
    `${simple}/icons/paramountplus.svg`,
    '6b8a777222266eefdb08b9737cef97cdecf10143f5402d06634f615b127851f9',
  ],
  [
    'peacock',
    commons('NBCUniversal_Peacock_Logo_%282026%29.svg'),
    'bf53d69c5deda4de4db1997d0616cfc43ede490f5634003bfcc5094477595f2c',
  ],
  [
    'hulu',
    commons('Hulu_logo_%282018%29.svg'),
    '6bf705502216ffd2ad58721bcb1b76b93d3a2a31995dfa919ef34a847d038e1f',
  ],
  [
    'crunchyroll',
    `${simple}/icons/crunchyroll.svg`,
    '3b9c3d87339e18ec09f25e0c3eaffdc5ad4630df3d108fcd907e1b64c4cd13ea',
  ],
  [
    'twitch',
    `${simple}/icons/twitch.svg`,
    'dbe27bc02a01b8d89b6259cacef328c9fa70bb13c29f38c36eb1421863f6f0b6',
  ],
  [
    'dazn',
    `${simple}/icons/dazn.svg`,
    '19d9da58d96ee8bd05bf7253f706203dd0c1762469523b8960eb8efc3b1017ef',
  ],
  [
    'rakuten-tv',
    commons('Rakuten_TV_logo.svg'),
    '43b83db4948b04a3072ce264265452e59a65a4ccc4e32c710c73044ebf49ae86',
  ],
  [
    'apple-music',
    commons('Apple_Music_Logo_%282026%29.svg'),
    '974eab4dbecce108dcfc5299bce64a5461eb57a56a259d59d9e52bc4d9b29556',
  ],
  [
    'amazon-music',
    commons('Amazon_Music_logo.svg'),
    '71b407417a0a20e42c79e6e0a643c76fbd56e4aaacc0b7335178e13fd458ef60',
  ],
  [
    'bbc-iplayer',
    commons('BBC_iPlayer_2021_%28Alt%29.svg'),
    '05325387e1af09f7f177eb4dbb20225793949f92f6295bfd8604a01c2c98f63e',
  ],
  [
    'itvx',
    `${simple}/icons/itvx.svg`,
    'eafc08ffdda9c16ec292c03ed795a1111945240670f50ef6515b896efc8b4153',
  ],
  [
    'channel-4',
    `${simple}/icons/channel4.svg`,
    'cb7205ad42186cf48bb30e350753d0323e1f4efd03a33749c7e17e7868d4978a',
  ],
  [
    'now-tv',
    `${simple}/icons/now.svg`,
    'bcffe72bc6cc06123c6e2b736fb7c2d2ff432025ebd014118702b5b670d17a85',
  ],
  [
    'rtl-plus',
    commons('RTL%2B_Logo_2021.svg'),
    '4f28b89be7c76acec5b467e66df51fb332ea0c46a9cde4f3be6fb5270c105884',
  ],
  [
    'joyn',
    commons('Joyn_logo.svg'),
    'a5b387fbb02c2be6c1408e0d56d4c4ee32fd19eaf86a776fa05de38d6afb0570',
  ],
  [
    'waipu-tv',
    commons('Waipu_TV_Logo_2016.svg'),
    '1013fbb5274899a386f57b34413165b9a66933a1614855c5acaa2553a942be97',
  ],
  [
    'zattoo',
    commons('Logo_Zattoo.svg'),
    '2df3c7eaf5e953f2aef43353c622f6c6c06688a9649995b88effb2d1fc0fce5d',
  ],
  [
    'skyshowtime',
    commons('SkyShowtime_Logo.svg'),
    'da31ab32038e050cb759335057243054034556117552fa88d4e14457503daa92',
  ],
  [
    'coupang-play',
    'https://raw.githubusercontent.com/SinJungho/Damoa_Team-Project/db1955f0f6f09d9329518120afe42af375632de0/src/svg/CoupangPlayLogo.jsx',
    '0069daf83ee2b3389d6082d8dd09ed2007872752beab3191ba7922935ac99c36',
  ],
  [
    'tving',
    commons('TVING_logo.svg'),
    '4dd2952a96b1b0b4d879d1b6444e137806cddee3cd8352145d7266f4f726964c',
  ],
  [
    'wavve',
    'https://www.wavve.com/static/element-icons-tag-ic-brand-wavve-b343dcb2.svg',
    'b343dcb207257023fe27845d230699d6320134b780e8571f1e71095f8c71f0a8',
  ],
  [
    'watcha',
    commons('Watcha_logo.svg'),
    '6d1239c5fd4d9ad9e8dd54d07e13c65bf970552195ee6243ace814d3c5aff18a',
  ],
  [
    'movistar-plus',
    commons('Movistar_Plus%2B_2022_logo.svg'),
    'f8b775c43d273a9e1d97b539c22356392146271c89a9f2a9fb851863ea0600a8',
  ],
  [
    'filmin',
    'https://raw.githubusercontent.com/DCueto/filmin-svelte-clone/36c977e31ec76abb42473b243584acbaed23579f/src/assets/filmin_logo.svg',
    'f9fbaec75b29bedec27236bf3717701b8871344e5bcb0ac8bc2e8c9a0c127b4d',
  ],
];

async function download(url) {
  const response = await fetch(url, { signal: AbortSignal.timeout(60000) });
  if (!response.ok) throw new Error(`${response.status}: ${url}`);
  const text = await response.text();
  if (!text.includes('<svg') || text.includes('data:image/'))
    throw new Error(`Not standalone vector artwork: ${url}`);
  return text;
}

// The Coupang SVG was distributed as a component. Keep only its static SVG
// paths, resolving numeric JSX attributes and removing its redundant bounds clip.
export function prepareSvg(name, original, url) {
  let svg = original.replace(/<\?xml[^?]*\?>/, '');
  if (name === 'crunchyroll') {
    svg = svg.replace('<svg ', '<svg fill="#FF5E00" ');
  }
  if (name === 'coupang-play') {
    svg = svg
      .slice(svg.indexOf('<svg'), svg.lastIndexOf('</svg>') + 6)
      .replace(/style=\{\{[\s\S]*?\}\}/g, '')
      .replace(/=\{([\d.]+)\}/g, '="$1"')
      .replace(/<defs>[\s\S]*?<\/defs>/g, '')
      .replace(/\sclip-path="[^"]*"/g, '');
  }
  if (['peacock', 'watcha'].includes(name)) {
    // These clips only repeat the artwork's rectangular outer bounds. Keep
    // gradient definitions while removing the redundant clipping wrapper.
    svg = svg
      .replace(/<clipPath\b[^>]*>[\s\S]*?<\/clipPath>/g, '')
      .replace(/\sclip-path="[^"]*"/g, '');
  }
  // Source provenance stays beside the original paths. Display-only white
  // treatments and tight bounds are applied by the host mesh compiler.
  return `<!-- Source: ${url} -->\n${svg.trim()}\n`;
}

if (process.argv[1] && path.resolve(process.argv[1]) === import.meta.filename) {
  const checksums = process.argv.includes('--checksums');
  const only = process.argv.includes('--only')
    ? process.argv[process.argv.indexOf('--only') + 1]
    : undefined;
  if (process.argv.includes('--only') && !sources.some(([name]) => name === only)) {
    throw new Error('Pass a known brand name after --only');
  }
  for (const [name, url, expected] of sources) {
    if (only && name !== only) continue;
    let original;
    try {
      original = await download(url);
    } catch (error) {
      if (!checksums) throw error;
      console.error(`${name}: ${error.message}`);
      continue;
    }
    const digest = createHash('sha256').update(original).digest('hex');
    if (checksums) {
      console.log(`${name} ${digest} ${Buffer.byteLength(original)}`);
      continue;
    }
    if (digest !== expected) throw new Error(`Source checksum mismatch: ${name}`);
    await fs.writeFile(
      path.join(root, 'assets/logos', `${name}.svg`),
      prepareSvg(name, original, url),
    );
    console.log(`Restored vector source: ${name}`);
  }
  if (!checksums && !only) {
    const response = await fetch(`${simple}/LICENSE.md`, { signal: AbortSignal.timeout(60000) });
    if (!response.ok) throw new Error('Could not restore the Simple Icons CC0 notice');
    await fs.writeFile(path.join(root, 'assets/licenses/simple-icons.txt'), await response.text());
  }
}
