---
'lg-luma': minor
---

Show a loading screen during a webOS 5/6 cold boot instead of a dark screen. The Home integration's compositor key filter draws Luma's mark on black, with a spinner after 2 s, from its first seconds until an app is in front, any key is pressed or 90 s have passed. The mark is rendered from `tv/boot-logo.svg` when packaging.
