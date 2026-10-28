---
"lg-luma": patch
---

Handle the remote Home key directly while Luma runs as the Home replacement, returning from Apps, Settings and overlays even when webOS does not send a lifecycle relaunch for an already-foreground Home. Normal app-tile launches retain system Home-key routing.
