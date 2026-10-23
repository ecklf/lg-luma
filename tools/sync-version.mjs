import fs from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.resolve(import.meta.dirname, '..');

function replaceVersion(source, pattern, version, filename) {
  if ([...source.matchAll(pattern)].length !== 1) {
    throw new Error(`Expected one release version in ${filename}.`);
  }
  return source.replace(pattern, (_match, prefix, suffix) => `${prefix}${version}${suffix}`);
}

function updateTomlVersion(source, header, version, filename) {
  let packages = 0;
  const updated = source
    .split(/(?=^\[)/m)
    .map((section) => {
      if (
        section.split(/\r?\n/, 1)[0].trim() !== header ||
        !/^name[ \t]*=[ \t]*"luma"[ \t]*\r?$/m.test(section)
      ) {
        return section;
      }
      packages++;
      return replaceVersion(
        section,
        /^([ \t]*version[ \t]*=[ \t]*")[^"\r\n]+(")/gm,
        version,
        filename,
      );
    })
    .join('');
  if (packages !== 1) throw new Error(`Expected one luma package in ${filename}.`);
  return updated;
}

export async function syncVersions(directory = root, { check = false } = {}) {
  const read = (filename) => fs.readFile(path.join(directory, filename), 'utf8');
  const { version } = JSON.parse(await read('package.json'));
  if (typeof version !== 'string' || !/^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.test(version)) {
    throw new Error('package.json must have a numeric major.minor.patch release version.');
  }

  const filenames = ['appinfo.json', 'Cargo.toml', 'Cargo.lock', 'readme.md'];
  const sources = await Promise.all(filenames.map(read));
  JSON.parse(sources[0]);
  const ipkPattern = /\bcom\.ecklf\.luma_\d+\.\d+\.\d+_arm\.ipk\b/g;
  if (![...sources[3].matchAll(ipkPattern)].length) {
    throw new Error('Expected versioned Luma IPK examples in readme.md.');
  }
  const updated = [
    replaceVersion(
      sources[0],
      /^([ \t]*"version"[ \t]*:[ \t]*")[^"\r\n]+(")/gm,
      version,
      'appinfo.json',
    ),
    updateTomlVersion(sources[1], '[package]', version, 'Cargo.toml'),
    updateTomlVersion(sources[2], '[[package]]', version, 'Cargo.lock'),
    sources[3].replace(ipkPattern, `com.ecklf.luma_${version}_arm.ipk`),
  ];
  const changed = filenames.filter((_filename, index) => sources[index] !== updated[index]);
  if (check && changed.length) {
    throw new Error(
      `Release versions are out of sync: ${changed.join(', ')}. Run pnpm version:sync.`,
    );
  }
  if (!check) {
    await Promise.all(
      filenames.map((filename, index) =>
        sources[index] === updated[index]
          ? undefined
          : fs.writeFile(path.join(directory, filename), updated[index]),
      ),
    );
  }
  return { version, changed };
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const check = process.argv.includes('--check');
  const { version, changed } = await syncVersions(root, { check });
  console.log(
    check
      ? `Release versions match ${version}.`
      : `Synchronized ${version}: ${changed.join(', ') || 'no changes'}.`,
  );
}
