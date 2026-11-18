---
"lg-luma": patch
---

Faster startup on TVs: the first scene's images decode on worker threads while SDL, the window and the shaders start, and linked shader programs are reused between launches.
