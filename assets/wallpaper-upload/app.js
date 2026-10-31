import { cropRect, panPosition } from './crop.js';
import { loadStoragePath } from './storage.js';

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
  if (!busy) draw();
});
element('change').addEventListener('click', () => file.click());
element('another').addEventListener('click', () => {
  image?.close();
  image = undefined;
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
  busy = true;
  for (const input of editor.querySelectorAll('input, button')) input.disabled = true;
  file.disabled = true;
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
    message('Sending to your TV…');
    const token = location.pathname.split('/')[1];
    const response = await fetch('upload', {
      method: 'POST',
      headers: { 'Content-Type': 'image/jpeg', 'X-Luma-Upload': token },
      body: blob,
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
    busy = false;
    for (const input of editor.querySelectorAll('input, button')) input.disabled = false;
    file.disabled = false;
  }
});
