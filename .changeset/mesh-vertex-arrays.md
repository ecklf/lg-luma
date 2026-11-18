---
"lg-luma": patch
---

Less CPU per frame on TVs: logo, text and paint draws keep their attribute setup in vertex array objects, and card logos are drawn after all faces so the GPU program switches far less often.
