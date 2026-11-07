---
"lg-luma": minor
---

Support Home takeover on webOS 24 (9.x). Its SAM has no `releaseBlockSystemApp`,
so a mounted Home manifest is never reread; webOS 24 therefore takes the same
executable method as webOS 5/6 instead of the manifest method used on 10.x/11.x.
