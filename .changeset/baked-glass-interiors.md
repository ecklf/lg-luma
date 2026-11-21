---
"lg-luma": patch
---

Draw resting Liquid Glass faster on webOS 5 Mali TVs: the interior of glass over a single wallpaper is rendered once per wallpaper and then read with one texture lookup, while rims keep the full lens and highlight shading.
