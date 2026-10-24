---
'lg-luma': patch
---

Stop the launch splash update failing when Luma is Home on webOS 5 and 6. The executable takeover method stages no Home app copy, so the update now leaves out the staged splash and only refreshes the normal tile's splash.
