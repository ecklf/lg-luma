# Luma assets and references

- `assets/mark.svg`, `assets/app-icon.svg`, and the Midnight/Slate/Prism SVG wallpapers:
  original Luma artwork, MIT. webOS supplies the app icon panel using `iconColor`.
- `assets/wallpapers/dunes.jpg`: Unsplash photo `1509316785289-025f5b846b35`,
  https://images.unsplash.com/photo-1509316785289-025f5b846b35, [Unsplash License](https://unsplash.com/license).
- `assets/wallpapers/san-francisco.jpg`: Unsplash photo `1501594907352-04cda38ebc29`,
  https://images.unsplash.com/photo-1501594907352-04cda38ebc29, Unsplash License.
- `assets/wallpapers/summit.jpg`: Unsplash photo `1454496522488-7a8e488e8606`,
  https://images.unsplash.com/photo-1454496522488-7a8e488e8606, Unsplash License.
- `assets/wallpapers/forest.jpg`: Unsplash photo `1441974231531-c6227db76b6e`,
  https://images.unsplash.com/photo-1441974231531-c6227db76b6e, Unsplash License.
- `assets/wallpapers/yosemite.jpg`: [Tunnel View, Yosemite Valley, Yosemite NP](https://commons.wikimedia.org/wiki/File:Tunnel_View,_Yosemite_Valley,_Yosemite_NP_-_Diliff.jpg)
  by David Iliff (Diliff), [CC BY-SA 3.0](https://creativecommons.org/licenses/by-sa/3.0/).
  Resized to a 2560px JPEG; generated wallpapers/thumbnails retain the source license.
- `assets/wallpapers/northern-lights.jpg`: Unsplash photo `1531366936337-7c912a4589a7`,
  https://images.unsplash.com/photo-1531366936337-7c912a4589a7, Unsplash License.
- Inter variable font: Rasmus Andersson, SIL Open Font License 1.1, `assets/fonts/ofl.txt`.
- Noto Sans SC: copyright 2014–2021 Adobe, SIL OFL 1.1. Source/license in `assets/fonts/`;
  `tools/font-assets.mjs` pins the Google Fonts revision and checksums. Generated regular/medium
  subsets are renamed Luma CJK and retain copyright/license tables.
- Noto Sans KR: copyright 2014–2021 Adobe, SIL OFL 1.1 (same license text as Noto Sans SC,
  `assets/fonts/noto-ofl.txt`). Pinned the same way. Google Fonts ships Hangul only in this family,
  not in Noto Sans SC; generated Hangul-only regular/medium subsets are renamed Luma KR.
- Lucide interface icons: exported standalone SVG sources in `assets/ui-icons/`, ISC license
  bundled at `assets/licenses/lucide.txt`. PlayStation/Apple TV input paths were supplied by the
  user; the brand marks belong to Sony/Apple. Spotify/Kodi symbol treatments use the original
  native artwork. Logos identify third-party apps; Luma is unaffiliated with their owners.
- SDL-webOS 2.30.12 (`release-2.30.12-webos.5`, standard SDL ABI), zlib license.
  `tools/webos-runtime.mjs` records archive/library SHA-256. Notice at `assets/licenses/sdl2.txt`.

The bundled icon/logo SVG sources are compiled into derived `.lvec` triangles with the same
source licenses. Apple TV, Disney+, Prime Video and Das Erste retain the white display treatment.
YouTube's wordmark is white and its play mark red. Jellyfin retains its gradient and in-file
CC BY-SA attribution. Both licenses and this document travel inside the IPK.

Generated `.build/assets/thumbnails/` are cropped/resized derivatives of the same wallpaper
sources. Original sources remain in `assets/`; no artwork service is contacted on the TV.

## Brand SVGs

| Asset             | Source                                                                                                | License / attribution                                                                  |
| ----------------- | ----------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------- |
| `apple-tv.svg`    | [Apple TV](https://commons.wikimedia.org/wiki/File:Apple_TV_logo.svg)                                 | Apple Inc.; public-domain logo artwork                                                 |
| `prime-video.svg` | [Prime Video](https://commons.wikimedia.org/wiki/File:Amazon_Prime_Video_logo.svg)                    | Amazon; public-domain logo artwork                                                     |
| `disney-plus.svg` | [Disney+](https://commons.wikimedia.org/wiki/File:Disney%2B_logo.svg)                                 | Disney Enterprises; public-domain logo artwork                                         |
| `jellyfin.svg`    | [Jellyfin branding](https://github.com/jellyfin/jellyfin-ux/blob/master/branding/SVG/banner-dark.svg) | Jellyfin contributors; [CC BY-SA 4.0](https://creativecommons.org/licenses/by-sa/4.0/) |
| `netflix.svg`     | [Netflix](https://commons.wikimedia.org/wiki/File:Netflix_2015_logo.svg)                              | Netflix Inc.; public-domain logo artwork                                               |
| `youtube.svg`     | [YouTube](https://commons.wikimedia.org/wiki/File:YouTube_2024.svg)                                   | Google Inc.; public-domain logo artwork                                                |
| `arte.svg`        | [ARTE](https://commons.wikimedia.org/wiki/File:Arte_Logo_2026.svg)                                    | ARTE; public-domain logo artwork                                                       |
| `zdf.svg`         | [ZDF](https://commons.wikimedia.org/wiki/File:ZDF_logo.svg)                                           | ZDF; public-domain logo artwork                                                        |
| `das-erste.svg`   | [Das Erste](https://commons.wikimedia.org/wiki/File:Das_Erste_2014.svg)                               | ARD; public-domain logo artwork                                                        |

Installed-app PNG/JPEG artwork is collected from the owner's TV. Names and marks remain the
property of the respective app owners. Luma's folded-light L logo is original artwork; its SVG
geometry produces the small/large webOS installation icons.

Weather data: [Open-Meteo](https://open-meteo.com/), [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/).

The provided Apple design guidance informed focus, motion and materials. This is not an Apple
or LG product.
