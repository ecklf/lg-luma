// Explicit, optional input registration. ES5 for the TV's Node 8 runtime.
var fs = require('fs');
var execFileSync = require('child_process').execFileSync;
var APP = 'com.ecklf.luma';
function eim(method, payload) {
  var raw = execFileSync(
    '/usr/bin/luna-send',
    ['-n', '1', '-w', '4000', 'luna://com.webos.service.eim/' + method, JSON.stringify(payload)],
    { encoding: 'utf8', timeout: 8000 }
  );
  var reply = JSON.parse(raw);
  if (reply.returnValue !== true) throw new Error(method + ': ' + raw);
  return reply;
}
function listed() {
  var reply = eim('getAllInputStatus', {});
  if (!Array.isArray(reply.devices)) throw new Error('Invalid input inventory');
  return reply.devices.some(function (device) {
    return device.appId === APP;
  });
}
function configure(command) {
  if (command === 'status') return { registered: listed() };
  if (command !== 'enable' && command !== 'disable')
    throw new Error('Usage: node tv-input.cjs enable|disable|status');
  if (command === 'enable') {
    var app = JSON.parse(fs.readFileSync(__dirname + '/appinfo.json', 'utf8'));
    if (app.id !== APP || app.supportGIP !== true)
      throw new Error('Install the Luma package with supportGIP first');
    if (listed()) return { registered: true };
    // Both empty image fields are required by older EIM implementations.
    // Idea: phranklins, issue #79; API shape verified against Hearth's tvinput.ts.
    try {
      eim('addDevice', {
        appId: APP,
        pigImage: '',
        mvpdIcon: '',
        label: 'Luma',
        description: 'Luma Home'
      });
      if (!listed()) throw new Error('This firmware did not list Luma as an input');
    } catch (error) {
      // We verified that this input was absent before adding it. Also roll back a malformed
      // verification reply or an ambiguous add timeout, without deleting a pre-existing input.
      try {
        eim('deleteDevice', { appId: APP });
      } catch (cleanup) {
        throw new Error(error.message + '; cleanup failed: ' + cleanup.message + '; retry disable');
      }
      throw new Error(error.message + '; registration removed');
    }
  } else {
    eim('deleteDevice', { appId: APP });
    if (listed()) throw new Error('Luma is still registered as an input');
  }
  return { registered: command === 'enable' };
}
try {
  console.log(JSON.stringify(configure(process.argv[2] || 'status')));
} catch (error) {
  console.error('Luma TV input: ' + error.message);
  process.exitCode = 1;
}
