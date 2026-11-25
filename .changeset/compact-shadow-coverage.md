---
"lg-luma": patch
---

Shorten the main-thread pause when Luma prepares a new shadow. GLES 3 TVs upload a single coverage channel for each shadow and reuse the existing white color when sampling it.
