---
"lg-luma": major
---

Enable native relaunch handling for Luma selected through Home Chooser, suppress the native splash, and document migration from the standalone takeover scripts. Home Chooser manages stock-Home suppression; Luma retains its existing Homebrew Channel platform integration.

BREAKING CHANGE: Luma now declares the Home Chooser applications API permission. Install Home Chooser and open it once before installing or updating Luma; otherwise webOS may reject installation with `Not allowed permission is used`. Runtime use of the API is not yet implemented.
