// Like the TV keyboard, an address typed without a scheme is sent over http.
export function normalizeUrl(text) {
  const url = text.trim();
  return url && !url.includes('://') ? `http://${url}` : url;
}

export function describe(tile) {
  return `${tile.method} · ${tile.url}`;
}

export async function loadListing(fetcher) {
  const response = await fetcher('tiles.json', { cache: 'no-store' });
  if (!response.ok) throw new Error('Shortcuts unavailable');
  return checkListing(await response.json());
}

export function checkListing(listing) {
  if (
    !Array.isArray(listing?.tiles) ||
    !Array.isArray(listing.icons) ||
    !listing.icons.length ||
    !Number.isInteger(listing.limit)
  ) {
    throw new Error('Invalid shortcut list');
  }
  return listing;
}

// The token in the address is the session's capability; writes repeat it as a header.
export async function post(fetcher, token, route, body, signal) {
  const response = await fetcher(route, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json', 'X-Luma-Token': token },
    body: JSON.stringify(body),
    signal,
  });
  const result = await response.json().catch(() => ({}));
  if (!response.ok) throw new Error(result.error || 'The TV could not complete this change.');
  return result;
}
