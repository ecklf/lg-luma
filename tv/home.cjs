// Root-only TV integration. Keep ES5 syntax for the TV's bundled Node runtime.
// The native executable remains in its installed package so its LS2 role,
// libraries, assets and preference identity continue to use that installation.
var fs = require('fs');
var path = require('path');
var execFileSync = require('child_process').execFileSync;
var spawnSync = require('child_process').spawnSync;
var APP = 'com.ecklf.luma';
var HOME = 'com.webos.app.home';
var STATE = '/var/lib/luma-home';
var STAGE = STATE + '/app';
var TARGET = '/usr/palm/applications/' + HOME;
var HOOK = '/var/lib/webosbrew/init.d/64-luma-home';
var CONFIG = STATE + '/state.json';
// webOS 5/6 (method "executable"): SAM cannot reread a mounted manifest there, but LG Home is a
// native lifecycle-v2 program whose LS2 role is bound to this executable path. Mounting Luma's
// executable over it keeps LG's manifest and role; Luma then registers with SAM itself.
var EXE = '/usr/bin/' + HOME;
// Tells a normal tile launch to open the Home instance. The tile's jail sees /usr/bin without
// this mount but shares /var/run, which is volatile like the mount itself.
var MARKER = '/var/run/luma-home';
// Early cold-boot start for the executable method (src/bin/luma-early.rs, home-filter.js). ls-hubd
// reads these volatile LS2 directories at core-boot-done and starts dynamic services as root, so
// the compositor key filter can have Luma mounted ~30 s before the Homebrew hook runs.
var EARLY = 'com.webos.service.lumahome';
// A copy beside home.sh: ls-hubd treats programs under /media/developer as developer-mode
// services and does not deliver the system key filter's calls to them.
var EARLY_EXE = STATE + '/luma-early';
var LS2 = '/var/luna-service2';
var FILTER = STATE + '/home-filter.js';
var KEY_FILTERS = 'com.webos.surfacemanager.keyFilters';
var FILTER_HANDLER = 'lumaHomeKey';

function read(file) {
  return fs.readFileSync(file, 'utf8');
}

function json(file) {
  return JSON.parse(read(file));
}

function exists(file) {
  try {
    fs.lstatSync(file);
    return true;
  } catch (error) {
    if (error.code === 'ENOENT') return false;
    throw error;
  }
}

function writeJson(file, value) {
  fs.writeFileSync(file + '.new', JSON.stringify(value, null, 2) + '\n', { mode: 420 });
  fs.renameSync(file + '.new', file);
}

function run(file, args) {
  return String(execFileSync(file, args, { timeout: 8000, encoding: 'utf8' }));
}

// lastinput pinning lives in home.sh, which also runs it early in a cold boot without Node.
// Best effort: without it LG still opens Live TV first at power-on.
function lastInput(command) {
  if (command === 'unpin') return restoreInputPolicy();
  try {
    execFileSync('/bin/sh', [STATE + '/home.sh', command], { timeout: 20000, stdio: 'ignore' });
  } catch (error) {
    console.error('Luma Home: lastinput ' + command + ' failed: ' + error.message);
  }
}

