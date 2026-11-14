/* webOS 4 embeds Home in the compositor. Keep the stock UI available for recovery,
 * but route Home requests and foreground app closes to Luma. ES5 for Qt 5.6. */
var lumaLegacyPath = '/var/lib/luma-legacy-home/legacy-home-filter.js';
var lumaLegacyApp = 'com.ecklf.luma';
var lumaLegacyEnabled = true;
var lumaLegacyFailed = false;
var lumaLegacyFront = '';
var lumaLegacyClosing = '';
var lumaLegacyOpening = false;
var lumaLegacyRibbonConnected = false;
var lumaLegacyReset = false;
var lumaLegacyResetTries = 0;

function lumaLegacyResetHome() {
  if (!lumaLegacyEnabled || lumaLegacyFailed || !lumaLegacyReset) return;
  // SAM can announce foreground before the new native surface has keyboard focus.
  // Inject only into Luma's focused surface, never into the app being left.
  if (
    lumaLegacyFront !== lumaLegacyApp ||
    global.activeSurfaceAppId !== lumaLegacyApp ||
    !fullscreenView.activeFocus
  ) {
    if (++lumaLegacyResetTries < 20) genericTimer.set(150, lumaLegacyResetHome);
    return;
  }
  lumaLegacyReset = false;
  lunaCall.send(
    'com.webos.service.networkinput',
    '/test/sendKeyCode',
    '{"keyCode":102}',
    function () {}
  );
}

function lumaLegacyParse(payload) {
  return typeof payload === 'string' ? JSON.parse(payload) : payload;
}

function lumaLegacyInput(id) {
  return !id || /^com\.webos\.app\.(livetv|hdmi\d|externalinput\..*)$/.test(id);
}

function lumaLegacyOpen() {
  lumaLegacyReset = true;
  lumaLegacyResetTries = 0;
  if (!lumaLegacyEnabled || lumaLegacyFailed) return;
  if (lumaLegacyOpening) {
    genericTimer.set(100, lumaLegacyResetHome);
    return;
  }
  lumaLegacyOpening = true;
  try {
    if (typeof global.hideHome === 'function') global.hideHome();
    lunaCall.send(
      'com.webos.applicationManager',
      '/launch',
      JSON.stringify({ id: lumaLegacyApp }),
      function (payload) {
        try {
          var reply = lumaLegacyParse(payload);
          if (reply.returnValue !== true) {
            // A removed/broken Luma must leave LG Home usable, without a redirect loop.
            lumaLegacyFailed = true;
            global.goHome();
          } else {
            genericTimer.set(100, lumaLegacyResetHome);
          }
        } catch (error) {
          console.warn('Luma legacy Home reply: ' + error);
        }
      }
    );
    genericTimer.set(1000, function () {
      lumaLegacyOpening = false;
    });
  } catch (error) {
    lumaLegacyOpening = false;
    lumaLegacyFailed = true;
    console.warn('Luma legacy Home: ' + error);
  }
}

function lumaLegacyRibbon() {
  if (superRibbon.visible) lumaLegacyOpen();
}

// oxlint-disable-next-line no-unused-vars
function init() {
  try {
    superRibbon.visibleChanged.connect(lumaLegacyRibbon);
    superRibbon.stateChanged.connect(lumaLegacyRibbon);
    lumaLegacyRibbonConnected = true;
    lunaCall.subscribe(
      'com.webos.service.config',
      '/getConfigs',
      JSON.stringify({ configNames: ['com.webos.surfacemanager.keyFilters'] }),
      function (payload) {
        try {
          var reply = lumaLegacyParse(payload);
          var filters = reply.configs['com.webos.surfacemanager.keyFilters'];
          lumaLegacyEnabled = filters.some(function (entry) {
            return entry.file === lumaLegacyPath;
          });
          if (!lumaLegacyEnabled && lumaLegacyRibbonConnected) {
            superRibbon.visibleChanged.disconnect(lumaLegacyRibbon);
            superRibbon.stateChanged.disconnect(lumaLegacyRibbon);
            lumaLegacyRibbonConnected = false;
          }
        } catch (error) {
          console.warn('Luma legacy Home config: ' + error);
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
          var reply = lumaLegacyParse(payload);
          if (reply.returnValue === true) {
            lumaLegacyFront = reply.appId || '';
            if (lumaLegacyFront === lumaLegacyApp && lumaLegacyReset)
              genericTimer.set(100, lumaLegacyResetHome);
          }
        } catch (error) {
          console.warn('Luma legacy Home foreground: ' + error);
        }
      },
      true
    );
    lunaCall.subscribe(
      'com.webos.applicationManager',
      '/getAppLifeStatus',
      '{}',
      function (payload) {
        try {
          var reply = lumaLegacyParse(payload);
          if (reply.status === 'launching' && reply.appId !== lumaLegacyClosing)
            lumaLegacyClosing = '';
          if (
            reply.status === 'closing' &&
            reply.appId === lumaLegacyFront &&
            reply.appId !== lumaLegacyApp &&
            !lumaLegacyInput(reply.appId) &&
            reply.reason !== 'memoryReclaim'
          )
            lumaLegacyClosing = reply.appId;
          if (reply.status === 'stop' && reply.appId === lumaLegacyClosing) {
            var closed = lumaLegacyClosing;
            lumaLegacyClosing = '';
            genericTimer.set(350, function () {
              // Do not take the screen from another app opened during the close.
              if (lumaLegacyInput(lumaLegacyFront) || lumaLegacyFront === closed) lumaLegacyOpen();
            });
          }
        } catch (error) {
          console.warn('Luma legacy Home lifecycle: ' + error);
        }
      },
      true
    );
  } catch (error) {
    console.warn('Luma legacy Home init: ' + error);
  }
}

// Screen saver/accessibility filters run first. Prime/Voice and all other keys pass through.
// oxlint-disable-next-line no-unused-vars
function lumaLegacyHomeKey(key, pressed, autoRepeat) {
  if (!lumaLegacyEnabled || lumaLegacyFailed) return KeyPolicy.NextPolicy;
  // Some older compositors ignore SDL's Back access-policy property. NotAccepted
  // ends filtering and delivers the key to the focused app. Keep LG popups in charge
  // whenever they have focus, including keyboard, PIN and native settings overlays.
  if (
    key === WebOS.Key_webOS_Back &&
    lumaLegacyFront === lumaLegacyApp &&
    fullscreenView.activeFocus &&
    !superRibbon.visible &&
    !modalAlert.visible
  )
    return KeyPolicy.NotAccepted;
  if (key !== Qt.Key_Super_L && key !== Qt.Key_Menu && key !== WebOS.Key_webOS_Recent)
    return KeyPolicy.NextPolicy;
  if (modalAlert.visible) return KeyPolicy.Accepted;
  if (pressed && !autoRepeat) lumaLegacyOpen();
  return KeyPolicy.Accepted;
}
