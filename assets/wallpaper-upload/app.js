import { cropRect, panPosition } from './crop.js';
import { loadStoragePath } from './storage.js';
import { maskPlacements, packWallpaper, suggestedCutoff, thresholdAlpha } from './mask.js';

const element = (id) => document.getElementById(id);
const file = element('file');
const picker = element('picker');
const editor = element('editor');
const canvas = element('preview');
const zoom = element('zoom');
const status = element('status');
let image;
let loading = 0;
let drag;
let busy = false;
let horizontal = 0.5;
let vertical = 0.5;
let maskCanvas, maskAlpha, maskStencil, depthData, depthWorker, generationTimer;
let generationId = 0;
let clockCenter = 480;

function setBusy(value, cancellable = false) {
  busy = value;
  for (const input of editor.querySelectorAll('input, button')) input.disabled = value;
  file.disabled = value;
  element('cancel-mask').hidden = !cancellable;
  element('cancel-mask').disabled = false;
}

function clearMask() {
  maskCanvas = maskAlpha = maskStencil = depthData = undefined;
  generationId++;
  element('mask-controls').hidden = true;
  element('use-mask').checked = true;
  element('show-mask').checked = false;
}

function applyMask(alpha, width, height) {
  const low = document.createElement('canvas');
  low.width = width;
  low.height = height;
  const pixels = new Uint8ClampedArray(width * height * 4);
  for (let i = 0; i < alpha.length; i++) {
    pixels[i * 4] = pixels[i * 4 + 1] = pixels[i * 4 + 2] = alpha[i];
    pixels[i * 4 + 3] = 255;
  }
  low.getContext('2d').putImageData(new ImageData(pixels, width, height), 0, 0);
  maskCanvas = document.createElement('canvas');
  maskCanvas.width = 1920;
  maskCanvas.height = 1080;
  const context = maskCanvas.getContext('2d', { willReadFrequently: true });
  context.drawImage(low, 0, 0, 1920, 1080);
  const stencil = context.getImageData(0, 0, 1920, 1080),
    data = stencil.data;
  maskAlpha = new Uint8Array(1920 * 1080);
  for (let i = 0; i < maskAlpha.length; i++) maskAlpha[i] = data[i * 4];
  if (maskAlpha.every((value) => value === maskAlpha[0])) {
    maskCanvas = maskAlpha = maskStencil = undefined;
    element('mask-controls').hidden = false;
    element('cutoff-row').hidden = !depthData;
    element('mask-warning').textContent =
      'This cutoff has no usable foreground. Adjust it or save without a mask.';
    draw();
    throw new Error(
      'The mask needs both foreground and background. Adjust the cutoff or choose another mask.',
    );
  }
  const placements = maskPlacements(maskAlpha);
  clockCenter = placements.centered[0] ?? 480;
  for (let i = 0; i < maskAlpha.length; i++) {
    data[i * 4] = data[i * 4 + 1] = data[i * 4 + 2] = 255;
    data[i * 4 + 3] = maskAlpha[i];
  }
  maskStencil = document.createElement('canvas');
  maskStencil.width = 1920;
  maskStencil.height = 1080;
  maskStencil.getContext('2d').putImageData(stencil, 0, 0);
  element('mask-warning').textContent =
    placements.centered[0] === null
      ? 'No safe centered-clock placement at this cutoff. The TV will use a normal clock in unsuitable cells.'
      : '';
  element('mask-controls').hidden = false;
  element('cutoff-row').hidden = !depthData;
  draw();
}

function updateCutoff() {
  if (!depthData) return;
  try {
    applyMask(
      thresholdAlpha(depthData.depth, Number(element('cutoff').value)),
      depthData.width,
      depthData.height,
    );
  } catch (error) {
    message(error.message, true);
  }
}

function endGeneration(terminate = false) {
  clearTimeout(generationTimer);
  if (terminate) {
    depthWorker?.terminate();
    depthWorker = undefined;
    generationId++;
  }
  setBusy(false);
}

const storageReady = loadStoragePath(fetch).catch(() => {
  element('storage-path').textContent = 'Storage path unavailable';
  return null;
});
storageReady.then((path) => {
  if (path) element('storage-path').textContent = path;
});

function message(text, error = false) {
  status.textContent = text;
  status.classList.toggle('error', error);
}

function crop() {
  return cropRect(image.width, image.height, Number(zoom.value), horizontal, vertical);
}

