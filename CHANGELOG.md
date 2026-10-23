# lg-luma

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
