---
'lg-luma': patch
---

Draw crisp shape edges. The paint shader's edge filter ran from 0.85 px inside a shape to its edge, so every edge had a half-covered pixel and one-pixel borders never became opaque. Coverage is now a one-pixel box filter centred on the edge, and rectangles and logo placement snap to whole drawable pixels, so straight edges stay a single step through a TV's 2x scale-up of its 1080p graphics plane.
