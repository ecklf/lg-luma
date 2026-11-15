---
'lg-luma': minor
---

Add a grid-first Home widget manager with app-style Configure, Move and Remove menus, plus Reset to defaults and Add widget pills above the grid. Greeting combines weekday/date and a time-based greeting in three columns and one row, with the original 90.4px greeting and 18.4px date typography; Weather spans one column and two rows with top-aligned content. By default Greeting sits at the top left and Weather at the top right, spanning the first two rows. Valid saved custom placements are preserved. Preview fixed-size placements with the remote or pointer and reject overlaps or out-of-bounds positions. Removing frees cells while retaining configuration, and adding only saves after confirming placement. Reset restores default widgets and positions without clearing their settings. Configuration no longer has show/hide widget switches. Inputs moves to its own sidebar section below Apps and is not a widget. The app dock is unchanged; date visibility/format remain inside Greeting settings, without a separate Date widget or configurable name.

Keep the full weather-setup action readable in every language by wrapping long translations inside the fixed-size widget instead of truncating them.

Preserve the original weather temperature typography and tight spacing instead of shrinking the temperature independently of its layout.

Show the city before the weather status in Compact style, after the weather icon.
