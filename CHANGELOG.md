# lg-luma

## 2.7.0

### Minor Changes

- 9b38438: Support running in place of LG's Home executable on webOS 5, where a root integration bind-mounts Luma over `/usr/bin/com.webos.app.home`. Luma then registers with SAM itself through lifecycle v2, so Home launches bring the running instance back instead of restarting it. Production and developer installations are supported by pre-start library search paths and native package discovery that matches the mounted executable, keeping assets and splash updates tied to its source installation. Registration failures are reported at startup, a lost registration quits so SAM can start Home afresh, and a panic in a subscription handler no longer unwinds into LS2.
- c6dd60f: Support `home.sh enable` on webOS 5 and 6. There, SAM cannot reread a mounted Home manifest, so the takeover bind-mounts Luma's executable over LG's Home executable instead, keeps LG's manifest and role, and relies on Luma's own lifecycle registration. Enabling, the cold-boot hook, status and restoring LG Home work as on webOS 25/26; a failed launch restores LG Home, package updates are remounted, and a normal tile launch opens the running Home instance.

## 2.6.0

### Minor Changes

- e2b5531: Let the Home favourites row run to the screen edges like Apple TV's rows: tiles are slightly narrower (256 px) so six show in full while the seventh peeks in and fades out at the right edge, and once the row scrolls the left edge fades in. The visible edge fragments support pointer selection and horizontal wheel scrolling without moving clicked targets. A new horizontal edge fade joins the vertical one in the paint, vector and text shaders. The strip still ends after the sixth tile, so a focused tile never sits in the fade.

### Patch Changes

- 2ed5213: Decode every installed app icon in the background right after the app inventory loads, so the Apps page opens with its artwork ready. Prefetched icons are fed to the artwork worker a few at a time through their own list. Visible cards take priority over queued background work, including when a prefetched icon becomes visible, while request queues and decoded caches stay bounded.

## 2.5.0

### Minor Changes

- 1cc543f: Simplify Recents to bare app cards: remove app captions, close buttons and the empty-state subtitle. Close the focused app directly with Up. Make the overlay more transparent and add a restrained, reversible crossfade between the switcher and its parent screen, with immediate input restoration and reduced-motion support.
- 1cc543f: Add an Apple TV–inspired recent-app switcher, opened by holding Back for 600 ms or selecting the new header shortcut. Browse the TV's actual recent history, launch an app, close one card, or use LG's own Close all operation. Keep motion restrained, support reduced motion and pointer navigation, and load available recent-card artwork off the UI thread. Include a labeled, isolated desktop preview.
- 1cc543f: Open Recents with a short Back press on Home and keep long-Back access from other pages. Remove the header shortcut, card counter and instructions, and move a more compact Close all button to the top-right. Reuse the Apps card renderer for consistent vector logos, installed icons and surfaces, fixing mismatched YouTube artwork in Recents.

### Patch Changes

- e084e94: Remove the desktop preview label from the recent-app switcher.
- 1cc543f: Fix native remote Back dismissal and long-Back handling in the app switcher. Preserve SDL-webOS Back/Exit scancodes, claim the Magic Remote's long-Back Exit key, and consume dismissal presses through release so they cannot also quit Luma. Remove the switcher's heading and subtitle for a quieter card-only layout.

## 2.4.0

### Minor Changes

- df483bf: Add Italian (`it`) translation: full `messages.json` catalog, plus locale registration in `i18n.rs` (language list, display name, and localized date formatting).
- 395cdac: Add curated vector cards for NRK TV and TV 2 Play, with exact app-ID matching, card colours, and desktop demo support.

### Patch Changes

- 5b5c4a3: Recognize Apple Music's verified `com.apple.applemusic` app ID so the installed TV app uses its curated vector card.
  Match the Apple Music card's dark background gradient to Apple TV.

## 2.3.1

### Patch Changes

- db4fece: Fix Home takeover enable failing with "no app matched by pid" by closing the normal Luma instance through the app-ID endpoint. Allow takeover to continue if that instance already exited during the close request.
- db4fece: Update the normal app launch splash to match the saved background, including after changing it in Settings. Refresh the matching Home takeover splash as well.

