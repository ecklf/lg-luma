---
'lg-luma': patch
---

Fix native remote Back dismissal and long-Back handling in the app switcher. Preserve SDL-webOS Back/Exit scancodes, claim the Magic Remote's long-Back Exit key, and consume dismissal presses through release so they cannot also quit Luma. Remove the switcher's heading and subtitle for a quieter card-only layout.
