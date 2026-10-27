// Coordinates are in the orientation-corrected source image, never in CSS pixels.
export function cropRect(width, height, zoom, horizontal, vertical) {
  const scale = Math.max(1920 / width, 1080 / height) * zoom;
  const w = 1920 / scale;
  const h = 1080 / scale;
  return { x: (width - w) * horizontal, y: (height - h) * vertical, w, h };
}

export function panPosition(position, delta, sourceExtent, cropExtent, previewExtent) {
  const range = sourceExtent - cropExtent;
  return range <= 0.001
    ? 0.5
    : Math.max(0, Math.min(1, position - ((delta / previewExtent) * cropExtent) / range));
}
