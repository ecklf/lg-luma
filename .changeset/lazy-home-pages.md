---
'lg-luma': patch
---

Keep hidden Apps and Settings rendering demand-driven instead of preparing them on the SDL thread after inventory refreshes, where they could delay Home relaunches and input. Return directly to Home without replaying the previous drawer's closing animation.
