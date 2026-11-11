---
'lg-luma': major
---

Consolidate the unpublished 2.19.0 and 2.20.0 changes into the requested 3.0.0 release, using the last published IPK version, 2.18.1, as the release baseline. This major version is an explicit release-line reset, not a claim of new breaking behavior.

Include the per-input SVG icon picker, Catalan localization, grouped System maintenance actions, and earlier cold-boot Home integration on webOS 5 and 6. Existing input icon choices default to Automatic and remain configurable under Settings → Widgets → Inputs → select input → Icon. The early Home helper is registered by `home.sh enable`, removed by `home.sh disable`, and retains the Homebrew cold-boot fallback.

Complete the Catalan input-icon translations and correct the update-card navigation regression test to follow Text rendering before Refresh apps & inputs.
