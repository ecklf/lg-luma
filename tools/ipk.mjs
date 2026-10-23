// Inspect the actual IPK, and normalize only Luma-owned entries. The webOS CLI
// emits world-writable directories; boot scripts need app-owned 0755 parents.
import { gunzipSync, gzipSync } from 'node:zlib';

function members(bytes) {
  if (bytes.subarray(0, 8).toString() !== '!<arch>\n') throw new Error('Not an IPK/ar archive.');
  const result = [];
  for (let offset = 8; offset + 60 <= bytes.length;) {
    const header = Buffer.from(bytes.subarray(offset, offset + 60));
    const size = Number(header.subarray(48, 58).toString().trim());
    if (!Number.isSafeInteger(size) || size < 0 || offset + 60 + size > bytes.length)
      throw new Error('Invalid IPK archive member.');
    result.push({
      header,
      name: header.subarray(0, 16).toString().trim().replace(/\/$/, ''),
      data: bytes.subarray(offset + 60, offset + 60 + size),
    });
    offset += 60 + size + (size % 2);
  }
  return result;
}

function entries(tar) {
  const result = [];
  for (let offset = 0; offset + 512 <= tar.length;) {
    const header = tar.subarray(offset, offset + 512);
    if (header.every((byte) => byte === 0)) break;
    const text = (start, length) =>
      header
        .subarray(start, start + length)
        .toString()
        .replace(/\0.*$/, '');
    const size = parseInt(text(124, 12).trim(), 8) || 0;
    if (!Number.isSafeInteger(size) || size < 0 || offset + 512 + size > tar.length)
      throw new Error('Invalid package tar member.');
    const prefix = text(345, 155);
    result.push({
      header,
      name: (prefix ? `${prefix}/` : '') + text(0, 100),
      type: text(156, 1),
      data: tar.subarray(offset + 512, offset + 512 + size),
    });
    offset += 512 + Math.ceil(size / 512) * 512;
  }
  return result;
}

