---
'lg-luma': minor
---

Support running in place of LG's Home executable on webOS 5, where a root integration bind-mounts Luma over `/usr/bin/com.webos.app.home`. Luma then registers with SAM itself through lifecycle v2, so Home launches bring the running instance back instead of restarting it. Production and developer installations are supported by pre-start library search paths and native package discovery that matches the mounted executable, keeping assets and splash updates tied to its source installation. Registration failures are reported at startup, a lost registration quits so SAM can start Home afresh, and a panic in a subscription handler no longer unwinds into LS2.
