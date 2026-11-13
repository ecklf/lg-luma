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
