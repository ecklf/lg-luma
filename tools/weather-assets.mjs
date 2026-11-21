// Optional restoration of pinned third-party weather sources. Ordinary builds are offline.
import fs from 'node:fs/promises';
import path from 'node:path';
import { createHash } from 'node:crypto';

const root = path.resolve(import.meta.dirname, '..');
const sets = {
  kickstand: {
    repository: 'kickstandapps/WeatherIcons',
    revision: '289e1fb1804496494addf945b9f0837999fde6f9',
    files: {
      sun: 'Sun',
      moon: 'Moon',
      'partly-cloudy': 'PartlySunny',
      'partly-cloudy-night': 'PartlyMoon',
      cloud: 'Cloud',
      fog: 'Haze',
      rain: 'Rain',
      snow: 'Snow',
      storm: 'Storm',
    },
  },
  kuankuan: {
    repository: 'kuankuan2007/weather-icon',
    revision: '53a7a6459a7d0192f8381b604be8f02a0f674d84',
    files: {
      sun: '晴',
      'partly-cloudy': '多云',
      cloud: '阴',
      fog: '雾',
      drizzle: '毛毛雨-细雨',
      rain: '雨',
      snow: '雪',
      storm: '雷阵雨',
    },
  },
};
const manifest = {};
const manifestPath = path.join(root, 'assets/weather/sources.json');
const pinned = JSON.parse(await fs.readFile(manifestPath, 'utf8'));
for (const [set, { repository, revision, files }] of Object.entries(sets)) {
  const directory = path.join(root, 'assets/weather', set);
  await fs.mkdir(directory, { recursive: true });
  const download = async (upstream, local) => {
    const url = `https://raw.githubusercontent.com/${repository}/${revision}/${upstream.split('/').map(encodeURIComponent).join('/')}`;
    const response = await fetch(url);
    if (!response.ok) throw new Error(`Weather source ${url}: ${response.status}`);
    const bytes = Buffer.from(await response.arrayBuffer());
    const sha256 = createHash('sha256').update(bytes).digest('hex');
    if (pinned[`${set}/${local}`]?.sha256 !== sha256) {
      throw new Error(`Weather source checksum mismatch: ${set}/${local}`);
    }
    await fs.mkdir(path.dirname(path.join(directory, local)), { recursive: true });
    await fs.writeFile(path.join(directory, local), bytes);
    manifest[`${set}/${local}`] = {
      url,
      sha256,
    };
  };
  for (const [name, upstream] of Object.entries(files)) {
    await download(`${set === 'kickstand' ? 'SVG Files' : 'svg'}/${upstream}.svg`, `${name}.svg`);
    if (set === 'kuankuan') await download(`ai/${upstream}.ai`, `source/${name}.ai`);
  }
  await download(set === 'kickstand' ? 'License.txt' : 'LICENSE', 'LICENSE');
}
// Source restoration never silently updates the pinned provenance manifest.
if (JSON.stringify(manifest) !== JSON.stringify(pinned))
  throw new Error('Weather provenance mismatch');