// This policy is separately opt-in: ordinary Home takeover leaves TV inputs unchanged.
var INPUT_FILE = '/var/lib/eim/lastinput';
var INPUT_SAVED = STATE + '/input-policy.json';
var INPUT_FLAG = STATE + '/startup-home';
function validInput(id) {
  return (
    typeof id === 'string' &&
    /^com\.webos\.app\.(livetv|hdmi[0-9]+|externalinput\.[a-zA-Z0-9_.-]+)$/.test(id)
  );
}
function inputSettings(method, payload) {
  var reply = JSON.parse(
    run('/usr/bin/luna-send', [
      '-n',
      '1',
      '-w',
      '4000',
      'luna://com.webos.settingsservice/' + method,
      JSON.stringify(payload)
    ])
  );
  if (!reply.returnValue) throw new Error('Input settings: ' + JSON.stringify(reply));
  return reply;
}
function inputMounted() {
  return read('/proc/mounts')
    .split('\n')
    .some(function (line) {
      return line.split(' ')[1] === INPUT_FILE;
    });
}
function restoreInputPolicy() {
  if (!fs.existsSync(INPUT_SAVED) && !fs.existsSync(STATE + '/lastinput')) return;
  // Retain recovery data if any step fails, so disable can be retried.
  if (inputMounted()) run('/bin/umount', [INPUT_FILE]);
  var current = json(INPUT_FILE);
  var saved = fs.existsSync(INPUT_SAVED) ? json(INPUT_SAVED) : null;
  // Migration from the original draft: its unmodified appId is the only available prior input.
  var fallback = validInput(current.appId) ? current.appId : null;
  if (current.physicalLastSourceAppId === HOME) {
    var previous = saved && saved.lastinput;
    var input =
      previous && validInput(previous.physicalLastSourceAppId)
        ? previous.physicalLastSourceAppId
        : fallback;
    if (!input) throw new Error('Cannot identify the previous TV input; recovery data retained');
    current.physicalLastSourceAppId = input;
    // Write in place: EIM owns this inode and may keep it open.
    fs.writeFileSync(INPUT_FILE, JSON.stringify(current));
  }
  var settings = inputSettings('getSystemSettings', {
    category: 'general',
    keys: ['lastInputApp']
  });
  if (settings.settings.lastInputApp === HOME) {
    var last = saved && validInput(saved.lastInputApp) ? saved.lastInputApp : fallback;
    if (!last) throw new Error('Cannot identify the previous Back/Exit input');
    inputSettings('setSystemSettings', { category: 'general', settings: { lastInputApp: last } });
  }
  [INPUT_FLAG, INPUT_SAVED, STATE + '/lastinput'].forEach(function (file) {
    if (fs.existsSync(file)) fs.unlinkSync(file);
  });
}
function writeInputFilter() {
  // configd may suppress unchanged filter lists. Remove our entry before replacing/readding it,
  // otherwise the old live QML context can immediately overwrite restored settings.
  setKeyFilters(false);
  pause();
  var file = STATE + '/home-filter.js';
  var source = read(file).replace(
    /var lumaHomeInputOverride = (true|false);/,
    'var lumaHomeInputOverride = ' + fs.existsSync(INPUT_FLAG) + ';'
  );
  fs.writeFileSync(file + '.new', source, { mode: 420 });
  fs.renameSync(file + '.new', file);
  setKeyFilters(true);
}
function configureInputPolicy(config, enabled) {
  if (!config.enabled || config.method !== 'executable')
    throw new Error('Requires executable Home takeover');
  if (enabled) {
    if (!fs.existsSync(INPUT_SAVED)) {
      var original = json(INPUT_FILE);
      var setting = inputSettings('getSystemSettings', {
        category: 'general',
        keys: ['lastInputApp']
      });
      if (
        !validInput(original.physicalLastSourceAppId) ||
        !validInput(setting.settings.lastInputApp)
      )
        throw new Error('Select a TV input before enabling startup-home');
      writeJson(INPUT_SAVED, { lastinput: original, lastInputApp: setting.settings.lastInputApp });
    }
    var pinned = json(INPUT_SAVED).lastinput;
    pinned.physicalLastSourceAppId = HOME;
    writeJson(STATE + '/lastinput', pinned);
    fs.writeFileSync(INPUT_FLAG, 'enabled\n', { mode: 420 });
    // Unlike the boot fallback, an explicit opt-in must report a failed mount.
    execFileSync('/bin/sh', [STATE + '/home.sh', 'pin'], { timeout: 20000, stdio: 'pipe' });
  } else {
    // Stop the setting writer before restoring its saved value.
    if (fs.existsSync(INPUT_FLAG)) fs.unlinkSync(INPUT_FLAG);
    try {
      writeInputFilter();
    } finally {
      restoreInputPolicy();
    }
  }
  writeInputFilter();
}

