/* Luma Home compositor key filter (webOS 5/6 executable method; home.cjs enable registers it in
 * com.webos.surfacemanager.keyFilters). LG's compositor loads it with Qt.include within seconds of
 * a cold boot, long before the Homebrew hooks (~50 s). While the TV boots it asks luma-early
 * (com.webos.service.lumahome, src/bin/luma-early.rs) to mount Luma over LG's Home executable
 * and then opens Home, at about 20 s. While an app is in front it points Back/Exit at Home
 * instead of Live TV. Keys pass through unchanged. ES5 for LG's QML engine.
 */
var lumaHomeBoot = ''; // "" until bootd answers, then "booting" or "done"
var lumaHomeEarly = ''; // "" until luma-early answers, then "ready" or "failed"
var lumaHomeTries = 0;
var lumaHomeFront = '';
var lumaHomeLaunches = 0;
var lumaHomeShown = false; // Home came to the front after a launch from here
var lumaHomeInputOverride = false; // Changed only by explicit startup-home opt-in.
var lumaHomeLastInput = null; // general/lastInputApp, null until the settings service answers
// The selected Home target; home.cjs installScripts writes it into the installed copy.
var lumaHomeTarget = 'com.webos.app.home';

function lumaHomeParse(payload) {
  return typeof payload === 'string' ? JSON.parse(payload) : payload;
}

function lumaHomeIsInput(id) {
  return /^com\.webos\.app\.(livetv|hdmi\d|externalinput\..*)$/.test(id);
}

// Back/Exit from an app: LG's compositor closes it and opens setting lastInputApp, normally the
// input watched last (Live TV). While an app other than an input is in front it names Home, so
// Back returns to Luma; LG's input apps set it to themselves again when chosen.
function lumaHomePinInput() {
  if (
    !lumaHomeInputOverride ||
    lumaHomeLastInput === null ||
    lumaHomeLastInput === lumaHomeTarget ||
    !lumaHomeFront ||
    lumaHomeIsInput(lumaHomeFront)
  )
    return;
  lumaHomeLastInput = lumaHomeTarget;
  lunaCall.send(
    'com.webos.settingsservice',
    '/setSystemSettings',
    JSON.stringify({ category: 'general', settings: { lastInputApp: lumaHomeTarget } }),
    function () {}
  );
}

function lumaHomePrepare() {
  if (lumaHomeEarly || lumaHomeBoot !== 'booting' || lumaHomeTries >= 30) return;
  lumaHomeTries++;
  // ls-hubd routes the call only after its volatile scan at core-boot-done, and an unroutable
  // call gets no reply: ask again on a timer.
  genericTimer.set(1500, lumaHomePrepare);
  lunaCall.send('com.webos.service.lumahome', '/prepare', '{}', function (payload) {
    try {
      var r = lumaHomeParse(payload);
      if (lumaHomeEarly || !r || typeof r.exitCode !== 'number') return;
      lumaHomeEarly = r.returnValue === true ? 'ready' : 'failed';
      lumaHomeOpen();
    } catch (error) {
      console.warn('luma home filter: ' + error);
    }
  });
}

// Home is a system app: SAM takes its launch at once, while normal apps wait for
// minimal-boot-done (~25-35 s). Only over an empty screen or the input bootd opened, so a choice
// the owner made meanwhile stays. Called again on every foreground change until Home is shown:
// the snapshot's LG Home can still be in front when the mount is ready, and Live TV follows it.
function lumaHomeOpen() {
  if (
    lumaHomeEarly !== 'ready' ||
    lumaHomeShown ||
    lumaHomeFront === lumaHomeTarget ||
    lumaHomeLaunches >= 2
  )
    return;
  if (lumaHomeFront && !lumaHomeIsInput(lumaHomeFront)) return;
  lumaHomeLaunches++;
  lunaCall.send(
    'com.webos.applicationManager',
    '/launch',
    JSON.stringify({ id: lumaHomeTarget }),
    function () {}
  );
  // Right after the snapshot's Home stopped, SAM can accept a launch without starting anything.
  genericTimer.set(8000, lumaHomeOpen);
}

// Called by LG's compositor once the file is loaded.
// oxlint-disable-next-line no-unused-vars
function init() {
  try {
    lunaCall.subscribe(
      'com.webos.bootManager',
      '/getBootStatus',
      '{}',
      function (payload) {
        try {
          var r = lumaHomeParse(payload);
          if (!r || !r.returnValue || !r.signals) return;
          // Only a cold boot: reloading the filter while the TV is in use never takes the screen.
          lumaHomeBoot = r.signals['boot-done']
            ? 'done'
            : lumaHomeBoot === ''
              ? 'booting'
              : lumaHomeBoot;
          lumaHomePrepare();
        } catch (error) {
          console.warn('luma home filter: ' + error);
        }
      },
      true
    );
    lunaCall.subscribe(
      'com.webos.applicationManager',
      '/getForegroundAppInfo',
      '{}',
      function (payload) {
        try {
          var r = lumaHomeParse(payload);
          lumaHomeFront = (r && r.appId) || '';
          if (lumaHomeFront === lumaHomeTarget && lumaHomeLaunches) lumaHomeShown = true;
          else lumaHomeOpen();
          lumaHomePinInput();
        } catch (error) {
          console.warn('luma home filter: ' + error);
        }
      },
      true
    );
    lunaCall.subscribe(
      'com.webos.settingsservice',
      '/getSystemSettings',
      JSON.stringify({ category: 'general', keys: ['lastInputApp'], subscribe: true }),
      function (payload) {
        try {
          var r = lumaHomeParse(payload);
          if (r && r.settings && typeof r.settings.lastInputApp === 'string')
            lumaHomeLastInput = r.settings.lastInputApp;
          lumaHomePinInput();
        } catch (error) {
          console.warn('luma home filter: ' + error);
        }
      },
      true
    );
  } catch (error) {
    console.warn('luma home filter: ' + error);
  }
}

// The handler named in the key filter chain (home.cjs); keys pass through.
// oxlint-disable-next-line no-unused-vars
function lumaHomeKey() {
  return KeyPolicy.NextPolicy;
}
