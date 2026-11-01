---
"lg-luma": patch
---

Read and close running apps directly from the rooted Home worker, avoiding repeated Homebrew and JavaScript startup delays. Preserve protected-app and process-identity checks.

Ignore exited processes and show automatically preloaded processes only after a real foreground use. Keep LG's preload policy intact and treat an already-stopped application as successfully closed.