function pause() {
  run('/bin/sleep', ['1']);
}

function excerpt(output) {
  return (
    String(output || '')
      .replace(/\s+/g, ' ')
      .trim()
      .slice(0, 480) || '<empty>'
  );
}

function luna(method, payload) {
  return lunaUri('com.webos.applicationManager/' + method, payload);
}

function lunaUri(uri, payload) {
  // -n prints one JSON reply to stdout. -t is timing mode and logs the
  // payload to stderr instead, leaving timing summaries on stdout.
  var result = spawnSync(
    '/usr/bin/luna-send',
    ['-n', '1', '-w', '4000', 'luna://' + uri, JSON.stringify(payload)],
    {
      encoding: 'utf8',
      timeout: 8000,
      maxBuffer: 2 * 1024 * 1024,
      stdio: ['ignore', 'pipe', 'pipe']
    }
  );
  var stdout = String(result.stdout || '').trim();
  var diagnostics = '; stdout: ' + excerpt(stdout) + '; stderr: ' + excerpt(result.stderr);
  if (result.error || result.status !== 0) {
    throw new Error(
      'Luna ' +
        uri +
        ' failed (' +
        (result.error
          ? result.error.message
          : 'exit ' + result.status + ', signal ' + result.signal) +
        ')' +
        diagnostics
    );
  }
  if (!stdout) throw new Error('No Luna JSON reply from ' + uri + diagnostics);
  var reply;
  try {
    reply = JSON.parse(stdout);
  } catch (error) {
    throw new Error('Invalid Luna JSON reply from ' + uri + ': ' + error.message + diagnostics);
  }
  if (
    !reply ||
    typeof reply !== 'object' ||
    Array.isArray(reply) ||
    typeof reply.returnValue !== 'boolean'
  ) {
    throw new Error('Invalid Luna response from ' + uri + diagnostics);
  }
  return reply;
}

function accepted(reply, action) {
  if (!reply || reply.returnValue !== true) {
    throw new Error(action + ': ' + JSON.stringify(reply));
  }
  return reply;
}

function mkdir(directory) {
  if (!fs.existsSync(directory)) fs.mkdirSync(directory, 493);
}

function mounted() {
  if (!fs.existsSync(STAGE)) return false;
  var a = fs.statSync(STAGE);
  var b = fs.statSync(TARGET);
  return a.dev === b.dev && a.ino === b.ino;
}

function foreignMount() {
  return (
    read('/proc/self/mountinfo')
      .split('\n')
      .some(function (line) {
        var target = line.split(' ')[4] || '';
        return target === TARGET || target.indexOf(TARGET + '/') === 0;
      }) && !mounted()
  );
}

// The mount source of an active bind mount over LG's Home executable, or null.
function exeMount() {
  var root = null;
  read('/proc/self/mountinfo')
    .split('\n')
    .forEach(function (line) {
      var fields = line.split(' ');
      if (fields[4] === EXE) root = fields[3];
    });
  return root;
}

// Luma's executable, also after a package update replaced the mounted file.
function exeOwned(root) {
  return /\/com\.ecklf\.luma\/luma(\/\/deleted)?$/.test(root || '');
}

function exeMounted(appDir) {
  try {
    var a = fs.statSync(appDir + '/luma');
    var b = fs.statSync(EXE);
    return a.dev === b.dev && a.ino === b.ino;
  } catch (error) {
    if (error.code === 'ENOENT') return false;
    throw error;
  }
}

function unmountExe() {
  try {
    run('/bin/umount', [EXE]);
  } catch (error) {
    // The running Home executes the mounted file.
    console.error('Luma Home: ' + error.message + ', detaching the mount');
    run('/bin/umount', ['-l', EXE]);
  }
  if (exeMount()) throw new Error('The Luma Home mount is still active');
}

