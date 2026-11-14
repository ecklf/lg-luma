---
"lg-luma": minor
---

Add reversible compositor Home routing for webOS 4, preserving remote mappings and returning to Luma when the foreground application closes. Deliver Back to focused Luma and use the older public Homebrew bus for recent-app, artwork and control requests, with the existing unified-bus path retained on newer TVs.

When the recent-history service is absent, populate Recent apps from the real running-card inventory and close apps through SAM. Keep inputs, the hidden Enyo container, Home, AirPlay and overlays running; preserve the newer history backend when available.
