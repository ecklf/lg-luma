---
'lg-luma': minor
---

Add your own wallpapers: JPEG, PNG or WebP images copied into the `wallpapers` folder beside Luma's preferences appear after the bundled backgrounds in Settings → Background. A worker thread prepares each image once, a 1920×1080 crop with the bundled wallpapers' shading plus a thumbnail, so the renderer never decodes a full-size photo. A last tile shows the folder.
