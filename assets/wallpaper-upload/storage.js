export async function loadStoragePath(fetcher) {
  const response = await fetcher('info.json', { cache: 'no-store' });
  if (!response.ok) throw new Error('Storage location unavailable');
  const info = await response.json();
  if (typeof info.path !== 'string' || !info.path.startsWith('/')) {
    throw new Error('Invalid storage location');
  }
  return info.path;
}
