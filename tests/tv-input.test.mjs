import assert from 'node:assert/strict';
import fs from 'node:fs';
import test from 'node:test';
import vm from 'node:vm';
const source = fs.readFileSync(new URL('../tv/tv-input.cjs', import.meta.url), 'utf8');
function fixture({
  supports = true,
  visible = true,
  malformed = false,
  verificationFails = false,
  removeFails = false,
} = {}) {
  const calls = [];
  let registered = false;
  const context = vm.createContext({
    __dirname: '/app',
    process: { argv: ['node', 'tv-input.cjs', 'status'] },
    console: { log() {}, error() {} },
    require(name) {
      if (name === 'fs')
        return {
          readFileSync: () => JSON.stringify({ id: 'com.ecklf.luma', supportGIP: supports }),
        };
      assert.equal(name, 'child_process');
      return {
        execFileSync(file, args) {
          const method = args[4].split('/').at(-1);
          calls.push([method, JSON.parse(args[5])]);
          if (method === 'addDevice') registered = true;
          if (method === 'deleteDevice' && removeFails) return '{"returnValue":false}';
          if (method === 'deleteDevice') registered = false;
          if (method === 'getAllInputStatus' && registered && verificationFails) return 'malformed';
          return malformed
            ? 'not JSON'
            : JSON.stringify({
                returnValue: true,
                devices: registered && visible ? [{ appId: 'com.ecklf.luma' }] : [],
              });
        },
      };
    },
  });
  vm.runInContext(source, context);
  return { context, calls };
}
test('registration verifies the listed input and carries both required image fields', () => {
  const { context, calls } = fixture();
  assert.equal(context.configure('enable').registered, true);
  const payload = calls.find(([method]) => method === 'addDevice')[1];
  assert.equal(payload.pigImage, '');
  assert.equal(payload.mvpdIcon, '');
  assert.equal(payload.appId, 'com.ecklf.luma');
  context.configure('enable');
  assert.equal(calls.filter(([method]) => method === 'addDevice').length, 1);
  assert.equal(context.configure('disable').registered, false);
});
test('an accepted but unlisted registration is rolled back', () => {
  const { context, calls } = fixture({ visible: false });
  assert.throws(() => context.configure('enable'), /registration removed/);
  assert.equal(calls.at(-1)[0], 'deleteDevice');
});
test('unsupported manifests and malformed replies cannot report success', () => {
  assert.throws(() => fixture({ supports: false }).context.configure('enable'), /supportGIP/);
  assert.throws(() => fixture({ malformed: true }).context.configure('enable'));
});

test('verification errors roll back newly added inputs and cleanup failures remain visible', () => {
  const tv = fixture({ verificationFails: true });
  assert.throws(() => tv.context.configure('enable'), /registration removed/);
  assert.equal(tv.calls.at(-1)[0], 'deleteDevice');
  const failed = fixture({ visible: false, removeFails: true });
  assert.throws(() => failed.context.configure('enable'), /cleanup failed.*retry disable/);
});
