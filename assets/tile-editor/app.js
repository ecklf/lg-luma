import { checkListing, describe, loadListing, normalizeUrl, post } from './tiles.js';

const element = (id) => document.getElementById(id);
const overview = element('overview');
const editor = element('editor');
const list = element('tiles');
const status = element('status');
const token = location.pathname.split('/')[1];
let listing;
let editing;
let busy = false;
let removing = false;

function message(text, error = false) {
  status.textContent = text;
  status.classList.toggle('error', error);
}

function icon(id) {
  const known = listing.icons.find((item) => item.id === id) ?? listing.icons[0];
  // The markup is one of the TV's own bundled interface icons, never page input.
  const svg = new DOMParser().parseFromString(known.svg, 'image/svg+xml').documentElement;
  svg.removeAttribute('width');
  svg.removeAttribute('height');
  svg.setAttribute('aria-hidden', 'true');
  return document.importNode(svg, true);
}

function render() {
  list.replaceChildren(
    ...listing.tiles.map((tile) => {
      const item = document.createElement('li');
      const button = document.createElement('button');
      button.type = 'button';
      button.className = 'tile';
      const mark = document.createElement('span');
      mark.className = 'mark';
      mark.append(icon(tile.icon));
      const text = document.createElement('span');
      text.className = 'text';
      const name = document.createElement('strong');
      name.textContent = tile.title;
      const detail = document.createElement('small');
      detail.textContent = describe(tile);
      text.append(name, detail);
      button.append(mark, text);
      button.addEventListener('click', () => open(tile));
      item.append(button);
      return item;
    }),
  );
  element('empty').hidden = listing.tiles.length > 0;
  element('add').disabled = listing.tiles.length >= listing.limit;
  element('icons').replaceChildren(
    ...listing.icons.map((item) => {
      const label = document.createElement('label');
      label.title = item.name;
      const input = document.createElement('input');
      input.type = 'radio';
      input.name = 'icon';
      input.value = item.id;
      input.setAttribute('aria-label', item.name);
      label.append(input, icon(item.id));
      return label;
    }),
  );
}

function open(tile) {
  editing = tile;
  removing = false;
  element('editor-title').textContent = tile ? 'Edit shortcut' : 'New shortcut';
  element('title').value = tile?.title ?? '';
  element('url').value = tile?.url ?? '';
  editor.elements.method.value = tile?.method ?? 'POST';
  editor.elements.icon.value = tile?.icon || listing.icons[0].id;
  const remove = element('remove');
  remove.hidden = !tile;
  remove.textContent = 'Remove shortcut';
  overview.hidden = true;
  editor.hidden = false;
  message('');
  element('title').focus();
}

function close() {
  editing = undefined;
  editor.hidden = true;
  overview.hidden = false;
}

function draft() {
  return {
    id: editing?.id ?? '',
    title: element('title').value.trim(),
    url: normalizeUrl(element('url').value),
    method: editor.elements.method.value,
    icon: editor.elements.icon.value,
  };
}

async function request(route, body, working) {
  if (busy) return;
  busy = true;
  for (const control of editor.querySelectorAll('input, button')) control.disabled = true;
  message(working);
  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), 15000);
  try {
    return await post(fetch, token, route, body, controller.signal);
  } catch (error) {
    message(
      error.name === 'AbortError'
        ? 'The TV did not respond in time. Keep the TV dialog open and try again.'
        : error instanceof TypeError
          ? 'The connection was lost. Reopen the dialog on your TV and scan the new code.'
          : error.message,
      true,
    );
  } finally {
    clearTimeout(timeout);
    busy = false;
    for (const control of editor.querySelectorAll('input, button')) control.disabled = false;
  }
}

function update(result, done) {
  if (!result) return;
  listing = checkListing(result);
  render();
  close();
  message(done);
}

editor.addEventListener('submit', async (event) => {
  event.preventDefault();
  const tile = draft();
  const result = await request('save', tile, 'Saving to your TV…');
  update(result, `${tile.title} ${editing ? 'saved' : 'added to Home'}.`);
});

element('send').addEventListener('click', async () => {
  const tile = draft();
  if (!tile.title || !tile.url) {
    message('Add a name and a web address first.', true);
    return;
  }
  if (await request('send', tile, 'Your TV is sending the request…')) {
    message(`${tile.title} sent. This did not save the shortcut.`);
  }
});

element('remove').addEventListener('click', async (event) => {
  if (!editing) return;
  // A second press confirms, so a stray tap cannot remove a shortcut.
  if (!removing) {
    removing = true;
    event.currentTarget.textContent = 'Press again to remove';
    return;
  }
  const { title } = editing;
  update(await request('remove', { id: editing.id }, 'Removing…'), `${title} removed.`);
});

element('add').addEventListener('click', () => open());
element('cancel').addEventListener('click', () => {
  close();
  message('');
});

loadListing(fetch)
  .then((result) => {
    listing = result;
    render();
  })
  .catch(() => {
    message('Could not read the shortcuts on your TV. Close and reopen the dialog.', true);
    element('add').disabled = true;
  });
