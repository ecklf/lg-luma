---
'lg-luma': patch
---

Retry the TV inventory while the TV service starts. Right after a cold boot or when the TV is short of memory, the Homebrew service that reads the app and input inventory can take longer than the 6.5 s Luna timeout. While no apps are shown yet, Luma now keeps loading and retries up to 5 times every 2 s before it shows the error and its Retry button.