function homeMethod(release, stock) {
  if (/release\s+(10|11)\./.test(release)) return 'manifest';
  if (
    /release\s+[56]\./.test(release) &&
    stock &&
    stock.type === 'native' &&
    stock.main === 'file://' + EXE &&
    stock.nativeLifeCycleInterfaceVersion === 2
  )
    return 'executable';
  throw new Error('This Home integration currently targets webOS 5/6 and 25/26 (10.x/11.x)');
}

function releaseHome() {
  var result;
  for (var i = 0; i < 3; i++) {
    try {
      result = luna('releaseBlockSystemApp', { id: HOME });
      if (result.returnValue === true || /not.*blocked/i.test(result.errorText || '')) return;
    } catch (error) {
      result = { errorText: error.message };
    }
    pause();
  }
  throw new Error('Could not release Home: ' + JSON.stringify(result));
}

function reread(expected) {
  var payload = { id: HOME };
  var schema = '/etc/palm/schemas/sam/applicationManager.blockSystemApp.schema';
  if (fs.existsSync(schema) && read(schema).indexOf('"forceBlockSystemApp"') !== -1) {
    payload.forceBlockSystemApp = false;
  }
  // Always release, even if the block request times out after taking effect.
  try {
    var reply = luna('blockSystemApp', payload);
    if (!/already\s*blocked/i.test(reply.errorText || '')) {
      accepted(reply, 'Block Home for manifest refresh');
    }
  } finally {
    releaseHome();
  }
  for (var i = 0; i < 15; i++) {
    var info = accepted(luna('getAppInfo', { id: HOME }), 'Read Home manifest').appInfo;
    if (
      info &&
      info.type === expected.type &&
      (typeof expected.main !== 'string' ||
        path.basename(info.main || '') === path.basename(expected.main)) &&
      (expected.main !== 'luma' ||
        (Array.isArray(info.appCategory) && info.appCategory.indexOf('home') !== -1))
    )
      return;
    pause();
  }
  throw new Error('SAM did not read the expected Home manifest');
}

function foreground() {
  return accepted(luna('getForegroundAppInfo', {}), 'Read foreground app').appId;
}

function normalRunning() {
  return (accepted(luna('running', {}), 'Read running apps').running || []).some(function (entry) {
    return entry.id === APP;
  });
}

function closeNormal() {
  if (!normalRunning()) return;
  try {
    // On TV, close's id identifies a process. Use the app-ID endpoint for APP.
    accepted(luna('closeByAppId', { id: APP }), 'Close the normal Luma instance');
  } catch (error) {
    // The app may have exited between the running query and the close request.
    if (normalRunning()) throw error;
    return;
  }
  for (var attempt = 0; attempt <= 5; attempt++) {
    if (!normalRunning()) return;
    if (attempt < 5) pause();
  }
  throw new Error('Close Luma before enabling Home integration');
}

function stopHome(appDir) {
  var stopped = [];
  fs.readdirSync('/proc').forEach(function (pid) {
    if (!/^\d+$/.test(pid)) return;
    try {
      var args = read('/proc/' + pid + '/cmdline').split('\0');
      var oldNative = args[0] === '/usr/bin/' + HOME;
      var flutter =
        path.basename(args[0]) === 'flutter-client' &&
        args.indexOf('-i') !== -1 &&
        args[args.indexOf('-i') + 1] === HOME;
      var luma =
        fs.readlinkSync('/proc/' + pid + '/exe').replace(/ \(deleted\)$/, '') === appDir + '/luma';
      var metadata = null;
      try {
        metadata = JSON.parse(args[1]);
      } catch (error) {
        if (!(error instanceof SyntaxError)) throw error;
      }
      if (oldNative || flutter || (luma && metadata && metadata.nid === HOME)) {
        process.kill(Number(pid), 'SIGTERM');
        stopped.push(pid);
      }
    } catch (error) {
      // A process may exit while /proc is being read. Kernel threads have no exe.
      if (error.code !== 'ENOENT' && error.code !== 'ESRCH') throw error;
    }
  });
  for (var i = 0; i < 5; i++) {
    if (
      !stopped.some(function (pid) {
        return fs.existsSync('/proc/' + pid);
      })
    )
      return;
    pause();
  }
  throw new Error('The previous Home process has not stopped');
}

