// Runs under the TV's system node (v8.12.0 on a webOS 6 C1), which predates
// optional catch binding (Node 10), so every catch names its error - and uses
// it, which the lint config requires.
var fs = require('fs'),
  path = require('path'),
  execSync = require('child_process').execSync,
  out = '/tmp/luma-artwork',
  map = {},
  seen = {},
  visible = {};
try {
  fs.mkdirSync(out);
} catch (error) {
  if (error.code !== 'EEXIST') throw error;
}
// normalize_apps() only excludes com.webos./com.palm./com.lge. apps when the
// live inventory does not report visible: true. appinfo.json alone cannot
// tell the two apart, so ask the same listApps the running app queries.
try {
  var listed = execSync(
    "/usr/bin/luna-send -n 1 -w 4000 luna://com.webos.applicationManager/listApps '{}'",
    { maxBuffer: 10 * 1024 * 1024 },
  );
  (JSON.parse(listed).apps || []).forEach(function (a) {
    if (a.id && a.visible === true) visible[a.id] = true;
  });
} catch (error) {
  console.error(
    'listApps failed, skipping every com.webos./com.palm./com.lge. app: ' + error.message,
  );
}

function isFile(p) {
  return fs.existsSync(p) && fs.statSync(p).isFile();
}

function collect(dir, a) {
  if (!a.id || seen[a.id]) return;
  if (
    ['com.webos.', 'com.palm.', 'com.lge.'].some(function (p) {
      return a.id.indexOf(p) === 0;
    }) &&
    visible[a.id] !== true
  )
    return;
  seen[a.id] = true;
  var src;
  [a.largeIcon, a.icon, 'icon.png'].some(function (i) {
    if (typeof i !== 'string' || !i) return false;
    var p = i[0] === '/' ? i : path.join(dir, i);
    if (!isFile(p)) return false;
    src = p;
    return true;
  });
  if (!src) return;
  var ext = path.extname(src).toLowerCase();
  if (['.png', '.jpg', '.jpeg', '.webp', '.svg'].indexOf(ext) < 0) return;
  var name = Object.keys(map).length + ext;
  fs.writeFileSync(path.join(out, name), fs.readFileSync(src));
  map[a.id] = './app-icons/' + name;
}

[
  '/media/developer/apps/usr/palm/applications',
  '/media/cryptofs/apps/usr/palm/applications',
  '/usr/palm/applications',
].forEach(function (root) {
  var entries;
  try {
    entries = fs.readdirSync(root);
  } catch (error) {
    if (error.code !== 'ENOENT') console.error('Skipped ' + root + ': ' + error.message);
    return;
  }
  entries.forEach(function (d) {
    var dir = path.join(root, d),
      manifest = path.join(dir, 'appinfo.json');
    if (!isFile(manifest)) return;
    try {
      collect(dir, JSON.parse(fs.readFileSync(manifest, 'utf8')));
    } catch (error) {
      console.error('Skipped ' + dir + ': ' + error.message);
    }
  });
});
fs.writeFileSync(path.join(out, 'app-icons.json'), JSON.stringify(map, null, 2));
console.log('Collected ' + Object.keys(map).length + ' app icons');
