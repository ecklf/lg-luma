---
"lg-luma": patch
---

Retry Home lifecycle registration after a release change or an old empty fallback marker, so an earlier focus failure does not keep forcing slow full restarts after updates. If a retry still fails to regain focus, preserve the restart safety fallback for that Luma version.