function launchHome() {
  accepted(luna('launch', { id: HOME }), 'Open Home');
}

// SAM may answer a cold native launch late, and right after the previous Home process stopped it
// can accept a launch without starting anything: Luma in front is the success condition.
function openExecutableHome() {
  for (var i = 0; i < 15; i++) {
    if (i === 0 || i === 6) {
      try {
        launchHome();
      } catch (error) {
        console.error('Luma Home: ' + error.message);
      }
    }
    if (foreground() === HOME) return;
    pause();
  }
  throw new Error('Home did not come to the front');
}

function hookOwned() {
  try {
    return fs.readlinkSync(HOOK) === STATE + '/home.sh';
  } catch (error) {
    if (error.code === 'ENOENT' || error.code === 'EINVAL') return false;
    throw error;
  }
}

function restoreExecutable(config) {
  var wasHome = false;
  try {
    wasHome = foreground() === HOME;
  } catch (error) {
    console.error('Luma Home: ' + error.message + ', restoring the files first');
  }
  var root = exeMount();
  if (root && !exeOwned(root)) throw new Error('Another integration is mounted over LG Home');
  if (fs.existsSync(MARKER)) fs.unlinkSync(MARKER);
  var inputError = null;
  try {
    lastInput('unpin');
  } catch (error) {
    inputError = error;
  }
  if (root) unmountExe();
  stopHome(config.appDir);
  // LG Home on webOS 5/6 is an overlay above the current app, not a foreground app.
  if (wasHome) launchHome();
  if (inputError) throw inputError;
}

function applyExecutable(config, open) {
  var root = exeMount();
  if (root && !exeOwned(root)) throw new Error('Another integration is mounted over LG Home');
  if (!fs.existsSync(config.appDir + '/luma')) throw new Error('Luma is no longer installed');
  var front = foreground();
  try {
    // After a package update the mount still holds the replaced file.
    if (root && !exeMounted(config.appDir)) unmountExe();
    if (!exeMounted(config.appDir)) run('/bin/mount', ['--bind', config.appDir + '/luma', EXE]);
    if (!exeMounted(config.appDir)) throw new Error('The Home bind mount did not take');
    fs.writeFileSync(MARKER, APP + '\n', { mode: 420 });
    stopHome(config.appDir);
    lastInput('pin');
    // With lastinput pinned, a cold boot whose early start failed has nothing in front.
    if (open || !front || (front === HOME && foreground() === HOME)) openExecutableHome();
  } catch (error) {
    try {
      restoreExecutable(config);
    } catch (recovery) {
      throw new Error(error.message + '\nRestore also failed: ' + recovery.message);
    }
    throw error;
  }
}

function restore(config) {
  if (config.method === 'executable') return restoreExecutable(config);
  var wasHome = false;
  try {
    wasHome = foreground() === HOME;
  } catch (error) {
    console.error('Luma Home: ' + error.message + ', restoring the files first');
  }
  if (foreignMount()) throw new Error('Another integration is mounted over LG Home');
  if (mounted()) {
    try {
      run('/bin/umount', [TARGET]);
    } catch (error) {
      // A running Home may still hold its working directory in the mount.
      console.error('Luma Home: ' + error.message + ', detaching the mount');
      run('/bin/umount', ['-l', TARGET]);
    }
    if (mounted()) throw new Error('The Luma Home mount is still active');
  }
  // Also finish a restore interrupted between unmounting and the rescan.
  // Read the uncovered files, since a firmware update may have changed LG Home.
  reread(json(TARGET + '/appinfo.json'));
  stopHome(config.appDir);
  if (wasHome) launchHome();
}

