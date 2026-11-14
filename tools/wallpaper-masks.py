"""Optional, offline Depth Anything V2 Small mask generation. Never runs on the TV.

Requires numpy, Pillow and onnxruntime. Run via pnpm assets:depth, passing the pinned
ONNX model with --model. No image or model is uploaded and this script makes no requests.
"""

import argparse
import hashlib
import json
from pathlib import Path

import numpy as np
import onnxruntime as ort
from PIL import Image, ImageFilter

MODEL_SHA256 = "fcf51f1b230362b28690bb9d1809bf0431f29cad20534e3f589bd7285547f20d"
MODEL_REVISION = "4472b7362082ad9968fee890ca0f1e5aca36b93d"
ROOT = Path(__file__).resolve().parent.parent


def placements(mask):
    """Find lightly occluded clock positions instead of burying the entire clock.

    Sample the clock's central glyph band, favouring a clear upper half and a
    foreground silhouette over its lower half. Reject fully covered/clear cells.
    """
    alpha = np.asarray(mask, dtype=np.float32) / 255
    result = {}
    for layout, columns, rows in [("row", 3, 1), ("centered", 1, 1), ("grid", 3, 2)]:
        values = []
        height = (584 - (rows - 1) * 24) / rows
        for slot in range(columns * rows):
            x = 676 if columns == 1 else 84 + slot % columns * 592
            top = 188 + slot // columns * (height + 24)
            best = None
            for center in range(round(top + 108), round(top + height - 108) + 1, 4):
                region = alpha[center - 65 : center + 65, x + 40 : x + 528]
                cover = float(region.mean())
                upper = float(region[:65].mean())
                lower = float(region[65:].mean())
                if 0.06 <= cover <= 0.32 and upper < 0.22 and lower > upper + 0.03:
                    score = abs(cover - 0.18) + upper + abs(center - (top + height / 2)) / 5000
                    if best is None or score < best[0]:
                        best = (score, center)
            values.append(best[1] if best else None)
        result[layout] = values
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--model", type=Path, required=True)
    parser.add_argument("--preview-dir", type=Path)
    args = parser.parse_args()
    if hashlib.sha256(args.model.read_bytes()).hexdigest() != MODEL_SHA256:
        parser.error("Model checksum does not match the pinned Depth Anything V2 Small q8 export")
    options = ort.SessionOptions()
    options.intra_op_num_threads = 4
    session = ort.InferenceSession(str(args.model), options, providers=["CPUExecutionProvider"])
    output = ROOT / "assets/wallpaper-masks"
    output.mkdir(exist_ok=True)
    manifest = {}
    for source in sorted((ROOT / "assets/wallpapers").glob("*.jpg")):
        # Infer on the exact cropped, colour-adjusted frame used by the renderer.
        image = Image.open(ROOT / f".build/depth-frames/{source.name}").convert("RGB")
        width, height = image.size
        scale = 518 / min(width, height)
        size = (round(width * scale / 14) * 14, round(height * scale / 14) * 14)
        pixels = np.asarray(image.resize(size, Image.Resampling.BICUBIC), dtype=np.float32) / 255
        mean = np.array([0.485, 0.456, 0.406], dtype=np.float32)
        std = np.array([0.229, 0.224, 0.225], dtype=np.float32)
        pixels = (pixels - mean) / std
        depth = session.run(
            None, {session.get_inputs()[0].name: pixels.transpose(2, 0, 1)[None]}
        )[0].squeeze()
        if not np.isfinite(depth).all() or float(np.ptp(depth)) < 1e-5:
            raise ValueError(f"Invalid depth prediction for {source.name}")
        lo, hi = np.percentile(depth, [2, 98])
        normalized = np.clip((depth - lo) / max(hi - lo, 1e-5), 0, 1)
        # Larger inverse-depth values are nearer. Keep a narrow feather at the
        # depth boundary, not a broad translucent duplicate of the wallpaper.
        candidates = []
        # Denoise before thresholding, then close pinholes. Feather only the final
        # silhouette edge, not a wide range of depths across the clock's digits.
        filtered = np.asarray(
            Image.fromarray(np.round(normalized * 255).astype(np.uint8))
            .filter(ImageFilter.MedianFilter(5)), dtype=np.float32
        ) / 255
        for threshold in [0.01, 0.02, 0.04, 0.08, 0.15, 0.30, 0.45, 0.60]:
            mask = (
                Image.fromarray(np.where(filtered > threshold, 255, 0).astype(np.uint8))
                .filter(ImageFilter.MaxFilter(3))
                .filter(ImageFilter.MinFilter(3))
                .resize((1920, 1080), Image.Resampling.BILINEAR)
                .filter(ImageFilter.GaussianBlur(0.6))
            )
            positions = placements(mask)
            score = (
                sum(value is not None for value in positions["row"])
                + 100 * sum(value is not None for value in positions["centered"])
                + 0.2 * sum(value is not None for value in positions["grid"])
                - threshold * 10
            )
            candidates.append((score, mask, positions, threshold))
        _, mask, positions, threshold = max(candidates, key=lambda candidate: candidate[0])
        mask_path = output / f"{source.stem}.png"
        mask.save(mask_path, optimize=True)
        manifest[source.stem] = {
            "source_sha256": hashlib.sha256(source.read_bytes()).hexdigest(),
            "mask_sha256": hashlib.sha256(mask_path.read_bytes()).hexdigest(),
            "model_revision": MODEL_REVISION,
            "model_sha256": MODEL_SHA256,
            "threshold": threshold,
            "placements": positions,
        }
        if args.preview_dir:
            args.preview_dir.mkdir(parents=True, exist_ok=True)
            preview = Image.fromarray(np.round(normalized * 255).astype(np.uint8))
            preview.resize((960, 540)).save(args.preview_dir / f"{source.stem}-depth.png")
        print(source.stem, manifest[source.stem]["placements"], flush=True)
    (output / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")


if __name__ == "__main__":
    main()
