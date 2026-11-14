import * as ort from './vendor/ort.wasm.min.mjs';
import { sha256 } from './sha256.js';

const MODEL_URL =
  'https://huggingface.co/onnx-community/depth-anything-v2-small/resolve/4472b7362082ad9968fee890ca0f1e5aca36b93d/onnx/model_quantized.onnx';
const MODEL_SHA = 'fcf51f1b230362b28690bb9d1809bf0431f29cad20534e3f589bd7285547f20d';
ort.env.wasm.numThreads = 1; // Works on private-LAN HTTP without cross-origin isolation.
ort.env.wasm.proxy = false;
ort.env.wasm.wasmPaths = {
  mjs: new URL('./vendor/ort-wasm-simd-threaded.mjs', import.meta.url).href,
  wasm: new URL('./vendor/ort-wasm-simd-threaded.wasm', import.meta.url).href,
};
let session;
let running = false;

async function loadSession(id) {
  if (session) return session;
  const response = await fetch(MODEL_URL, {
    credentials: 'omit',
    referrerPolicy: 'no-referrer',
    cache: 'force-cache',
  });
  if (!response.ok || !response.body)
    throw new Error(
      'The mask model could not be downloaded. Check your internet connection or upload a mask PNG.',
    );
  const chunks = [];
  let length = 0;
  const reader = response.body.getReader();
  try {
    for (;;) {
      const { done, value } = await reader.read();
      if (done) break;
      length += value.length;
      if (length > 28 * 1024 * 1024) throw new Error('Mask model download exceeded its limit.');
      chunks.push(value);
      postMessage({
        type: 'progress',
        id,
        message: `Downloading the mask model… ${(length / 1048576).toFixed(1)} MiB`,
      });
    }
  } finally {
    await reader.cancel().catch(() => {});
  }
  const bytes = new Uint8Array(length);
  let offset = 0;
  for (const chunk of chunks) {
    bytes.set(chunk, offset);
    offset += chunk.length;
  }
  postMessage({
    type: 'progress',
    id,
    message: 'Verifying the model and starting local inference…',
  });
  if (sha256(bytes) !== MODEL_SHA)
    throw new Error('The model checksum did not match. No model was run.');
  session = await ort.InferenceSession.create(bytes, {
    executionProviders: ['wasm'],
    graphOptimizationLevel: 'all',
  });
  return session;
}

self.onmessage = async ({ data }) => {
  const { id, image } = data;
  if (running || !(image instanceof ImageBitmap)) {
    image?.close?.();
    return;
  }
  running = true;
  let input, output;
  try {
    if (image.width !== 1920 || image.height !== 1080)
      throw new Error('Generate the mask after choosing your 16:9 crop.');
    const engine = await loadSession(id);
    postMessage({ type: 'progress', id, message: 'Estimating foreground depth on this device…' });
    const width = 924,
      height = 518,
      area = width * height;
    const canvas = new OffscreenCanvas(width, height);
    const context = canvas.getContext('2d', { willReadFrequently: true });
    context.drawImage(image, 0, 0, width, height);
    const rgba = context.getImageData(0, 0, width, height).data;
    const pixels = new Float32Array(area * 3),
      mean = [0.485, 0.456, 0.406],
      std = [0.229, 0.224, 0.225];
    for (let i = 0; i < area; i++)
      for (let c = 0; c < 3; c++) pixels[c * area + i] = (rgba[i * 4 + c] / 255 - mean[c]) / std[c];
    input = new ort.Tensor('float32', pixels, [1, 3, height, width]);
    const outputs = await engine.run({ [engine.inputNames[0]]: input });
    output = outputs[engine.outputNames[0]];
    const values = output.data,
      w = output.dims.at(-1),
      h = output.dims.at(-2);
    if (w * h !== values.length || w > 1024 || h > 1024 || !values.every(Number.isFinite))
      throw new Error('The model returned invalid depth data.');
    const sorted = Float32Array.from(values).sort(),
      lo = sorted[Math.floor(values.length * 0.02)],
      hi = sorted[Math.floor(values.length * 0.98)];
    if (hi - lo < 0.00001)
      throw new Error(
        'No usable foreground depth was found. Try a different photo or upload a mask PNG.',
      );
    const raw = Uint8Array.from(values, (value) =>
      Math.round(Math.max(0, Math.min(1, (value - lo) / (hi - lo))) * 255),
    );
    // A small median removes quantization speckles before the user's cutoff.
    const depth = raw.slice(),
      neighbors = [];
    for (let y = 1; y < h - 1; y++)
      for (let x = 1; x < w - 1; x++) {
        neighbors.length = 0;
        for (let dy = -1; dy <= 1; dy++)
          for (let dx = -1; dx <= 1; dx++) neighbors.push(raw[(y + dy) * w + x + dx]);
        neighbors.sort((a, b) => a - b);
        depth[y * w + x] = neighbors[4];
      }
    postMessage({ type: 'depth', id, width: w, height: h, depth }, [depth.buffer]);
  } catch (error) {
    postMessage({
      type: 'error',
      id,
      message:
        error.message ||
        'Automatic mask generation failed. Upload a mask PNG or save without depth.',
    });
  } finally {
    input?.dispose();
    output?.dispose();
    image.close();
    running = false;
  }
};
