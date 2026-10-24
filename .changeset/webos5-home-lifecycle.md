---
'lg-luma': minor
---

Support running in place of LG's Home executable on webOS 5, where a root integration bind-mounts Luma over `/usr/bin/com.webos.app.home`. Luma then registers with SAM itself through lifecycle v2, so Home launches bring the running instance back instead of restarting it, and it resolves its assets, libraries and splash from the installed package. Registration failures are reported at startup, a lost registration quits so SAM can start Home afresh, and a panic in a subscription handler no longer unwinds into LS2.
