# lg-luma

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
