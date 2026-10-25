---
'lg-luma': patch
---

Fix black or partly black screens after presses that change nothing, such as Right at the end of a row: an unchanged frame named a damage pixel outside the surface, which left `EGL_KHR_partial_update` nothing to preserve. It now names, clears and redraws one pixel inside the drawable.
