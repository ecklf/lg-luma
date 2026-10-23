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
CC BY-SA attribution. Source licence notices and this document travel inside the IPK.

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

### Expanded streaming and music artwork

`tools/brand-assets.mjs` records each upstream URL and SHA-256. The SVGs are checked in, so
ordinary builds and TV rendering remain offline. `pnpm assets:brands` is an optional source
restoration command. The mesh compiler tightly fits these logos and applies white/dark-card ink
treatments while retaining selected brand colours. Peacock and Watcha have redundant rectangular
clips removed. Coupang Play's static SVG paths are extracted from its source component.

| SVG asset            | Source / attribution                                                                                                                                                                        | Licence                                         |
| -------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------- |
| `spotify.svg`        | [Spotify](https://commons.wikimedia.org/wiki/File:Spotify_logo_with_text.svg)                                                                                                               | Public domain logo                              |
| `plex.svg`           | [Plex via Simple Icons](https://simpleicons.org/?q=plex)                                                                                                                                    | CC0 contribution                                |
| `hbo-max.svg`        | [HBO Max 2025](<https://commons.wikimedia.org/wiki/File:HBO_Max_(2025).svg>), Warner Bros. Discovery, LucasJance2006 and Lee6597                                                            | Public domain logo                              |
| `paramount-plus.svg` | [Paramount+ via Simple Icons](https://simpleicons.org/?q=paramount)                                                                                                                         | CC0 contribution                                |
| `peacock.svg`        | [Peacock 2026](<https://commons.wikimedia.org/wiki/File:NBCUniversal_Peacock_Logo_(2026).svg>), NBCUniversal                                                                                | Public domain logo                              |
| `hulu.svg`           | [Hulu](<https://commons.wikimedia.org/wiki/File:Hulu_logo_(2018).svg>)                                                                                                                      | Public domain logo                              |
| `crunchyroll.svg`    | [Crunchyroll via Simple Icons](https://simpleicons.org/?q=crunchyroll)                                                                                                                      | CC0 contribution                                |
| `twitch.svg`         | [Twitch via Simple Icons](https://simpleicons.org/?q=twitch)                                                                                                                                | CC0 contribution                                |
| `dazn.svg`           | [DAZN via Simple Icons](https://simpleicons.org/?q=dazn)                                                                                                                                    | CC0 contribution                                |
| `rakuten-tv.svg`     | [Rakuten TV](https://commons.wikimedia.org/wiki/File:Rakuten_TV_logo.svg), Kashiwa Satō                                                                                                     | Public domain logo                              |
| `apple-music.svg`    | [Apple Music 2026 wordmark](<https://commons.wikimedia.org/wiki/File:Apple_Music_Logo_(2026).svg>), Apple Inc.                                                                              | Public domain logo                              |
| `amazon-music.svg`   | [Amazon Music](https://commons.wikimedia.org/wiki/File:Amazon_Music_logo.svg), Amazon.com LLC                                                                                               | Public domain logo                              |
| `bbc-iplayer.svg`    | [BBC iPlayer](<https://commons.wikimedia.org/wiki/File:BBC_iPlayer_2021_(Alt).svg>), BBC                                                                                                    | Public domain logo                              |
| `itvx.svg`           | [ITVX via Simple Icons](https://simpleicons.org/?q=itvx)                                                                                                                                    | CC0 contribution                                |
| `channel-4.svg`      | [Channel 4 via Simple Icons](https://simpleicons.org/?q=channel4)                                                                                                                           | CC0 contribution                                |
| `now-tv.svg`         | [NOW / Now TV via Simple Icons](https://simpleicons.org/?q=now), Sky                                                                                                                        | CC0 contribution                                |
| `rtl-plus.svg`       | [RTL+](https://commons.wikimedia.org/wiki/File:RTL%2B_Logo_2021.svg), RTL Group                                                                                                             | Public domain logo                              |
| `joyn.svg`           | [Joyn](https://commons.wikimedia.org/wiki/File:Joyn_logo.svg), Joyn GmbH, vector by Daniel FR                                                                                               | Public domain logo                              |
| `waipu-tv.svg`       | [waipu.tv](https://commons.wikimedia.org/wiki/File:Waipu_TV_Logo_2016.svg), Exaring AG                                                                                                      | Public domain logo                              |
| `zattoo.svg`         | [Zattoo](https://commons.wikimedia.org/wiki/File:Logo_Zattoo.svg)                                                                                                                           | Public domain logo                              |
| `skyshowtime.svg`    | [SkyShowtime](https://commons.wikimedia.org/wiki/File:SkyShowtime_Logo.svg), Comcast / Paramount                                                                                            | Public domain logo                              |
| `coupang-play.svg`   | Coupang Play, [SVG source preserved by SinJungho](https://github.com/SinJungho/Damoa_Team-Project/blob/db1955f0f6f09d9329518120afe42af375632de0/src/svg/CoupangPlayLogo.jsx)                | Coupang brand artwork, used to identify the app |
| `tving.svg`          | [TVING](https://commons.wikimedia.org/wiki/File:TVING_logo.svg)                                                                                                                             | Public domain logo                              |
| `wavve.svg`          | [Official Wavve artwork](https://www.wavve.com/static/element-icons-tag-ic-brand-wavve-b343dcb2.svg)                                                                                        | Wavve brand artwork, used to identify the app   |
| `watcha.svg`         | [Watcha](https://commons.wikimedia.org/wiki/File:Watcha_logo.svg), xixi pictures                                                                                                            | Public domain logo                              |
| `movistar-plus.svg`  | [Movistar Plus+](https://commons.wikimedia.org/wiki/File:Movistar_Plus%2B_2022_logo.svg)                                                                                                    | Public domain logo                              |
| `filmin.svg`         | Filmin, [SVG source preserved by DCueto](https://github.com/DCueto/filmin-svelte-clone/blob/36c977e31ec76abb42473b243584acbaed23579f/src/assets/filmin_logo.svg), originally Brandlogos.net | Filmin brand artwork, used to identify the app  |

Simple Icons contributions use [CC0](https://creativecommons.org/publicdomain/zero/1.0/), with
the notice bundled in `assets/licenses/simple-icons.txt`. Trademark and other underlying brand
rights remain with the respective owners. The source artwork's attribution and terms also apply
to compiled `.lvec` derivatives.

### Live TV and browser cards

`live-tv.svg` and `browser.svg` are original Luma artwork, MIT. Both share a blue/cyan material
with a light rim and white details. Live TV uses a rounded display, play symbol and live
indicator. Browser uses a globe with a navigation pointer. The gradients and highlights are vector
geometry, with no embedded images, runtime fonts, filters or raster effects. Each utility card
uses its native app's exact ID.

On TV, installed-app PNG/JPEG/WebP artwork is read from that TV's installed applications at runtime.
Desktop previews use curated vector logos or generic/category fallbacks, with no dumped TV artwork
or collected icon maps in the repository. Names and marks remain the property of the respective
app owners. Luma's folded-light L logo is original artwork; its SVG geometry produces the
small/large webOS installation icons.

Weather data: [Open-Meteo](https://open-meteo.com/), [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/).

The provided Apple design guidance informed focus, motion and materials. This is not an Apple
or LG product.
