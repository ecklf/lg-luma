# Breezy Weather artwork

These PNGs are unmodified artwork from
[Breezy Weather](https://github.com/breezy-weather/breezy-weather), revision
`6602dfac0e30a2825ff9e90e2c10a4e518802459`, under **LGPL-3.0-only** with the
upstream additional terms in `LICENSE_ADDITIONAL`. See `COPYING.LESSER` and `COPYING`.
They are separate, replaceable image resources, not part of Luma's MIT-licensed code.
No app name, logo, or claim of affiliation is included.

Original sources are `app/src/main/res/drawable/weather_<name>.png` at that revision:

| Local filename          | Original name       |
| ----------------------- | ------------------- |
| sun.png                 | clear_day           |
| moon.png                | clear_night         |
| partly-cloudy.png       | partly_cloudy_day   |
| partly-cloudy-night.png | partly_cloudy_night |
| cloud.png               | cloudy              |
| fog.png                 | fog                 |
| drizzle.png, rain.png   | rain                |
| snow.png                | snow                |
| storm.png               | thunderstorm        |

Only filenames differ; source PNG bytes are preserved. `checksums.json` records their SHA-256
hashes. Luma uses static icons, not Breezy's animation layers. Drizzle and rain share upstream's
rain artwork; condition text still distinguishes them. Weather data remains from Open-Meteo.

## Additional selectable icon sets

Kuankuan is the default when no icon choice is saved. Explicit saved choices remain unchanged.
The picker calls Breezy Weather “Breezy”. Icon style, Show icon and location-name visibility
are independent per-widget settings. Turning Show icon off keeps the selected artwork.
Layout previews use all saved appearance settings.

- **Lucide:** `assets/ui-icons/weather-*.svg`, ISC with MIT Feather-derived icons. Full notices
  are in `assets/licenses/lucide.txt`.
- **Kickstand:** Copyright (c) 2013 KickstandApps, **SIL OFL 1.1**. Unmodified regular-weight
  sources in `kickstand/`, from [WeatherIcons](https://github.com/kickstandapps/WeatherIcons)
  revision `289e1fb1804496494addf945b9f0837999fde6f9`. `kickstand/LICENSE` accompanies the
  original SVGs and compiled meshes. Runtime white tint does not alter the source paths.
  Haze represents fog and Rain also represents drizzle.
- **Kuankuan:** kuankuan2007, **MPL-2.0**, from
  [weather-icon](https://github.com/kuankuan2007/weather-icon) revision
  `53a7a6459a7d0192f8381b604be8f02a0f674d84`. Original SVGs and preferred editable Illustrator
  sources are included in `kuankuan/` and `kuankuan/source/`, with `kuankuan/LICENSE`.
  `partly-cloudy-night.svg` is Luma's MPL-2.0 adaptation of `partly-cloudy.svg`, replacing the
  sun with an original silver-blue crescent. `moon.svg` is an original Luma companion under MIT.
  All other Kuankuan SVGs retain the original source bytes and palette.

`sources.json` maps renamed original files to immutable upstream URLs and SHA-256 checksums.
The corresponding editable source and any modifications are included in Luma's public repository
and package; MPL-2.0 applies to those assets, not unrelated Luma code. Both vector sets compile
to `ui-icons/weather-<set>-<condition>.lvec` at build time, without rasterizing the SVGs.
`node tools/weather-assets.mjs` optionally restores original files against the pinned checksums;
ordinary builds use checked-in files without downloading anything. It leaves Luma's night
companions intact. The full notices are also referenced in System → Attributions.
