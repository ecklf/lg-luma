export function thresholdAlpha(depth, threshold) {
  if (
    !(depth instanceof Uint8Array) ||
    !Number.isFinite(threshold) ||
    threshold < 0 ||
    threshold > 255
  )
    throw new Error('Invalid depth mask');
  return Uint8Array.from(depth, (value) => (value > threshold ? 255 : 0));
}

export function suggestedCutoff(depth, width, height) {
  // Cheap coarse search for an initially readable centered-clock silhouette.
  // The TV still derives exact placements from the final full-resolution PNG.
  for (const cutoff of [3, 5, 10, 20, 38, 77, 115, 153]) {
    for (let center = 296; center <= 664; center += 8) {
      let upper = 0,
        lower = 0;
      for (let y = 0; y < 8; y++)
        for (let x = 0; x < 16; x++) {
          const px = Math.min(width - 1, Math.floor(((716 + (x * 488) / 16) * width) / 1920));
          for (const side of [0, 1]) {
            const py = Math.min(
              height - 1,
              Math.floor(((center - 65 + side * 65 + (y * 65) / 8) * height) / 1080),
            );
            if (depth[py * width + px] > cutoff) {
              if (side) lower++;
              else upper++;
            }
          }
        }
      upper /= 128;
      lower /= 128;
      const cover = (upper + lower) / 2;
      if (cover >= 0.06 && cover <= 0.32 && upper < 0.22 && lower > upper + 0.03) return cutoff;
    }
  }
  return 77;
}

// Mirror the TV worker's bounded summed-area placement search. Only the mask is
// sent to the TV, which independently computes and validates all placements.
export function maskPlacements(mask) {
  if (!(mask instanceof Uint8Array) || mask.length !== 1920 * 1080)
    throw new Error('Invalid mask dimensions');
  const sums = new Uint32Array(1921 * 1081);
  for (let y = 0; y < 1080; y++) {
    let row = 0;
    for (let x = 0; x < 1920; x++) {
      row += mask[y * 1920 + x];
      sums[(y + 1) * 1921 + x + 1] = sums[y * 1921 + x + 1] + row;
    }
  }
  const mean = (x, y, w, h) =>
    (sums[(y + h) * 1921 + x + w] +
      sums[y * 1921 + x] -
      sums[y * 1921 + x + w] -
      sums[(y + h) * 1921 + x]) /
    (w * h * 255);
  const result = {};
  for (const [layout, columns, rows] of [
    ['row', 3, 1],
    ['centered', 1, 1],
    ['grid', 3, 2],
  ]) {
    const height = (584 - (rows - 1) * 24) / rows;
    result[layout] = [];
    for (let slot = 0; slot < columns * rows; slot++) {
      const x = columns === 1 ? 676 : 84 + (slot % columns) * 592,
        top = 188 + Math.floor(slot / columns) * (height + 24);
      let best;
      for (let center = top + 108; center <= top + height - 108; center += 4) {
        const upper = mean(x + 40, center - 65, 488, 65),
          lower = mean(x + 40, center, 488, 65),
          cover = (upper + lower) / 2;
        if (cover >= 0.06 && cover <= 0.32 && upper < 0.22 && lower > upper + 0.03) {
          const score =
            Math.abs(cover - 0.18) + upper + Math.abs(center - (top + height / 2)) / 5000;
          if (!best || score < best.score) best = { score, center };
        }
      }
      result[layout].push(best?.center ?? null);
    }
  }
  return result;
}

export function packWallpaper(photo, mask) {
  if (
    !(photo instanceof Blob) ||
    !(mask instanceof Blob) ||
    !photo.size ||
    !mask.size ||
    photo.size + mask.size + 16 > 12 * 1024 * 1024
  )
    throw new Error('Wallpaper and mask together must be smaller than 12 MiB.');
  const header = new Uint8Array(16);
  header.set(new TextEncoder().encode('LUMAWP01'));
  const view = new DataView(header.buffer);
  view.setUint32(8, photo.size, false);
  view.setUint32(12, mask.size, false);
  return new Blob([header, photo, mask], { type: 'application/x-luma-wallpaper' });
}
