// Optional real-browser/model check. Supply an already downloaded pinned model,
// so this test never needs a model service, weather API, TV, or user profile.
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import path from 'node:path';
import http from 'node:http';
import { spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import sharp from 'sharp';

const root = path.resolve(import.meta.dirname, '..');
assert.ok(process.env.LUMA_DEPTH_MODEL, 'Set LUMA_DEPTH_MODEL to the pinned local q8 ONNX file');
const model = await fs.readFile(process.env.LUMA_DEPTH_MODEL);
assert.equal(
  createHash('sha256').update(model).digest('hex'),
  'fcf51f1b230362b28690bb9d1809bf0431f29cad20534e3f589bd7285547f20d',
);
const temporary = await fs.mkdtemp(path.join(root, '.build/mask-browser-'));
const source = path.join(root, 'assets/wallpaper-upload');
const photo = await fs.readFile(path.join(root, '.build/assets/wallpapers/summit-plain.jpg'));
const policy = (await fs.readFile(path.join(root, 'src/wallpapers.rs'), 'utf8')).match(
  /const BROWSER_POLICY: &str\s*=\s*"([^"]+)"/,
)[1];
let finish,
  fail,
  child,
  timer,
  modelRequests = 0;
const completed = new Promise((resolve, reject) => {
  finish = resolve;
  fail = reject;
});
const uploads = [];
const browserScript = `
import './app.js';
const el=id=>document.getElementById(id);
const until=async check=>{const end=Date.now()+180000; while(!check()) {if(Date.now()>end) throw new Error('Timed out: '+el('status').textContent); await new Promise(resolve=>setTimeout(resolve,50));}};
const select=async(id,bytes,name,type)=>{const transfer=new DataTransfer(); transfer.items.add(new File([bytes],name,{type})); el(id).files=transfer.files; el(id).dispatchEvent(new Event('change'));};
try {
  const photo=await (await fetch('photo.jpg')).blob();
  await select('file',photo,'sample.jpg','image/jpeg');
  await until(()=>!el('editor').hidden);
  el('generate-mask').click();
  await until(()=>!el('mask-controls').hidden&&!el('submit').disabled);
  if(el('cutoff-row').hidden) throw new Error('Automatic cutoff was not offered');
  await fetch('preview',{method:'POST',body:await new Promise(resolve=>el('preview').toBlob(resolve,'image/png'))});
  el('editor').requestSubmit(); await until(()=>!el('success').hidden);
  el('another').click();
  await select('file',photo,'sample.jpg','image/jpeg'); await until(()=>!el('editor').hidden);
  const mask=await(await fetch('mask.png')).blob();
  await select('mask-file',mask,'mask.png','image/png');
  await until(()=>!el('mask-controls').hidden&&!el('submit').disabled);
  el('zoom').value='1.1'; el('zoom').dispatchEvent(new Event('input'));
  if(!el('mask-controls').hidden) throw new Error('Changing crop did not invalidate the mask');
  el('zoom').value='1'; el('zoom').dispatchEvent(new Event('input'));
  await select('mask-file',mask,'mask.png','image/png');
  await until(()=>!el('mask-controls').hidden&&!el('submit').disabled);
  if(!el('cutoff-row').hidden) throw new Error('Manual mask unexpectedly has automatic cutoff controls');
  el('editor').requestSubmit(); await until(()=>!el('success').hidden);
  el('another').click(); await select('file',photo,'sample.jpg','image/jpeg'); await until(()=>!el('editor').hidden);
  el('generate-mask').click(); el('cancel-mask').click();
  if(el('submit').disabled) throw new Error('Cancellation left saving disabled');
  el('editor').requestSubmit(); await until(()=>!el('success').hidden);
  await fetch('result',{method:'POST',body:JSON.stringify({ok:true})});
} catch(error) {await fetch('result',{method:'POST',body:JSON.stringify({error:error.message,status:el('status').textContent})});}
`;
const server = http.createServer(async (req, res) => {
  try {
    const route = req.url.replace(/^\/secret\//, '');
    const send = (kind, body, status = 200) => {
      res.writeHead(status, {
        'Content-Type': kind,
        'Content-Security-Policy': policy,
        'Referrer-Policy': 'no-referrer',
        'Cache-Control': 'no-store',
      });
      res.end(body);
    };
    if (req.method === 'POST') {
      const chunks = [];
      let length = 0;
      for await (const chunk of req) {
        length += chunk.length;
        assert.ok(length <= 12 * 1024 * 1024);
        chunks.push(chunk);
      }
      const body = Buffer.concat(chunks);
      if (route === 'result') {
        const result = JSON.parse(body);
        if (result.error) fail(new Error(JSON.stringify(result)));
        else finish();
        send('application/json', '{}');
        return;
      }
      if (route === 'preview') {
        await fs.writeFile(path.join(root, '.build/uploaded-mask-preview.png'), body);
        send('application/json', '{}');
        return;
      }
      assert.equal(route, 'upload');
      assert.equal(req.headers['x-luma-upload'], 'secret');
      if (req.headers['content-type'] === 'application/x-luma-wallpaper') {
        assert.equal(body.subarray(0, 8).toString(), 'LUMAWP01');
        const photoLength = body.readUInt32BE(8),
          maskLength = body.readUInt32BE(12);
        assert.equal(body.length, 16 + photoLength + maskLength);
        const jpeg = body.subarray(16, 16 + photoLength),
          mask = body.subarray(16 + photoLength);
        assert.equal((await sharp(jpeg).metadata()).format, 'jpeg');
        const info = await sharp(mask).metadata();
        assert.equal(info.width, 1920);
        assert.equal(info.height, 1080);
        assert.equal(info.format, 'png');
        const rgba = await sharp(mask).removeAlpha().greyscale().raw().toBuffer();
        assert.ok(rgba.includes(0) && rgba.includes(255));
        // The export must contain the photo only, not the preview clock.
        const actual = await sharp(jpeg).raw().toBuffer(),
          expected = await sharp(photo).raw().toBuffer();
        let difference = 0;
        for (let i = 0; i < actual.length; i++) difference += Math.abs(actual[i] - expected[i]);
        assert.ok(difference / actual.length < 3, 'Preview clock leaked into the saved photo');
        uploads.push({ mask, jpeg });
      } else {
        assert.equal(req.headers['content-type'], 'image/jpeg');
        uploads.push({});
      }
      send('application/json', JSON.stringify({ id: 'custom-' + '1'.repeat(32) }), 201);
      return;
    }
    if (route === 'result') {
      send('application/json', '{}');
      return;
    }
    if (route === 'model.onnx') {
      modelRequests++;
      send('application/octet-stream', model);
      return;
    }
    if (route === 'photo.jpg') {
      send('image/jpeg', photo);
      return;
    }
    if (route === 'mask.png') {
      send('image/png', uploads[0].mask);
      return;
    }
    if (route === 'browser-check.js') {
      send('text/javascript', browserScript);
      return;
    }
    if (route === 'info.json') {
      send('application/json', JSON.stringify({ path: '/media/internal/luma/wallpapers' }));
      return;
    }
    let file = route || 'index.html';
    assert.ok(
      [
        'index.html',
        'app.css',
        'app.js',
        'crop.js',
        'storage.js',
        'mask.js',
        'sha256.js',
        'depth-worker.js',
        'luma-mark.svg',
        'vendor/ort.wasm.min.mjs',
        'vendor/ort-wasm-simd-threaded.mjs',
        'vendor/ort-wasm-simd-threaded.wasm',
      ].includes(file),
      `Unexpected browser route: ${route}`,
    );
    let body = await fs.readFile(
      file === 'luma-mark.svg'
        ? path.join(root, 'assets/app-icon.svg')
        : file.startsWith('vendor/')
          ? path.join(root, '.build/assets/wallpaper-runtime', file.slice(7))
          : path.join(source, file),
    );
    if (file === 'index.html')
      body = Buffer.from(
        body
          .toString()
          .replace('</body>', '<script type="module" src="browser-check.js"></script></body>'),
      );
    if (file === 'depth-worker.js')
      body = Buffer.from(
        body
          .toString()
          .replace(
            /https:\/\/huggingface.co\/onnx-community\/depth-anything-v2-small\/resolve\/[0-9a-f]+\/onnx\/model_quantized.onnx/,
            `http://127.0.0.1:${server.address().port}/secret/model.onnx`,
          ),
      );
    send(
      file.endsWith('.wasm')
        ? 'application/wasm'
        : file.endsWith('.css')
          ? 'text/css'
          : file.endsWith('.html')
            ? 'text/html'
            : file.endsWith('.svg')
              ? 'image/svg+xml'
              : 'text/javascript',
      body,
    );
  } catch (error) {
    res.writeHead(500);
    res.end(String(error));
    fail(error);
  }
});
try {
  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
  const prefs = {
    'browser.shell.checkDefaultBrowser': false,
    'browser.startup.page': 0,
    'browser.newtabpage.enabled': false,
    'toolkit.telemetry.enabled': false,
    'datareporting.healthreport.uploadEnabled': false,
    'network.captive-portal-service.enabled': false,
    'network.connectivity-service.enabled': false,
    'network.proxy.type': 1,
    'network.proxy.http': '127.0.0.1',
    'network.proxy.http_port': 9,
    'network.proxy.ssl': '127.0.0.1',
    'network.proxy.ssl_port': 9,
    'network.proxy.no_proxies_on': 'localhost,127.0.0.1',
  };
  await fs.writeFile(
    path.join(temporary, 'user.js'),
    Object.entries(prefs)
      .map(([k, v]) => `user_pref(${JSON.stringify(k)}, ${JSON.stringify(v)});`)
      .join('\n'),
  );
  child = spawn(
    process.env.FIREFOX ?? 'firefox',
    [
      '--headless',
      '--no-remote',
      '--profile',
      temporary,
      `http://127.0.0.1:${server.address().port}/secret/`,
    ],
    { stdio: ['ignore', 'pipe', 'pipe'] },
  );
  let log = '';
  child.stderr.on('data', (data) => {
    log = (log + data.toString()).slice(-8000);
  });
  child.on('error', fail);
  child.on('exit', (code) => {
    if (code) fail(new Error(`Firefox exited ${code}: ${log}`));
  });
  timer = setTimeout(() => fail(new Error(`Browser check timed out: ${log}`)), 240000);
  await completed;
  assert.equal(uploads.length, 3);
  assert.ok(uploads[0].mask && uploads[1].mask && !uploads[2].mask);
  assert.equal(modelRequests, 1);
  console.log(
    'Firefox browser checks passed: real local WASM inference, checksum, mask preview, crop invalidation, manual masks, cancellation and atomic upload envelopes.',
  );
} finally {
  clearTimeout(timer);
  child?.kill('SIGTERM');
  server.closeAllConnections();
  await new Promise((resolve) => server.close(resolve));
  if (child?.pid && child.exitCode === null && child.signalCode === null)
    await new Promise((resolve) => child.once('exit', resolve));
  await fs.rm(temporary, { recursive: true, force: true });
}