## 2.3.0

### Minor Changes

- 3bf0ad6: Show a wallpaper splash at launch and match the saved background when staging the native Home integration.

## 2.2.2

### Patch Changes

- 072b7e9: Show installed app icons as soon as they are decoded instead of one per idle wake-up.
- fed795c: Keep decoded app icons across the automatic inventory refreshes at startup and whenever Luma becomes visible, reading an icon again only when its app's version or icon paths change. Refresh apps still reloads every icon.
- 969a6fd: Keep the neutral card colour for transparent app icons whose logo reaches the edges, instead of filling the card with the logo's own colour.

## 2.2.1

### Patch Changes

- d81cb66: Publish the changes from the unreleased 2.2.0 version after improving CI dependency installation and build timeouts.

## 2.2.0

### Minor Changes

- 201c030: Add an optional seconds display to the header clock, configurable with the remote in Clock format settings.
- e8f9085: Add 27 curated vector logos for streaming, music, and regional TV apps, including NOW / Now TV, plus dedicated Live TV and browser vector cards. Select branded cards by exact app IDs instead of keywords, so unrelated apps such as Amazon Alexa retain their installed artwork. Include every SVG-backed app in the desktop demo and add a rendered gallery preview.

  Remove dumped TV artwork, icon/colour maps, and their collection tools and tests. Desktop previews now use curated vectors or generic/category fallbacks, and builds no longer depend on collected icon fixtures.

- 589044b: Add custom input names: rename any input with the remote in Settings → Inputs, or reset it to the
  port name. Home cards, settings rows and the breadcrumb show the custom name, keeping the TV label,
  port and connection status as detail.
- 978b946: Discover app icons directly from installed TV applications instead of requiring collected artwork and icon-map updates. Load visible artwork in the background, derive matching card colours automatically, and refresh cached icons with app inventory. Exclude desktop-only collected icons and maps from TV packages.
- 9463b56: Add an optional name to the Home greeting, with remote-friendly name replacement and reset controls.
- e2c2a12: Add separate Home and Apps caption settings, with names hidden on Home and visible in Apps by default.
  Legacy caption settings do not override the new per-view defaults.
  Group greeting name editing under Greeting, use Clear name for greeting and input names, and reserve
  the bright settings-row highlight for the focused control.

  Group clock preferences under Clock, with separate nested Clock format and Time format selections.
  Time format offers HH:MM by default or HH:MM:SS while retaining existing saved seconds preferences.
  Move Refresh apps & inputs and Quit app into a dedicated System sidebar category.
  Keep the top-level Clock row free of format values and remove the chevron from input Rename actions.

### Patch Changes

- dcf1b03: Read the real HDMI cable state from `hdmiPlugIn` on TVs that report every port as `connected`, so
  inputs in Auto mode only appear when a device is actually plugged in.

## 2.1.0

### Minor Changes

- d417291: Add Brazilian Portuguese as a selectable interface language, including translated app labels,
  localized dates and weather location searches.

### Patch Changes

- 27802ac: Match brand keywords at ASCII word boundaries so unrelated apps such as dashboards retain their
  own artwork and Utilities category instead of being mistaken for ARD/Das Erste.
- 14b18f8: Render Korean app and input names using bundled Hangul font subsets loaded on demand. Add
  Korean-labelled LG Gallery and Room Connect apps to the desktop preview inventory.
- 3e256f6: Add installed-app artwork collected from an LG C1 for Korean-market apps, with matching card
  background colours. Make icon collection work on older webOS Node versions and follow the
  launcher's app visibility and exclusion rules.

  Include Coupang Play, Watcha Play, Wavve, Rainbow TV and LG TV VOD in the desktop demo inventory
  so their artwork can be previewed during development.

- ee2f6ff: Coordinate app releases through Changesets with synchronized Rust and webOS versions, publishing
  only after the checked Version Packages PR is merged.

  Add updating PR comments that report missing changesets and explain semantic version bumps for
  code changes.
