---
'lg-luma': patch
---

Precompute lossless Liquid Glass wallpaper backdrops during asset preparation and uploads instead of decoding, resizing and blurring them before the first frame. Add startup diagnostics and regression coverage while preserving legacy wallpaper fallback.
