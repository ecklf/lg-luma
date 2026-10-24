// Embedded in the native executable and run on its platform worker through
// Homebrew's elevated exec service. Keep ES5 syntax for webOS's Node 8.
var fs = require('fs');
var path = require('path');
var selected = JSON.parse(process.argv[1]);
var APP = 'com.ecklf.luma';
var STATE = '/var/lib/luma-home';
var roots = [
  '/media/developer/apps/usr/palm/applications',
  '/media/cryptofs/apps/usr/palm/applications',
  '/usr/palm/applications'
];

if (
  typeof selected.directory !== 'string' ||
  !roots.some(function (root) {
    return selected.directory === root + '/' + APP;
  })
) {
  throw new Error('Invalid Luma installation directory');
}
if (typeof selected.background !== 'string' || !/^[a-z0-9-]{1,32}$/.test(selected.background)) {
  throw new Error('Invalid splash background');
}
var appDir = selected.directory;
var app = JSON.parse(fs.readFileSync(appDir + '/appinfo.json', 'utf8'));
if (app.id !== APP || app.type !== 'native' || app.main !== 'luma') {
  throw new Error('Splash updates require an installed native Luma package');
}
// Read the selection already loaded by the native app, rather than trying to
// rediscover its preferences from outside the jail. Never copy the default over
// a working splash if an update is missing the selected image.
var image = fs.readFileSync(path.join(appDir, 'splash', selected.background + '.png'));

function replace(file) {
  try {
    if (fs.readFileSync(file).equals(image)) return;
  } catch (error) {
    if (error.code !== 'ENOENT') throw error;
  }
  fs.writeFileSync(file + '.new', image, { mode: 420 });
  fs.renameSync(file + '.new', file);
}

// SAM's normal tile uses the installed manifest's splash.png. Updating only
// the staged Home copy leaves every normal launch on the package default.
replace(appDir + '/splash.png');
var config = null;
try {
  config = JSON.parse(fs.readFileSync(STATE + '/state.json', 'utf8'));
} catch (error) {
  if (error.code !== 'ENOENT') throw error;
}
// Only the manifest method stages a Home copy. The executable method (webOS 5/6)
// keeps LG's own Home manifest, so there is no staged splash to update.
if (config && config.enabled && config.method !== 'executable' && config.appDir === appDir) {
  replace(STATE + '/app/splash.png');
}
console.log(JSON.stringify({ returnValue: true, background: selected.background }));
