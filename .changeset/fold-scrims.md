---
'lg-luma': patch
---

Fold full-screen translucent fills into the wallpaper. The Recents backdrop and gradient each cost a full screen of fill per frame; the wallpaper is now drawn once with their combined effect, and during the Recents crossfade they sit beneath the outgoing interface so they fold in as well. On an LG webOS 5 TV the Recents carousel runs at 48 instead of 37 fps and opening or closing Recents drops from 100 to 63 ms at the 95th percentile.