function draw(target = canvas) {
  if (!image) return;
  const { x, y, w, h } = crop();
  const context = target.getContext('2d', { alpha: false });
  context.fillStyle = '#17191c';
  context.fillRect(0, 0, target.width, target.height);
  context.drawImage(image, x, y, w, h, 0, 0, target.width, target.height);
  // Export/model input always receives the photo only, never preview clock pixels.
  if (target !== canvas || !maskCanvas) return;
  if (element('show-mask').checked) {
    context.drawImage(maskCanvas, 0, 0, canvas.width, canvas.height);
    return;
  }
  context.save();
  context.scale(canvas.width / 1920, canvas.height / 1080);
  context.font = '500 180px sans-serif';
  context.textAlign = 'center';
  context.textBaseline = 'middle';
  context.fillStyle = '#f5f5f5';
  context.fillText('20:41', 960, clockCenter);
  context.restore();
  if (element('use-mask').checked) {
    const foreground = document.createElement('canvas');
    foreground.width = canvas.width;
    foreground.height = canvas.height;
    const fg = foreground.getContext('2d');
    fg.drawImage(image, x, y, w, h, 0, 0, canvas.width, canvas.height);
    fg.globalCompositeOperation = 'destination-in';
    fg.drawImage(maskStencil, 0, 0, canvas.width, canvas.height);
    context.drawImage(foreground, 0, 0);
  }
}

file.addEventListener('change', async () => {
  const selected = file.files[0];
  if (!selected || busy) return;
  const generation = ++loading;
  message('Opening your image…');
  try {
    if (!['image/jpeg', 'image/png', 'image/webp'].includes(selected.type))
      throw new Error('Choose a JPEG, PNG or WebP image.');
    if (selected.size > 16 * 1024 * 1024) throw new Error('Choose an image smaller than 16 MiB.');
    const next = await createImageBitmap(selected, { imageOrientation: 'from-image' });
    if (generation !== loading) {
      next.close();
      return;
    }
    if (!next.width || !next.height || next.width * next.height > 32_000_000) {
      next.close();
      throw new Error('Choose an image with at most 32 megapixels.');
    }
    image?.close();
    image = next;
    clearMask();
    element('picker-title').textContent = selected.name;
    zoom.value = '1';
    horizontal = vertical = 0.5;
    picker.hidden = true;
    editor.hidden = false;
    draw();
    message('');
    zoom.focus();
  } catch (error) {
    if (generation === loading) message(error.message || 'This image could not be opened.', true);
  }
  file.value = '';
});

