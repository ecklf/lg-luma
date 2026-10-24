---
'lg-luma': patch
---

Decode every installed app icon in the background right after the app inventory loads, so the Apps page opens with its artwork ready. Prefetched icons are fed to the artwork worker a few at a time through their own list, keeping the bounded request queue free for the cards on screen.
