---
'lg-luma': minor
---

Let the Home favourites row run to the screen edges like Apple TV's rows: tiles are slightly narrower (256 px) so six show in full while the seventh peeks in and fades out at the right edge, and once the row scrolls the left edge fades in. The visible edge fragments support pointer selection and horizontal wheel scrolling without moving clicked targets. A new horizontal edge fade joins the vertical one in the paint, vector and text shaders. The strip still ends after the sixth tile, so a focused tile never sits in the fade.
