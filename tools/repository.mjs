import fs from 'node:fs/promises';
import path from 'node:path';
import { createHash } from 'node:crypto';

const root = path.resolve(import.meta.dirname, '..');
const manifest = JSON.parse(await fs.readFile(path.join(root, 'appinfo.json'), 'utf8'));
const pkg = JSON.parse(await fs.readFile(path.join(root, 'package.json'), 'utf8'));
if (!/^\d+\.\d+\.\d+$/.test(manifest.version) || pkg.version !== manifest.version)
  throw new Error('package.json and appinfo.json must have the same numeric release version.');
const tag = `v${manifest.version}`;
const requestedTag =
  process.env.RELEASE_TAG ||
  (process.env.GITHUB_REF_TYPE === 'tag' ? process.env.GITHUB_REF_NAME : '');
if (requestedTag && requestedTag !== tag)
  throw new Error(`Release tag must be ${tag}, matching the packaged app version.`);

const repository = process.env.GITHUB_REPOSITORY || 'ecklf/lg-luma';
if (!/^[\w.-]+\/[\w.-]+$/.test(repository)) throw new Error('Invalid GitHub repository name.');
const sourceUrl = `https://github.com/${repository}`;
const releaseUrl = `${sourceUrl}/releases/download/${tag}`;
const filename = `${manifest.id}_${manifest.version}_arm.ipk`;
const output = path.join(root, 'dist');
const bytes = await fs.readFile(path.join(output, filename));
const iconUri = `${releaseUrl}/${manifest.largeIcon}`;
const descriptionFilename = 'description.html';
const shortDescription = manifest.appDescription?.trim();
if (!shortDescription) throw new Error('appinfo.json must provide a non-empty appDescription.');
const feed = {
  paging: { page: 1, count: 1, maxPage: 1, itemsTotal: 1 },
  packages: [
    {
      id: manifest.id,
      title: manifest.title,
      shortDescription,
      fullDescriptionUrl: `${releaseUrl}/${descriptionFilename}`,
      iconUri,
      manifest: {
        id: manifest.id,
        version: manifest.version,
        type: manifest.type,
        title: manifest.title,
        appDescription: manifest.appDescription,
        iconUri,
        sourceUrl,
        rootRequired: true,
        ipkUrl: `${releaseUrl}/${filename}`,
        ipkHash: { sha256: createHash('sha256').update(bytes).digest('hex') },
        ipkSize: bytes.length,
      },
    },
  ],
};
await fs.copyFile(
  path.join(root, '.build/package', manifest.largeIcon),
  path.join(output, manifest.largeIcon),
);
await fs.copyFile(
  path.join(root, 'assets/homebrew-description.html'),
  path.join(output, descriptionFilename),
);
await fs.writeFile(path.join(output, 'repo.json'), JSON.stringify(feed, null, 2) + '\n');
const manifestFilename = `${manifest.id}.manifest.json`;
await fs.writeFile(
  path.join(output, manifestFilename),
  JSON.stringify(feed.packages[0].manifest, null, 2) + '\n',
);
if (process.env.GITHUB_OUTPUT) await fs.appendFile(process.env.GITHUB_OUTPUT, `tag=${tag}\n`);
console.log(`Homebrew feed: dist/repo.json (${tag}, ${bytes.length} byte IPK)`);
