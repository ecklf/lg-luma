---
'lg-luma': patch
---

Render crisper text and logos on the TV. The webOS window is created without MSAA so text and logos use their analytic edge filters, which keep stems sharper through the panel's 2x scale-up of a 1080p graphics plane than 4x sample coverage, at the same cost. Small text is emboldened by 0.25 physical pixels per edge so thin stems get an opaque core. Desktop previews keep multisampling.
