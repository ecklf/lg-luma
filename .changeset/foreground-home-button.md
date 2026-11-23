---
"lg-luma": patch
---

Handle the native foreground Home key in ordinary Luma launches as well as legacy Home takeover. Home returns directly from Apps, Settings or an overlay to Luma's Home tab, and stays there at the root, without requiring the Home Chooser launcher API.