// The LG splash is a static file. Stage the splash of the owner's current background, so a cold
// boot into Luma Home opens on their wallpaper. Falls back to the package default on any error.
function stageSplash(appDir) {
  var background = 'dunes';
  var candidates = [
    '/media/developer/' + APP + '/preferences.json',
    '/media/developer/temp/' + APP + '/preferences.json',
    '/media/internal/.' + APP + '/preferences.json'
  ];
  // Match storage.rs: production native jails prefer internal storage. Reads
  // skip absent files, preserving an existing destination before migration.
  if (appDir.indexOf('/media/cryptofs/') === 0) candidates.unshift(candidates.pop());
  candidates.push(appDir + '/preferences.json');
  for (var i = 0; i < candidates.length; i++) {
    try {
      var saved = json(candidates[i]).background;
      if (typeof saved === 'string' && /^[a-z0-9-]{1,32}$/.test(saved)) {
        background = saved;
        break;
      }
    } catch (error) {
      if (error.code !== 'ENOENT') console.log('home: preferences unreadable: ' + error.message);
    }
  }
  var source = appDir + '/splash/' + background + '.png';
  if (!fs.existsSync(source)) source = appDir + '/splash.png';
  try {
    fs.writeFileSync(STAGE + '/splash.png.new', fs.readFileSync(source), { mode: 420 });
    fs.renameSync(STAGE + '/splash.png.new', STAGE + '/splash.png');
  } catch (error) {
    console.log('home: splash not staged: ' + error.message);
  }
}

function apply(config, open) {
  if (config.method === 'executable') return applyExecutable(config, open);
  if (foreignMount()) throw new Error('Another integration is mounted over LG Home');
  if (!fs.existsSync(config.appDir + '/luma')) throw new Error('Luma is no longer installed');
  stageSplash(config.appDir);
  var front = foreground();
  try {
    if (!mounted()) run('/bin/mount', ['--bind', STAGE, TARGET]);
    if (!mounted()) throw new Error('The Home bind mount did not take');
    reread(json(STAGE + '/appinfo.json'));
    stopHome(config.appDir);
    // Only replace the screen if Home is still in front. During boot the
    // owner may have already selected an input or another app.
    if (open || (front === HOME && foreground() === HOME)) launchHome();
  } catch (error) {
    try {
      restore(config);
    } catch (recovery) {
      throw new Error(error.message + '\nRestore also failed: ' + recovery.message);
    }
    throw error;
  }
}

function installScripts() {
  ['home.sh', 'home.cjs', 'home-filter.js'].forEach(function (name) {
    var target = STATE + '/' + name;
    var data = fs.readFileSync(path.join(__dirname, name));
    if (name === 'home-filter.js')
      data = String(data).replace(
        'var lumaHomeInputOverride = false;',
        'var lumaHomeInputOverride = ' + fs.existsSync(INPUT_FLAG) + ';'
      );
    fs.writeFileSync(target + '.new', data, { mode: name === 'home.sh' ? 493 : 420 });
    fs.renameSync(target + '.new', target);
  });
}

// The LS2 definitions of luma-early: dynamic service, role bound to its executable, the one
// method in API group "all" (the only group LG grants com.webos.starfish.keyfilters), callable
// by the key filter alone.
function earlyFiles() {
  var exe = EARLY_EXE;
  var files = {};
  files[LS2 + '/services.d/' + EARLY + '.service'] =
    '[D-BUS Service]\nName=' + EARLY + '\nExec=' + exe + '\nType=dynamic\n';
  files[LS2 + '/roles.d/' + EARLY + '.json'] =
    JSON.stringify({
      exeName: exe,
      type: 'regular',
      allowedNames: [EARLY],
      permissions: [{ service: EARLY, inbound: ['com.webos.starfish.keyfilters'], outbound: [] }]
    }) + '\n';
  files[LS2 + '/api-permissions.d/' + EARLY + '.json'] =
    JSON.stringify({ all: [EARLY + '/prepare'] }) + '\n';
  files[LS2 + '/manifests.d/' + EARLY + '.json'] =
    JSON.stringify({
      id: EARLY,
      // ls-hubd silently drops a manifest whose version is not x.y.z.
      version: '1.0.0',
      serviceFiles: [LS2 + '/services.d/' + EARLY + '.service'],
      roleFiles: [LS2 + '/roles.d/' + EARLY + '.json'],
      apiPermissionFiles: [LS2 + '/api-permissions.d/' + EARLY + '.json']
    }) + '\n';
  return files;
}

