---
"lg-luma": major
---

Use Home Chooser's launcher API for installed apps, HDMI input state and jailed icon reads through SDL/libhelpers' app-named connection, with API-level checks and existing Homebrew Channel read fallbacks. Keep legacy takeover, USB, recent-history and app-management behavior unchanged.

BREAKING CHANGE: Install Home Chooser and open it once before installing or updating Luma. This API-enabled release declares `org.webosbrew.homechooser.applications`, which webOS can reject until Home Chooser registers the permission. Home Chooser selection support in the preceding release does not require this permission.