zoom.addEventListener('input', () => {
  if (!busy) {
    clearMask();
    draw();
  }
});
element('cutoff').addEventListener('input', updateCutoff);
for (const id of ['use-mask', 'show-mask']) element(id).addEventListener('change', () => draw());
element('clear-mask').addEventListener('click', () => {
  clearMask();
  draw();
  message('');
});
element('cancel-mask').addEventListener('click', () => {
  endGeneration(true);
  message('Mask generation cancelled. You can still save without a mask.');
});
element('choose-mask').addEventListener('click', () => element('mask-file').click());
element('mask-file').addEventListener('change', async () => {
  const selected = element('mask-file').files[0];
  if (!selected || busy || !image) return;
  setBusy(true);
  let bitmap;
  try {
    if (selected.size > 12 * 1024 * 1024) throw new Error('Choose a mask smaller than 12 MiB.');
    bitmap = await createImageBitmap(selected);
    if (bitmap.width !== 1920 || bitmap.height !== 1080)
      throw new Error('The mask must match the 1920 × 1080 crop.');
    const mask = document.createElement('canvas');
    mask.width = 1920;
    mask.height = 1080;
    const context = mask.getContext('2d');
    context.fillStyle = '#000';
    context.fillRect(0, 0, 1920, 1080);
    context.drawImage(bitmap, 0, 0);
    const pixels = context.getImageData(0, 0, 1920, 1080).data,
      alpha = new Uint8Array(1920 * 1080);
    for (let i = 0; i < alpha.length; i++)
      alpha[i] = Math.floor(
        (pixels[i * 4] * 299 + pixels[i * 4 + 1] * 587 + pixels[i * 4 + 2] * 114) / 1000,
      );
    clearMask();
    applyMask(alpha, 1920, 1080);
    message('Mask loaded. Check its alignment in the preview.');
  } catch (error) {
    message(error.message, true);
  } finally {
    bitmap?.close();
    element('mask-file').value = '';
    setBusy(false);
  }
});
element('generate-mask').addEventListener('click', async () => {
  if (busy || !image) return;
  if (!globalThis.Worker || !globalThis.OffscreenCanvas) {
    message(
      'This browser cannot generate masks. Use a recent browser, upload a mask PNG, or save without depth.',
      true,
    );
    return;
  }
  const id = ++generationId;
  setBusy(true, true);
  message('Starting local mask generation… Keep the TV dialog open.');
  try {
    depthWorker ??= new Worker(new URL('depth-worker.js', location.href), { type: 'module' });
    depthWorker.onmessage = ({ data }) => {
      if (data.id !== generationId) return;
      if (data.type === 'progress') {
        message(data.message);
        return;
      }
      if (data.type === 'error') {
        endGeneration(true);
        message(data.message, true);
        return;
      }
      if (data.type === 'depth') {
        endGeneration();
        depthData = data;
        element('cutoff').value = String(suggestedCutoff(data.depth, data.width, data.height));
        updateCutoff();
        if (maskCanvas)
          message('Mask generated locally. Adjust the cutoff and preview before saving.');
      }
    };
    depthWorker.onerror = () => {
      endGeneration(true);
      message(
        'Local mask generation could not start. Try a recent browser, upload a mask PNG, or save without depth.',
        true,
      );
    };
    generationTimer = setTimeout(() => {
      endGeneration(true);
      message(
        'Mask generation timed out on this device. Upload a mask PNG or save without depth.',
        true,
      );
    }, 240000);
    const frame = document.createElement('canvas');
    frame.width = 1920;
    frame.height = 1080;
    draw(frame);
    const snapshot = await createImageBitmap(frame);
    if (id !== generationId) {
      snapshot.close();
      return;
    }
    depthWorker.postMessage({ id, image: snapshot }, [snapshot]);
  } catch (error) {
    endGeneration(true);
    message(error.message, true);
  }
});
element('change').addEventListener('click', () => file.click());
element('another').addEventListener('click', () => {
  image?.close();
  image = undefined;
  clearMask();
  file.value = '';
  file.disabled = false;
  editor.reset();
  editor.hidden = true;
  element('success').hidden = true;
  element('storage-footer').hidden = false;
  picker.hidden = false;
  element('picker-title').textContent = 'Choose an image';
  zoom.value = '1';
  horizontal = vertical = 0.5;
  drag = undefined;
  message('');
  picker.focus();
});
canvas.addEventListener('pointerdown', (event) => {
  if (!image || busy) return;
  canvas.setPointerCapture(event.pointerId);
  drag = { x: event.clientX, y: event.clientY };
});
canvas.addEventListener('pointermove', (event) => {
  if (!drag || busy) return;
  const rect = canvas.getBoundingClientRect();
  const current = crop();
  clearMask();
  horizontal = panPosition(horizontal, event.clientX - drag.x, image.width, current.w, rect.width);
  vertical = panPosition(vertical, event.clientY - drag.y, image.height, current.h, rect.height);
  drag = { x: event.clientX, y: event.clientY };
  draw();
});
for (const event of ['pointerup', 'pointercancel', 'lostpointercapture'])
  canvas.addEventListener(event, () => {
    drag = undefined;
  });

editor.addEventListener('submit', async (event) => {
  event.preventDefault();
  if (busy || !image) return;
  const destination = await storageReady;
  if (!destination) {
    message('Could not read the TV storage path. Close and reopen this dialog.', true);
    return;
  }
  setBusy(true);
  message('Preparing your background…');
  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), 45000);
  try {
    const output = document.createElement('canvas');
    output.width = 1920;
    output.height = 1080;
    draw(output);
    const blob = await new Promise((resolve) => output.toBlob(resolve, 'image/jpeg', 0.94));
    if (!blob) throw new Error('The image could not be prepared. Try another photo.');
    const mask =
      maskCanvas && element('use-mask').checked
        ? await new Promise((resolve) => maskCanvas.toBlob(resolve, 'image/png'))
        : null;
    if (maskCanvas && element('use-mask').checked && !mask)
      throw new Error('The mask could not be prepared. Try again or save without depth.');
    const body = mask ? packWallpaper(blob, mask) : blob;
    message('Sending to your TV…');
    const token = location.pathname.split('/')[1];
    const response = await fetch('upload', {
      method: 'POST',
      headers: {
        'Content-Type': mask ? 'application/x-luma-wallpaper' : 'image/jpeg',
        'X-Luma-Upload': token,
      },
      body,
      signal: controller.signal,
    });
    const result = await response.json();
    if (!response.ok) throw new Error(result.error || 'The TV could not save this background.');
    element('success-path').textContent = destination;
    editor.hidden = true;
    element('success').hidden = false;
    element('storage-footer').hidden = true;
    message('');
  } catch (error) {
    message(
      error.name === 'AbortError'
        ? 'The TV did not respond in time. Check the TV gallery before retrying.'
        : error instanceof TypeError
          ? 'The connection was lost. Keep the TV dialog open and check the gallery before retrying.'
          : error.message,
      true,
    );
  } finally {
    clearTimeout(timeout);
    setBusy(false);
  }
});