// LG's filter chain with ours removed, and appended again when `on`. Keys pass through it.
function keyFilters(list, on) {
  var rest = list.filter(function (entry) {
    return !entry || entry.file !== FILTER;
  });
  if (on) rest.push({ file: FILTER, handler: FILTER_HANDLER });
  return rest;
}

function setKeyFilters(on) {
  var reply = accepted(
    lunaUri('com.webos.service.config/getConfigs', { configNames: [KEY_FILTERS] }),
    'Read key filters'
  );
  var list = reply.configs && reply.configs[KEY_FILTERS];
  if (!Array.isArray(list)) throw new Error('Missing ' + KEY_FILTERS);
  var next = keyFilters(list, on);
  if (JSON.stringify(next) === JSON.stringify(list)) return;
  var configs = {};
  configs[KEY_FILTERS] = next;
  accepted(
    lunaUri('com.webos.service.config/setConfigs', { configs: configs }),
    'Write key filters'
  );
}

function rescanLs2() {
  run('/usr/sbin/ls-control', ['scan-volatile-dirs']);
}

// Best effort: without it a cold boot still reaches Luma through the Homebrew hook.
function installEarly(appDir) {
  if (!fs.existsSync(appDir + '/luma-early')) return;
  try {
    fs.writeFileSync(EARLY_EXE + '.new', fs.readFileSync(appDir + '/luma-early'), { mode: 493 });
    fs.renameSync(EARLY_EXE + '.new', EARLY_EXE);
    var files = earlyFiles();
    Object.keys(files).forEach(function (file) {
      mkdir(path.dirname(file));
      fs.writeFileSync(file + '.new', files[file], { mode: 420 });
      fs.renameSync(file + '.new', file);
    });
    rescanLs2();
    setKeyFilters(false);
    setKeyFilters(true);
  } catch (error) {
    console.error('Luma Home: early start not installed: ' + error.message);
  }
}

// Best effort as well: a failed step is reported and must not keep disable from restoring LG Home.
function removeEarly() {
  function attempt(step) {
    try {
      step();
    } catch (error) {
      console.error('Luma Home: early start cleanup: ' + error.message);
    }
  }
  attempt(function () {
    setKeyFilters(false);
  });
  var changed = false;
  Object.keys(earlyFiles()).forEach(function (file) {
    attempt(function () {
      if (exists(file)) {
        fs.unlinkSync(file);
        changed = true;
      }
    });
  });
  attempt(function () {
    if (exists(EARLY_EXE)) fs.unlinkSync(EARLY_EXE);
  });
  if (changed) attempt(rescanLs2);
}

function disable(config) {
  config.enabled = false;
  writeJson(CONFIG, config);
  if (config.method === 'executable') removeEarly();
  try {
    restore(config);
  } finally {
    if (hookOwned()) fs.unlinkSync(HOOK);
  }
}

function installHook() {
  mkdir('/var/lib/webosbrew');
  mkdir('/var/lib/webosbrew/init.d');
  if (!hookOwned()) fs.symlinkSync(STATE + '/home.sh', HOOK);
}

