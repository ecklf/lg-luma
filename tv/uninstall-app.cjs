// Embedded elevated reader/mutation helper, compatible with webOS Node 8.
// Send one normal appInstallService/remove subscription, never dev/remove.
var spawn = require('child_process').spawn;
var id = process.argv[1];
var done = false;
var child;
var timer;
var buffer = '';
var total = 0;
var replies = [];

function finish(ok, error) {
  if (done) return;
  done = true;
  clearTimeout(timer);
  if (child) child.kill('SIGKILL');
  process.stdout.write(
    JSON.stringify({ returnValue: ok, errorText: error, id: id, replies: replies }) + '\n'
  );
}

if (
  !id ||
  !/^[A-Za-z0-9][A-Za-z0-9._-]{0,255}$/.test(id) ||
  /^(com\.webos\.|com\.palm\.|com\.lge\.)/.test(id) ||
  [
    'com.ecklf.luma',
    'com.luma.native',
    'com.luma.launcher',
    'com.luma.launcher.native',
    'org.webosbrew.hbchannel',
    'org.webosbrew.hbchannel.service'
  ].indexOf(id) !== -1
) {
  finish(false, 'Protected or invalid application identifier');
} else if (process.getuid() !== 0) {
  finish(false, 'Homebrew Channel needs root access');
} else {
  child = spawn('/usr/bin/luna-send', [
    '-i',
    '-w',
    '15000',
    'luna://com.webos.appInstallService/remove',
    JSON.stringify({ id: id, subscribe: true })
  ]);
  timer = setTimeout(function () {
    finish(false, 'App removal completion timed out');
  }, 15000);
  child.on('error', function (error) {
    finish(false, error.message);
  });
  child.stderr.on('data', function (data) {
    total += data.length;
    if (total > 64 * 1024) finish(false, 'App removal output limit exceeded');
  });
  child.stdout.on('data', function (data) {
    total += data.length;
    if (total > 64 * 1024) return finish(false, 'App removal output limit exceeded');
    buffer += data.toString('utf8');
    // luna-send emits a stream of objects, optionally pretty printed. Count
    // braces outside strings rather than interpreting acceptance as completion.
    while (!done) {
      var start = buffer.indexOf('{');
      if (start < 0) break;
      var depth = 0;
      var quoted = false;
      var escaped = false;
      var end = -1;
      for (var i = start; i < buffer.length; i++) {
        var c = buffer[i];
        if (quoted) {
          if (escaped) escaped = false;
          else if (c === '\\') escaped = true;
          else if (c === '"') quoted = false;
        } else if (c === '"') quoted = true;
        else if (c === '{') depth++;
        else if (c === '}' && --depth === 0) {
          end = i + 1;
          break;
        }
      }
      if (end < 0) break;
      var reply;
      try {
        reply = JSON.parse(buffer.slice(start, end));
      } catch (error) {
        return finish(false, 'Invalid app removal reply: ' + error.message);
      }
      buffer = buffer.slice(end);
      if (replies.length >= 64) return finish(false, 'Too many app removal replies');
      replies.push(reply);
      var details = reply.details || {};
      if (
        reply.returnValue === false ||
        reply.statusValue === 25 ||
        details.errorCode !== undefined
      ) {
        return finish(false, reply.errorText || details.reason || 'App removal failed');
      }
      if (reply.statusValue === 31) {
        if (reply.id !== id || (details.packageId && details.packageId !== id)) {
          return finish(false, 'App removal identity mismatch');
        }
        return finish(true);
      }
    }
  });
  child.on('close', function () {
    finish(false, 'App removal ended without completion');
  });
}
