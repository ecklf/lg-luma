---
'lg-luma': patch
---

Redraw only what changed since the back buffer's frame. Consecutive scenes are compared command by command and only the changed area is cleared and drawn, using EGL buffer age with `EGL_KHR_partial_update` where available. On an LG webOS 5 TV, focus moves on Home run at 60 instead of 39 fps at 1080p (p95 17 instead of 34 ms) and mixed navigation redraws a fifth of the screen on average.
