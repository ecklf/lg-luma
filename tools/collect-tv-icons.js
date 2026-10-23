// Runs under the TV's system node (v8.12.0 on a webOS 6 C1), which predates
// optional catch binding (Node 10), so every catch names its error - and uses
// it, which the lint config requires.
var fs = require('fs'),
  path = require('path'),
  execSync = require('child_process').execSync,
  out = '/tmp/luma-artwork',
  map = Object.create(null),
  seen = Object.create(null),
  visible = Object.create(null);
try {
  fs.mkdirSync(out);
} catch (error) {
  if (error.code !== 'EEXIST') throw error;
}
// Match normalize_apps() visibility rules using the live inventory rather
// than appinfo.json. Explicitly hidden apps are excluded, system apps need
// visible: true, and Live TV remains available regardless of that flag.
try {
  var listed = execSync(
    "/usr/bin/luna-send -n 1 -w 4000 luna://com.webos.applicationManager/listApps '{}'",
    { maxBuffer: 10 * 1024 * 1024 },
  );
  var inventory = JSON.parse(listed);
  if (
    inventory.returnValue !== true ||
    !Array.isArray(inventory.apps) ||
    inventory.apps.length > 1000
  )
    throw new Error('Invalid listApps response');
  inventory.apps.forEach(function (a) {
    if (a && validAppId(a.id) && (!(a.id in visible) || !shouldCollect(a.id, visible[a.id])))
      visible[a.id] = a.visible;
  });
} catch (error) {
  console.error('listApps failed, skipping system apps except Live TV: ' + error.message);
}

function isFile(p) {
  return fs.existsSync(p) && fs.statSync(p).isFile();
}

function validAppId(id) {
  return (
    typeof id === 'string' &&
    id.length > 0 &&
    id.length <= 128 &&
    id.split(/[._-]/).every(function (part) {
      return /^[a-zA-Z0-9]+$/.test(part);
    })
  );
}

function shouldCollect(id, visibility) {
  if (!validAppId(id)) return false;
  var live = id === 'com.webos.app.livetv';
  return !(
    [
      'com.ecklf.luma',
      'com.luma.native',
      'com.luma.launcher',
      'com.luma.launcher.native',
      'com.webos.app.home',
    ].indexOf(id) >= 0 ||
    id.indexOf('com.webos.app.hdmi') === 0 ||
    id.indexOf('com.webos.app.externalinput.') === 0 ||
    (!live && (visibility === false || visibility === 'false')) ||
    (!live &&
      ['com.webos.', 'com.palm.', 'com.lge.'].some(function (p) {
        return id.indexOf(p) === 0;
      }) &&
      visibility !== true)
  );
}

function collect(dir, a) {
  if (!shouldCollect(a.id, visible[a.id]) || seen[a.id]) return;
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