function install(appDir) {
  appDir = fs.realpathSync(appDir);
  var app = json(appDir + '/appinfo.json');
  if (app.id !== APP || app.type !== 'native' || app.main !== 'luma') {
    throw new Error('Enable this from an installed native Luma package');
  }
  var release = read('/etc/starfish-release');
  // The executable method never mounts over LG's manifest, so it reads the original directly.
  var method = homeMethod(release, mounted() ? null : json(TARGET + '/appinfo.json'));
  if (foreignMount()) throw new Error('Another integration is mounted over LG Home');
  var exeRoot = exeMount();
  if (exeRoot && !exeOwned(exeRoot)) throw new Error('Another integration is mounted over LG Home');
  if (exists(HOOK) && !hookOwned()) throw new Error('64-luma-home is already in use');
  if (exists('/var/lib/webosbrew/init.d/65-luma')) {
    throw new Error('Disable the standalone cold-boot hook with autostart.sh disable first');
  }
  var previous = fs.existsSync(CONFIG) ? json(CONFIG) : null;
  var stock = mounted() ? previous && previous.stock : json(TARGET + '/appinfo.json');
  if (!stock || stock.id !== HOME || stock.main === 'luma') {
    throw new Error('Could not identify the original LG Home manifest');
  }
  // Finish any normal tile instance before the Home identity starts using the
  // same settings. A later tile launch redirects in the native executable.
  closeNormal();
  if (method === 'executable') {
    installScripts();
    var exeConfig = { enabled: false, method: method, appDir: appDir, stock: stock };
    writeJson(CONFIG, exeConfig);
    installHook();
    apply(exeConfig, true);
    exeConfig.enabled = true;
    writeJson(CONFIG, exeConfig);
    installEarly(appDir);
    console.log('Luma is now Home. Home Auto Launch and Quick Start+ follow the TV settings.');
    return;
  }
  mkdir(STAGE);
  // Symlinks keep the executable at its installed path for the dynamic loader
  // and LS2. They also follow ordinary in-place package updates.
  ['luma', 'assets', 'lib', 'splash', app.icon, app.largeIcon].forEach(function (name) {
    var link = STAGE + '/' + name;
    if (exists(link + '.new')) fs.unlinkSync(link + '.new');
    fs.symlinkSync(appDir + '/' + name, link + '.new');
    fs.renameSync(link + '.new', link);
  });
  app.id = HOME;
  app.title = 'Home';
  app.visible = false;
  app.appCategory = ['home'];
  writeJson(STAGE + '/appinfo.json', app);
  fs.writeFileSync(STAGE + '/.luma-home', APP + '\n', { mode: 420 });
  installScripts();
  var config = { enabled: false, method: method, appDir: appDir, stock: stock };
  writeJson(CONFIG, config);
  installHook();
  apply(config, true);
  config.enabled = true;
  writeJson(CONFIG, config);
  console.log('Luma is now Home. Home Auto Launch and Quick Start+ follow the TV settings.');
}

try {
  var command = process.argv[2];
  if (command === 'enable') {
    install(process.argv[3]);
  } else if (command === 'startup-home') {
    if (process.argv[3] !== 'on' && process.argv[3] !== 'off')
      throw new Error('Usage: home.sh startup-home on|off');
    configureInputPolicy(json(CONFIG), process.argv[3] === 'on');
  } else if (command === 'unpin') {
    restoreInputPolicy();
  } else if (command === 'status') {
    var saved = fs.existsSync(CONFIG) ? json(CONFIG) : null;
    console.log(
      JSON.stringify(
        {
          enabled: !!(saved && saved.enabled),
          method: saved ? saved.method || 'manifest' : null,
          mounted: saved && saved.method === 'executable' ? exeMounted(saved.appDir) : mounted(),
          bootHook: hookOwned(),
          earlyStart:
            !!(saved && saved.method === 'executable') &&
            exists(LS2 + '/services.d/' + EARLY + '.service'),
          appDir: saved ? saved.appDir : null
        },
        null,
        2
      )
    );
  } else if (command === 'apply' || command === 'disable' || command === 'restore') {
    if (!fs.existsSync(CONFIG)) throw new Error('Luma Home has not been enabled');
    var config = json(CONFIG);
    if (command === 'apply' && config.enabled) {
      apply(config, false);
    } else {
      disable(config);
      console.log('LG Home restored. Luma Home boot integration disabled.');
    }
  } else {
    throw new Error('Unknown Home integration command');
  }
} catch (error) {
  console.error('Luma Home: ' + error.message);
  process.exitCode = 1;
}