export function normalizeAndVerifyPackage(bytes, manifest) {
  const archive = members(bytes);
  const member = archive.find((entry) => entry.name === 'data.tar.gz');
  if (!member || !archive.some((entry) => entry.name === 'control.tar.gz'))
    throw new Error('Incomplete IPK.');
  const tar = gunzipSync(member.data);
  const files = entries(tar);
  const owned = files.filter((file) => file.name.split('/').includes(manifest.id));
  if (!owned.length) throw new Error('The IPK contains no Luma app directory.');
  if (
    owned.some((file) => /\/assets\/(app-icons(?:\/|$)|icons\.json$|colors\.json$)/.test(file.name))
  )
    throw new Error('Retired dumped artwork must not be shipped in the TV package.');
  const appinfo = owned.find((file) => file.name.endsWith('/appinfo.json'));
  const actual = appinfo && JSON.parse(appinfo.data.toString());
  if (
    actual?.id !== manifest.id ||
    actual?.version !== manifest.version ||
    actual.type !== 'native' ||
    actual.main !== manifest.main
  )
    throw new Error('Packaged app metadata does not match the source.');
  if (JSON.stringify(actual.requiredACG) !== JSON.stringify(manifest.requiredACG))
    throw new Error('Packaged app permissions do not match the source.');
  if (actual.icon !== manifest.icon || actual.largeIcon !== manifest.largeIcon)
    throw new Error('Packaged icon paths do not match the source.');
  const vectorManifest = owned.find((file) => file.name.endsWith('/assets/vector-artwork.json'));
  const vectorAssets = vectorManifest && JSON.parse(vectorManifest.data.toString());
  if (
    vectorAssets?.version !== 1 ||
    !Array.isArray(vectorAssets.assets) ||
    !vectorAssets.assets.length
  )
    throw new Error('The IPK is missing its vector artwork manifest.');
  for (const required of [
    manifest.main,
    manifest.icon,
    manifest.largeIcon,
    'autostart.sh',
    'home.sh',
    'home.cjs',
    'splash.png',
    'lib/libSDL2-2.0.so.0',
    'licenses/sdl2.txt',
    'licenses/lucide.txt',
    'assets/font-metrics.json',
    'assets/font-license.txt',
    'assets/font-cjk-license.txt',
    'assets/version.txt',
    ...[350, 400, 450, 500, 550, 600, 650, 800].flatMap((weight) => [
      `assets/font-${weight}.ttf`,
      `assets/font-display-${weight}.ttf`,
    ]),
    'assets/font-cjk-400.ttf',
    'assets/font-cjk-500.ttf',
    'assets/font-kr-400.ttf',
    'assets/font-kr-500.ttf',
    ...[
      'dunes',
      'midnight',
      'san-francisco',
      'summit',
      'slate',
      'forest',
      'prism',
      'yosemite',
      'northern-lights',
    ].flatMap((name) => [
      `assets/wallpapers/${name}.jpg`,
      `assets/thumbnails/${name}.jpg`,
      `splash/${name}.png`,
    ]),
    ...vectorAssets.assets.map((name) => {
      if (typeof name !== 'string' || !/^(logos|ui-icons)\/[a-z0-9-]+\.lvec$/.test(name))
        throw new Error('Invalid packaged vector asset path.');
      return `assets/${name}`;
    }),
  ]) {
    if (!owned.some((file) => file.name.endsWith(`/${required}`) && file.data.length > 0))
      throw new Error(`Package is missing ${required}.`);
  }
  const executable = owned.find((file) => file.name.endsWith(`/${manifest.main}`))?.data;
  if (
    !executable ||
    executable.length < 52 ||
    executable.subarray(0, 4).toString('hex') !== '7f454c46' ||
    executable[4] !== 1 ||
    executable[5] !== 1 ||
    executable.readUInt16LE(18) !== 40 ||
    (executable.readUInt32LE(36) & 0x400) !== 0
  )
    throw new Error('Packaged executable must be an ARM soft-float ELF.');
  const controlArchive = archive.find((entry) => entry.name === 'control.tar.gz');
  const control = entries(gunzipSync(controlArchive.data))
    .find((file) => file.name.replace(/^\.\//, '') === 'control')
    ?.data.toString();
  if (!control || !/^Architecture: arm\s*$/m.test(control))
    throw new Error('The IPK must declare Architecture: arm.');
  const version = owned
    .find((file) => file.name.endsWith('/assets/version.txt'))
    ?.data.toString()
    .trim();
  if (version !== manifest.version)
    throw new Error('Packaged assets were generated for another version.');
  const vectorFiles = owned.filter((file) => file.name.endsWith('.lvec'));
  for (const file of vectorFiles) {
    if (
      file.data.length < 20 ||
      file.data.subarray(0, 8).toString() !== 'LUMAVEC1' ||
      file.data.readUInt32LE(16) === 0 ||
      file.data.readUInt32LE(16) % 3 !== 0 ||
      file.data.length !== 20 + file.data.readUInt32LE(16) * 36
    )
      throw new Error(`Invalid packaged vector geometry: ${file.name}`);
  }
  for (const file of owned) {
    if (!['', '0', '5'].includes(file.type))
      throw new Error(`Unexpected package entry type: ${file.name}`);
    const executable =
      file.type === '5' ||
      file.name.endsWith('/autostart.sh') ||
      file.name.endsWith('/home.sh') ||
      file.name.endsWith(`/${manifest.main}`);
    file.header.write(executable ? '0000755\0' : '0000644\0', 100, 8, 'ascii');
    file.header.fill(32, 148, 156);
    const sum = file.header.reduce((total, byte) => total + byte, 0);
    file.header.write(`${sum.toString(8).padStart(6, '0')}\0 `, 148, 8, 'ascii');
  }
  member.data = gzipSync(tar, { level: 9 });
  const parts = [Buffer.from('!<arch>\n')];
  for (const entry of archive) {
    entry.header.write(String(entry.data.length).padEnd(10, ' '), 48, 10, 'ascii');
    parts.push(entry.header, entry.data);
    if (entry.data.length % 2) parts.push(Buffer.from('\n'));
  }
  console.log(
    `Verified IPK contents: ${owned.length} Luma entries; app metadata, assets and modes checked.`,
  );
  return Buffer.concat(parts);
}
