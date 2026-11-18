---
"lg-luma": patch
---

Smoother app drawer and Settings on single-core TV GPUs: shadows and borders skip pixels that cannot change, opaque faces let the GPU discard hidden fragments, the glass interior takes a shorter shader path, and redundant GL state changes are skipped.
