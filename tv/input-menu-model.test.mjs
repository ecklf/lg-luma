import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';

const source = fs
  .readFileSync(new URL('./input-menu-model.qml', import.meta.url), 'utf8')
  .replace(/^\s*property bool lumaReady: false\s*/, '');
function harness(entries = {}) {
  const rows = [];
  const devices = [
    {
      id: 'HDMI_1',
      appId: 'com.webos.app.hdmi1',
      label: 'Item 1',
      separator: 'one',
      inputType: 'input',
    },
    {
      id: 'HDMI_2',
      appId: 'com.webos.app.hdmi2',
      label: 'Item 2',
      separator: 'two',
      inputType: 'input',
    },
    {
      id: 'USB',
      appId: 'example.storage',
      label: 'Item 3',
      separator: 'three',
      inputType: 'storage',
    },
  ];
  const ribbon = { currentIndex: 0 };
  let closed = 0;
  const context = {
    JSON,
    rows,
    inputPickerMainView: { mainRibbon: ribbon },
    inputPickerRoot: {
      styler: { lumaVisibleCount: 4 },
      lumaMenu: { enabled: true, entries },
      props: { forgroundAppValue: 'com.webos.app.hdmi2' },
      inputPickerUtils: {
        titleItemObj: [
          {
            id: 'titleItem',
            appId: 'example.dashboard',
            label: 'Item 0',
            separator: 'dashboard',
            inputType: 'defaultTile',
            activate: true,
          },
        ],
      },
      close() {
        closed++;
      },
      showView() {},
      getLastInputIndex() {
        return Math.max(
          0,
          rows.findIndex((x) => x.appId === 'com.webos.app.hdmi2'),
        );
      },
    },
    reOrderListModel() {
      return devices.map((x) => ({ ...x }));
    },
    get(i) {
      assert(i >= 0 && i < rows.length);
      return rows[i];
    },
    clear() {
      rows.length = 0;
    },
    append(item) {
      rows.push({ ...item });
    },
  };
  Object.defineProperty(context, 'count', {
    get() {
      return rows.length;
    },
  });
  vm.createContext(context);
  vm.runInContext(source, context);
  return { rows, ribbon, context, closed: () => closed, rebuild: () => context.lumaRebuild(false) };
}
test('hiding the dashboard does not skip the first physical input during updates', () => {
  const h = harness({ home_dashboard: false });
  h.rebuild();
  assert.deepEqual(
    h.rows.map((x) => x.id),
    ['HDMI_1', 'HDMI_2', 'USB'],
  );
  assert.equal(h.rows[0].hasCategory, true);
  h.context.inputPickerRoot.lumaMenu.entries.HDMI_1 = false;
  h.rebuild();
  assert.deepEqual(
    h.rows.map((x) => x.id),
    ['HDMI_2', 'USB'],
  );
});
test('a live settings change preserves the selected source by identity', () => {
  const h = harness();
  h.rebuild();
  h.ribbon.currentIndex = h.rows.findIndex((x) => x.id === 'HDMI_1');
  h.context.inputPickerRoot.lumaMenu.entries.home_dashboard = false;
  h.rebuild();
  assert.equal(h.rows[h.ribbon.currentIndex].id, 'HDMI_1');
});
test('hiding every visible option safely closes without opening an empty ribbon', () => {
  const h = harness({ home_dashboard: false, HDMI_1: false, HDMI_2: false, usb: false });
  h.rebuild();
  assert.equal(h.rows.length, 0);
  assert.equal(h.closed(), 1);
});
test('disabled customization shows all original entries', () => {
  const h = harness({ home_dashboard: false, HDMI_1: false, HDMI_2: false, usb: false });
  h.context.inputPickerRoot.lumaMenu.enabled = false;
  h.rebuild();
  assert.deepEqual(
    h.rows.map((x) => x.id),
    ['titleItem', 'HDMI_1', 'HDMI_2', 'USB'],
  );
  assert.equal(h.closed(), 0);
});

test('the ribbon remains usable before configuration is available', () => {
  for (const settings of [undefined, null]) {
    const h = harness();
    h.context.inputPickerRoot.lumaMenu = settings;
    h.rebuild();
    assert.deepEqual(
      h.rows.map((x) => x.id),
      ['titleItem', 'HDMI_1', 'HDMI_2', 'USB'],
    );
    assert.equal(h.closed(), 0);
  }
});

test('configuration responses replace initialized settings only when valid', () => {
  const controller = fs.readFileSync(
    new URL('./input-menu-controller.qml', import.meta.url),
    'utf8',
  );
  const accept = controller.match(
    /function lumaAcceptSettings\(response\) \{[\s\S]*?\n        \}/,
  )?.[0];
  assert(accept, 'the service response handler must be present');
  const initial = { enabled: false, entries: {} };
  const context = { inputPickerRoot: { lumaMenu: initial } };
  vm.createContext(context);
  vm.runInContext(accept, context);
  for (const response of [
    undefined,
    null,
    {},
    { returnValue: false },
    { returnValue: true, configs: {} },
    {
      returnValue: true,
      configs: { 'com.ecklf.luma.inputMenu': { enabled: true } },
    },
  ]) {
    context.lumaAcceptSettings(response);
    assert.equal(context.inputPickerRoot.lumaMenu, initial);
  }
  const value = { enabled: true, entries: { home_dashboard: false } };
  context.lumaAcceptSettings({ returnValue: true, configs: { 'com.ecklf.luma.inputMenu': value } });
  assert.equal(context.inputPickerRoot.lumaMenu, value);
});
