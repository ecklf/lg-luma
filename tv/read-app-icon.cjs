// Embedded in the native executable and run only through Homebrew's elevated
// exec service when jailed file reads fail. Compatible with webOS's Node 8.
var fs = require('fs');
var path = require('path');
var source = JSON.parse(process.argv[1]);
var roots = [
  '/media/developer/apps/usr/palm/applications',
  '/media/cryptofs/apps/usr/palm/applications',
  '/usr/palm/applications'
];
var remaining = 512 * 1024;
var icons = [];
var seen = Object.create(null);

function read(file, limit) {
  var fd;
  try {
    fd = fs.openSync(file, 'r');
    var stat = fs.fstatSync(fd);
    if (!stat.isFile() || stat.size > limit) return null;
    var bytes = Buffer.alloc(stat.size);
    var offset = 0;
    while (offset < bytes.length) {
      var count = fs.readSync(fd, bytes, offset, bytes.length - offset, offset);
      if (!count) return null;
      offset += count;
    }
    return bytes;
  } catch (error) {
    if (error.code !== 'ENOENT' && error.code !== 'EACCES') console.error(error.message);
    return null;
  } finally {
    if (fd !== undefined) fs.closeSync(fd);
  }
}

if (
  typeof source.id !== 'string' ||
  source.id.length > 128 ||
  !/^[a-zA-Z0-9]+([._-][a-zA-Z0-9]+)*$/.test(source.id)
) {
  throw new Error('Invalid application ID');
}
var directories = roots.map(function (root) {
  return path.join(root, source.id);
});
if (
  typeof source.directory === 'string' &&
  roots.indexOf(path.dirname(source.directory)) >= 0 &&
  source.directory.split('/').indexOf('..') < 0
) {
  if (directories.indexOf(source.directory) >= 0)
    directories.splice(directories.indexOf(source.directory), 1);
  directories.unshift(source.directory);
}
directories.forEach(function (directory) {
  var manifest = {};
  var bytes = read(path.join(directory, 'appinfo.json'), 64 * 1024);
  if (bytes) {
    try {
      manifest = JSON.parse(bytes.toString('utf8'));
      if (!manifest || manifest.id !== source.id) manifest = {};
    } catch (error) {
      manifest = {};
      console.error('Invalid app manifest: ' + error.message);
    }
  }
  [source.large_icon, manifest.largeIcon, source.icon, manifest.icon, 'icon.png'].forEach(
    function (icon) {
      if (
        typeof icon !== 'string' ||
        !icon ||
        icon.length > 4096 ||
        icon.split('').some(function (character) {
          var code = character.charCodeAt(0);
          return code < 32 || code === 127;
        })
      )
        return;
      icon = icon.replace(/^file:\/\//, '');
      if (icon.indexOf('://') >= 0) return;
      if (icon.split('/').indexOf('..') >= 0) return;
      var file = path.resolve(directory, icon);
      if (['.png', '.jpg', '.jpeg', '.webp'].indexOf(path.extname(file).toLowerCase()) < 0) return;
      if (
        !roots.some(function (root) {
          return file.indexOf(root + '/') === 0;
        }) ||
        seen[file]
      )
        return;
      seen[file] = true;
      var data = read(file, remaining);
      if (!data || !data.length) return;
      remaining -= data.length;
      icons.push(data.toString('hex'));
    }
  );
});
console.log(JSON.stringify({ returnValue: true, icons: icons }));
