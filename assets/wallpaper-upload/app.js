import { cleanName, cropRect, panPosition } from './crop.js';

const element = (id) => document.getElementById(id);
const file = element('file');
const picker = element('picker');
const editor = element('editor');
const name = element('name');
const canvas = element('preview');
const zoom = element('zoom');
const horizontal = element('horizontal');
const vertical = element('vertical');
const status = element('status');
let image;
let loading = 0;
let drag;
let busy = false;

function message(text, error = false) {
  status.textContent = text;
  status.classList.toggle('error', error);
}

function crop() {
  return cropRect(
    image.width,
    image.height,
    Number(zoom.value),
    Number(horizontal.value),
    Number(vertical.value),
  );
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
    name.value = cleanName(selected.name.replace(/\.[^.]+$/, ''));
    zoom.value = '1';
    horizontal.value = vertical.value = '0.5';
    picker.hidden = true;
    editor.hidden = false;
    draw();
    message('');
    name.focus();
  } catch (error) {
    if (generation === loading) message(error.message || 'This image could not be opened.', true);
  }
  file.value = '';
});

for (const slider of [zoom, horizontal, vertical])
  slider.addEventListener('input', () => {
    if (!busy) draw();
  });
element('change').addEventListener('click', () => file.click());
canvas.addEventListener('pointerdown', (event) => {
  if (!image || busy) return;
  canvas.setPointerCapture(event.pointerId);
  drag = { x: event.clientX, y: event.clientY };
});
canvas.addEventListener('pointermove', (event) => {
  if (!drag || busy) return;
  const rect = canvas.getBoundingClientRect();
  const current = crop();
  horizontal.value = String(
    panPosition(
      Number(horizontal.value),
      event.clientX - drag.x,
      image.width,
      current.w,
      rect.width,
    ),
  );
  vertical.value = String(
    panPosition(
      Number(vertical.value),
      event.clientY - drag.y,
      image.height,
      current.h,
      rect.height,
    ),
  );
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
  const title = cleanName(name.value);
  if (!title) {
    message('Give your wallpaper a name.', true);
    name.focus();
    return;
  }
  name.value = title;
  busy = true;
  for (const input of editor.querySelectorAll('input, button')) input.disabled = true;
  file.disabled = true;
  message('Preparing your wallpaper…');
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
    const response = await fetch(`upload?name=${encodeURIComponent(title)}`, {
      method: 'POST',
      headers: { 'Content-Type': 'image/jpeg', 'X-Luma-Upload': token },
      body: blob,
      signal: controller.signal,
    });
    const result = await response.json();
    if (!response.ok) throw new Error(result.error || 'The TV could not save this wallpaper.');
    message(
      `“${result.name}” is on your TV. Close the upload dialog and select it in Background settings.`,
    );
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
