---
"lg-luma": patch
---

Preserve the selected MSAA text rendering when fading pages through offscreen layers, with the original rendering path retained when multisample resolve is unavailable. Include layer preparation and drawing in frame diagnostics instead of reporting only the final composition pass.
