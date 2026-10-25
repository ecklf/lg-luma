/* Luma Home compositor key filter (webOS 5/6 executable method; home.cjs enable registers it in
 * com.webos.surfacemanager.keyFilters). LG's compositor loads it with Qt.include within seconds of
 * a cold boot, long before the Homebrew hooks (~50 s). While the TV boots it asks luma-early
 * (com.webos.service.lumahome, src/bin/luma-early.rs) to mount Luma over LG's Home executable
 * and then opens Home, at about 20 s, showing a loading screen until then. While an app is in
 * front it points Back/Exit at Home instead of Live TV. Keys pass through unchanged. ES5 for LG's
 * QML engine.
 */
var lumaHomeBoot = ''; // "" until bootd answers, then "booting" or "done"
var lumaHomeEarly = ''; // "" until luma-early answers, then "ready" or "failed"
var lumaHomeTries = 0;
var lumaHomeFront = '';
var lumaHomeLaunches = 0;
var lumaHomeShown = false; // Home came to the front after a launch from here
var lumaHomeLastInput = null; // general/lastInputApp, null until the settings service answers

function lumaHomeParse(payload) {
  return typeof payload === 'string' ? JSON.parse(payload) : payload;
}

// Cold-boot loading screen: black, Luma's mark (boot-logo.png beside this file) and after 2 s a
// spinner. The compositor draws it itself (compositorWindow is a context property of LG's
// compositor QML), so it shows from the filter's first seconds instead of a dark screen. Gone once
// an app is in front, on any key, or after 90 s.
var lumaHomeSplash = null;
var lumaHomeSplashDone = false;
var LUMA_HOME_SPLASH_QML =
  'import QtQuick 2.4\n' +
  'Rectangle {\n' +
  '  anchors.fill: parent; z: 100000; color: "black"; opacity: 0\n' +
  '  Behavior on opacity { NumberAnimation { duration: 400 } }\n' +
  '  Component.onCompleted: opacity = 1\n' +
  '  Image {\n' +
  '    id: logo; source: "file:///var/lib/luma-home/boot-logo.png"\n' +
  '    anchors.centerIn: parent; anchors.verticalCenterOffset: -40\n' +
  '  }\n' +
  '  Item {\n' +
  '    id: spinner; width: 40; height: 40; opacity: 0\n' +
  '    anchors.horizontalCenter: parent.horizontalCenter\n' +
  '    anchors.top: logo.bottom; anchors.topMargin: 90\n' +
  '    property int step: 0\n' +
  '    Behavior on opacity { NumberAnimation { duration: 600 } }\n' +
  '    Timer { interval: 2000; running: true; onTriggered: spinner.opacity = 1 }\n' +
  '    Timer { interval: 83; running: true; repeat: true; onTriggered: spinner.step = (spinner.step + 1) % 12 }\n' +
  '    Repeater {\n' +
  '      model: 12\n' +
  '      Rectangle {\n' +
  '        x: 18; width: 4; height: 11; radius: 2; color: "white"; antialiasing: true\n' +
  '        opacity: 0.12 + 0.88 * (1 - ((spinner.step - index + 12) % 12) / 12)\n' +
  '        transform: Rotation { origin.x: 2; origin.y: 20; angle: index * 30 }\n' +
  '      }\n' +
  '    }\n' +
  '  }\n' +
  '}\n';

function lumaHomeSplashShow() {
  if (lumaHomeSplash || lumaHomeSplashDone || typeof compositorWindow === 'undefined') return;
  try {
    lumaHomeSplash = Qt.createQmlObject(
      LUMA_HOME_SPLASH_QML,
      compositorWindow.contentItem,
      'lumaHomeSplash'
    );
    genericTimer.set(90000, lumaHomeSplashHide);
  } catch (error) {
    console.warn('luma home filter: no loading screen: ' + error);
  }
}

function lumaHomeSplashHide() {
  lumaHomeSplashDone = true;
  if (!lumaHomeSplash) return;
  var splash = lumaHomeSplash;
  lumaHomeSplash = null;
  try {
    splash.opacity = 0;
    genericTimer.set(500, function () {
      splash.destroy();
    });
  } catch (error) {
    console.warn('luma home filter: ' + error);
  }
}

function lumaHomeSplashCheck() {
  if (lumaHomeFront) lumaHomeSplashHide();
  else if (lumaHomeBoot === 'booting') lumaHomeSplashShow();
}

function lumaHomeIsInput(id) {
  return /^com\.webos\.app\.(livetv|hdmi\d|externalinput\..*)$/.test(id);
}

// Back/Exit from an app: LG's compositor closes it and opens setting lastInputApp, normally the
// input watched last (Live TV). While an app other than an input is in front it names Home, so
// Back returns to Luma; LG's input apps set it to themselves again when chosen.
function lumaHomePinInput() {
  if (
    lumaHomeLastInput === null ||
    lumaHomeLastInput === 'com.webos.app.home' ||
    !lumaHomeFront ||
    lumaHomeIsInput(lumaHomeFront)
  )
    return;
  lumaHomeLastInput = 'com.webos.app.home';
  lunaCall.send(
    'com.webos.settingsservice',
    '/setSystemSettings',
    JSON.stringify({ category: 'general', settings: { lastInputApp: 'com.webos.app.home' } }),
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
    lumaHomeFront === 'com.webos.app.home' ||
    lumaHomeLaunches >= 2
  )
    return;
  if (lumaHomeFront && !lumaHomeIsInput(lumaHomeFront)) return;
  lumaHomeLaunches++;
  lunaCall.send(
    'com.webos.applicationManager',
    '/launch',
    JSON.stringify({ id: 'com.webos.app.home' }),
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
          lumaHomeSplashCheck();
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
          if (lumaHomeFront === 'com.webos.app.home' && lumaHomeLaunches) lumaHomeShown = true;
          else lumaHomeOpen();
          lumaHomeSplashCheck();
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

// The handler named in the key filter chain (home.cjs). Any key ends the loading screen and
// passes through.
// oxlint-disable-next-line no-unused-vars
function lumaHomeKey(_key, pressed) {
  if (pressed) lumaHomeSplashHide();
  return KeyPolicy.NextPolicy;
}
