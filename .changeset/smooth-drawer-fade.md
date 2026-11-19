---
"lg-luma": patch
---

Keep the app drawer animation at 60 fps on webOS 5 TVs: the drawer grid now turns fully opaque at 70% of its motion, so the last frames take the opaque drawing path. Settings → App drawer → Smoother animation (on by default) restores the full-length fade when turned off.
