# Luma development

## Layout

```text
Cargo.toml / Cargo.lock       Rust workspace and pinned dependencies
src/                         Rust launcher, platform adapter and GLES2 shaders
assets/                      Editable SVGs, fonts, wallpapers, translations and artwork metadata
vector-geometry/             Shared fill tessellation and vector boundary coverage
tools/vector-assets/         Host-only SVG compiler
tools/                       Asset preparation, SDK/runtime setup, packaging and release feed
tests/                       Desktop rendering and mocked HTTP smoke scripts
tv/autostart.sh              Optional cold-boot launch hook
tv/home.sh / home.cjs         Optional root-only native Home integration
dist/                        Generated ARM IPK, checksum and Homebrew feed
.build/                      Generated assets, SDK/runtime caches and rendering diagnostics
```

React, JSX, Vite, browser tests and web rendering are no longer part of the project. Node/pnpm
remain build/deployment tools for Sharp, the webOS CLI and formatting/linting. The installed app
is the Rust executable `luma`, with app ID `com.ecklf.luma`.

## Toolchain and commands

```sh
nix develop
pnpm install --frozen-lockfile
pnpm assets
pnpm dev
```

The flake supplies Rust 1.93, Cargo/rustfmt/Clippy, Node.js 24, pnpm 10.34.5, SDL2, pkg-config,
woff2, Python/fontTools, CMake and Git. Linux includes Mesa and Xvfb. Supported Nix hosts are
Apple Silicon macOS and ARM64/x86-64 Linux. Without Nix, supply those tools and Rust 1.88+,
including `armv7-unknown-linux-gnueabi` for TV builds. No browser installation is needed.

| Command                   | Purpose                                                    |
| ------------------------- | ---------------------------------------------------------- |
| `pnpm dev [--options]`    | Run the desktop preview                                    |
| `pnpm assets [--4k]`      | Prepare fonts, vector artwork and raster backgrounds       |
| `pnpm screenshots`        | Capture native Home, Apps, General and Inputs into `docs/` |
| `pnpm assets:vector`      | Recompile bundled SVG icon/logo meshes                     |
| `pnpm assets:fonts`       | Restore checksum-pinned Noto font source if needed         |
| `pnpm sdk`                | Download/relocate a pinned host webOS SDK                  |
| `pnpm runtime`            | Prepare checksum-pinned SDL-webOS                          |
| `pnpm build`              | Release ARM cross-build using `WEBOS_NDK`                  |
| `pnpm run package`        | Assets, cross-build, IPK verification and Homebrew feed    |
| `pnpm deploy --device tv` | Build/install/launch through an ares device profile        |
| `pnpm check`              | Oxlint and workspace Clippy, without executing tests       |
| `pnpm format:check`       | Rust and tooling/document formatting checks                |
| `pnpm test`               | Rust workspace unit tests                                  |
| `pnpm test:smoke`         | Desktop rendering, mocked weather and localization checks  |

The TV target is ARMv7 **soft-float calling convention**, `gnueabi`, not `gnueabihf`. Codegen
uses the NDK's Cortex-A9/VFP baseline with NEON disabled for Rust code. Hardware floating point
is enabled without changing the calling convention; the pinned compiler emits unstable-feature
warnings for those target switches. `src/webos-compat.rs` implements `getauxval` for the SDK's
older libc by reading real kernel auxiliary values, used by Rust startup and TLS CPU detection.

The NDK's SDL is a link-time stub with Wayland disabled. `tools/webos-runtime.mjs` instead pins
SDL-webOS `release-2.30.12-webos.5`, standard SDL ABI, with archive/library SHA-256 verification.
It is linked and packaged with runtime name `libSDL2-2.0.so.0`, loaded from `$ORIGIN/lib`.
Desktop pkg-config paths are isolated from the ARM build. `WEBOS_NATIVE_LIBS` can point to
explicitly selected ARM webOS shared libraries; SDL overrides must have a real webOS/Wayland
backend. Avoid copying the whole SDK library directory or the TV's graphics drivers/libc.

## Controls and navigation

### Documentation screenshots

`pnpm screenshots` generates `docs/home.png`, `docs/apps.png`, `docs/settings.png` and
`docs/inputs.png`, linked by the README. It builds the desktop executable without the TV feature,
uses isolated temporary preferences, sample inventory and a fixed date/time, and captures actual
1920×1080 OpenGL framebuffers. Weather is unconfigured, so no real weather APIs are contacted.
It does not execute the test suites. A desktop display is required; on headless Linux use
`xvfb-run -a pnpm screenshots`. These images document the native design, not TV hardware behavior.

### Input

| Input                     | Action                                                 |
| ------------------------- | ------------------------------------------------------ |
| Arrows                    | Move focus, revealing remote-focused content           |
| OK / Enter                | Activate on release                                    |
| Hold OK for 600 ms        | Reorder Home favorite or open app options              |
| OK or Back in move mode   | Finish reordering without launching                    |
| Escape / Back / Backspace | Parent navigation, overlay dismissal or quit from Home |
| Menu / M / right-click    | App options                                            |
| Pointer / wheel           | Hover, click, hold and scrolling                       |
| F5                        | Refresh inventory                                      |

Backspace deletes while editing. Native Back uses SDL's webOS-specific keycode. Back dismisses
the keyboard first; Weather Location then returns to its parent Location row, HDMI choices to
that HDMI row, and the Background gallery to Background in the sidebar. Back from the sidebar
focuses the Settings tab, then returns Home. General ends with a vector power-icon Quit app row.

Home favorites form one horizontal strip, never a second row. Left/Right follows saved order,
including reorder mode, and reveals complete cards. Up returns to Home navigation; Down enters
the nearest input in the first row. The strip position survives page changes. Wheel movement over
favorites scrolls horizontally; elsewhere it scrolls vertically. Pointer hover/down never moves
the clicked control. Apps Up/Down follows the six-column grid order, including offscreen rows,
and only the first row's Up route reaches the category toolbar. Scrolling Apps content fades into
the existing wallpaper at the header/footer, with one alpha mask for surfaces, shadows, rings,
vector icons/logos and text. The wallpaper/header are unmasked; no blur texture is needed.

TV editing uses the system keyboard through SDL-webOS. Desktop editing uses the custom panel.
System text events update app/location queries; Done/Back stops editing and restores focus.

## Splash images

`pnpm assets` renders one 1920×1080 PNG per background into `.build/assets/splash/` from the shaded
wallpapers. `tools/package.mjs` ships them under `splash/` plus `splash.png` (the default background),
which `appinfo.json` names as `splashBackground`, with `bgColor` set to the UI base colour so the
frame between the splash and the first draw is not black. With Home takeover enabled, `tv/home.cjs`
copies the selected background's splash into the staged Home app on every apply, reading the saved
preferences from the TV storage candidates in `storage.rs`.

## Vector rendering

`assets/ui-icons/` holds standalone SVGs exported from the original Lucide/control artwork,
including the supplied PlayStation/Apple TV marks. `assets/logos/` holds original bundled logos.
`tools/vector-assets/` parses SVGs with usvg, resolves stroke outlines and transforms, and uses
the shared lyon tessellator to emit `.lvec` geometry. The format contains positions, edge offsets,
vertex colors and coverage; holes, round strokes and the bundled gradients are preserved.
Gradient colors are adaptively sampled at build time. The TV uploads each mesh lazily once and
draws with a texture-free GLES2 shader. No SVG parser runs on the TV. The shared tessellator also
handles font outlines. Collected app PNG/JPEG artwork, wallpaper photos, tile-background images
and the webOS installation icon remain raster assets. See [assets.md](assets.md) for licenses.

Text reads the bundled TTF's advances, kerning and line metrics lazily. Only needed glyph outlines
are tessellated in font-em coordinates. Complete text runs are cached in static GPU buffers,
independent of position/color/drawable density, avoiding a draw call per letter. CJK fonts open
only when needed; there are no glyph bitmap atlases or full-font mesh exports.

The window requests 4× MSAA, with 2×/single-sample fallbacks if EGL rejects a configuration.
Multisampled text draws intact glyph fills without inward offsets/fringe triangles, which can
cross on very thin small-text strokes. Single-sample targets use a narrower half-pixel boundary
fringe. Glyph origins/baselines are pixel-aligned per drawable axis. There is no LCD subpixel
rendering, font hinting or complex-script shaping. Inspect `multisample_samples` and
`font_antialiasing`: offscreen captures can be single-sample while the actual window uses MSAA.

Glyph geometry has an 8 MiB/4096-entry CPU cache target and text runs a 32 MiB/512-entry GPU
target. Eviction preserves the current frame, which can temporarily exceed those targets.
Raster artwork has a 48 MiB cache budget. Diagnostics include geometry bytes, triangles, draw
calls and missing/failed artwork/glyphs. Idle screens draw on demand; VSync paces animation.

## Resolution and profiling

The shipping default requests 1920×1080 and retains a 1920×1080 logical layout. Drawable-aware
vector coverage follows the actual SDL framebuffer. `--resolution 4k` is an **experimental**
native surface request, not a guaranteed panel-output mode. The TV compositor can constrain or
rescale it. A 4K screenshot or drawable alone does not prove 1:1 panel presentation.

```sh
pnpm dev --resolution 4k --profile-frames 180 --diagnostics .build/profile.json
pnpm dev --script 'home,down' --frames 90 --capture-scale 2 --screenshot .build/home-4k.png
```

`--resolution` also accepts `1080p` or `WIDTHxHEIGHT`. Explicit TV requests use a borderless
window, avoiding SDL desktop-fullscreen overriding the dimensions. `--capture-scale 2` uses a
separate 3840×2160 offscreen target and leaves the actual window unchanged. Font/vector geometry
uses that target's physical coverage rather than enlarging a 1080p image. Optional `@2x` wallpaper
images need both `pnpm assets --4k` and `node tools/package.mjs --4k`; default packages exclude
them even if they remain in an old cache. Source photographic detail still limits sharpness.

`--profile-frames N` forces redraws and measures swap-to-swap intervals after ten warmup
presentations. Hidden windows pause profiling. Capture stalls are excluded. CPU submission is
not a GPU timestamp, and `--frames` counts event-loop iterations, not presentation rate.
Inspect `window`, `drawable`, `drawable_is_4k`, `last_presented`, `capture` and `profile` in JSON.
Do not infer 60 fps from a successful VSync request; measure actual TV pacing and transitions.

## TV startup diagnostics

SAM passes launch metadata as a JSON object in `argv[1]`. The CLI skips that object while leaving
the original process arguments available to SDL/libhelpers. To expose the actual developer-mode
sandbox's stdout/stderr, close Luma and run in the TV's root SSH shell:

```sh
APP=/media/developer/apps/usr/palm/applications/com.ecklf.luma
XDG_RUNTIME_DIR=/tmp/xdg RUST_BACKTRACE=1 /usr/bin/jailer \
  -t native_devmode -i com.ecklf.luma -p "$APP" \
  "$APP/luma" '{"@system_native_app":true,"nid":"com.ecklf.luma"}'
```

Startup prints accepted launch parameters, SDL driver, actual sample count, cumulative stage
timings, first-frame latency and vector-text cache size. Timings begin inside the executable and
exclude SAM/jailer process-launch overhead. The IPK is unpacked at install, not every launch.
A previous vector-font startup on the development TV presented its first frame in 399 ms;
new rendering paths still need their own hardware comparisons. SDL helper `getAppInfo` or
`getSystemSettings` registration calls can report permission warnings; they have not blocked
the verified interface/inventory runs. Normal launch/relaunch behavior should be checked too.

The native adapter registers LS2 connections from the executable and calls `LSCallOneReply`.
Anonymous names avoid collisions with SDL's registration; the hub still applies the executable's
role. `luna-send-pub -a` app impersonation is rejected on the development webOS 26 TV and is not
used. Native requests run on one worker's private GLib context with a 6.5-second budget and 2 MiB
reply limit. Errors include URI and the bus refusal. Fixed inventory commands run via elevated
Homebrew Channel `exec`, parsing complete multi-line `luna-send -n 1` JSON replies. Input-read
failures preserve the last valid inputs. No failed inventory read substitutes demo apps on TV.

### Optional native Home integration

`tv/home.sh` and `tv/home.cjs` are packaged beside `luma`. They are a root-only, explicit integration
for webOS 10/25 and 11/26. `enable` stages a native manifest with ID `com.webos.app.home` and
symlinks to the installed executable, libraries and assets under `/var/lib/luma-home/app`, then
bind-mounts it over `/usr/palm/applications/com.webos.app.home`. SAM rereads that one app through
`blockSystemApp` followed by `releaseBlockSystemApp`, including `forceBlockSystemApp: false` when
the TV schema requires it. The script verifies the reported manifest before stopping the previous
Home process. It attempts to restore LG Home if applying the mount or refreshing SAM fails.

Root-shell calls use `luna-send -n 1` on both firmware versions, with bounded stdout/stderr
capture and operation-specific diagnostics for empty or invalid replies. `-t 1` is timing mode:
it logs the payload to stderr and writes timing summaries to stdout, not a JSON response.

The `enable` shell entry point prints the change summary and requires a `y`/`n` confirmation
before creating the state directory, acquiring the lock or invoking the installer. Only `y`/`Y`
proceeds. `n`/`N` or end-of-input cancels, and other input repeats the prompt. The boot hook and
`apply` remain non-interactive, and `disable`/`restore` remain available without confirmation.

The native CLI accepts SAM's Home `nid` and gives SDL the matching Home surface identity. The
executable itself stays at its installed path for LS2 authentication, `$ORIGIN/lib`, asset discovery
and preference selection. Opening Luma's normal tile while the Home mount is active redirects to
the Home instance. The scripts do not create a browser wrapper or change the package's app ID.

`64-luma-home` reapplies the mount from a detached boot hook. The existing TV Home Auto Launch
and Quick Start+ settings determine whether power-on opens Home. Until the hook applies, LG Home
can still appear. This integration does not change those settings or install a remote-key filter.
`disable` (also `restore`) removes the mount, refreshes LG's manifest, stops the native Home
instance and removes the boot hook. The copied restore scripts remain in `/var/lib/luma-home`
even if the installed package is removed. Disable the standalone `65-luma` cold-boot hook before
enabling this integration.

Build and script checks cannot establish native Home compatibility. Manually verify Home, Back
out of the last app, standby wake with the TV's chosen Quick Start+ setting, a full cold boot,
saved preferences and restoring LG Home. The broader webOS 6+ app compatibility statement does
not establish Home-replacement support on older firmware.

## Manual extra-app preview

```sh
pnpm dev --test-extra-apps 120
```

On the TV, add `--test-extra-apps 120` after the JSON argument in the jailed command above.
This explicitly appends up to 300 labeled non-launchable cards to a successful real inventory,
using its apps as artwork/category templates. It uses `scroll-preview-preferences.json`,
initialized from normal choices once. Favoriting test cards exercises long Home strips without
changing normal favorites. Ctrl+C then a normal launch restores real inventory/preferences.
The placeholder cards are never a fallback for an inventory failure.

## Storage, weather and languages

TV preferences live in an app-owned persistent directory outside the installed executable's
payload. Startup probes `/media/developer/<appid>/`, `/media/developer/temp/<appid>/` and
`/media/internal/.<appid>/`, selecting a directory the jailed app can actually create and write.
Production `/media/cryptofs` installations prefer internal storage. `/media/developer/temp` is
on the persistent developer partition, not the reboot-cleared `/tmp`. No settings are placed
in `/tmp` and no system/app-payload permissions are widened. Existing JSON preferences and a
valid weather cache from beside this installation's old executable are imported once, without
overwriting the chosen destination. Invalid existing preferences still report an error.
Startup prints `Luma persistent storage:` with the chosen path. Desktop preview retains the legacy
`$XDG_CONFIG_HOME/luma-native/preferences.json` path, or `~/.config/luma-native/preferences.json`,
so existing preview choices survive flattening. `--preferences PATH` isolates a manual run.
Invalid preference files report an error rather than being overwritten. Writes use atomic
temporary-file renames; use one instance per preferences file. Browser local storage is not
migrated into native JSON preferences.

Weather Location searches cities/postcodes only on explicit Search. Forecasts run when Home is
visible, the widget enabled and a city configured. ureq/rustls uses embedded certificate roots,
an eight-second timeout, no redirects/proxy and 256 KiB reply limits. Search/forecast/cache writes
have separate workers and generation IDs, avoiding stale responses overwriting current state.
Fresh Celsius readings are reused for 15 minutes; valid saved data lasts up to six hours and
failure retries after five minutes. Unit changes convert unrounded cached Celsius immediately.
Removing a city clears its cache. The `.weather.json` sidecar uses Unix-seconds timestamps.
Mock servers accept only literal loopback HTTP origins and are refused in TV builds.

`assets/messages.json` supplies English/German/Spanish/Simplified Chinese, including locale formatting
and weather labels. App/device names and saved city names remain real data. No geolocation,
runtime font/translation services or telemetry are used. Weather attribution: Open-Meteo,
[CC BY 4.0](https://creativecommons.org/licenses/by/4.0/).

## Compatibility

Luma's native startup, rendering and app/input discovery have been verified on rooted webOS 26.
External testing confirms compatibility with webOS 6 and newer. On webOS 26 use elevated Homebrew Channel
0.8.0-rc1+ for `lunabus.query`. Root SSH execution does not establish the normal jailed caller
permissions; check TV-menu startup, launching/reopening and remote/pointer behavior manually.

[webOS Homebrew Can I Use](https://www.webosbrew.org/develop/caniuse) lists firmware library
versions. Its [library guide](https://www.webosbrew.org/develop/guides/library-version) maps
internal releases to marketing names: 7/22, 8/23, 9/24, 10/25, 11/26. SDL-webOS's newer
[`2.30.12-webos.6`](https://github.com/webosbrew/SDL-webOS/releases/tag/release-2.30.12-webos.6)
reports hotplug verification on webOS 3.4, 4 and 10. That is subsystem evidence for a newer
prerelease than the pinned `.5` library, not a full Luma compatibility guarantee.

## Checks and CI

Rust behavior tests cover navigation, preferences, localization, rendering geometry and weather
state without a TV. `tests/` scripts exercise actual desktop framebuffers and mock HTTP responses
for navigation, persistence, pointer input, translations/CJK, weather limits/cache/races and
profiling. They contact no live weather APIs or television. On Linux:

```sh
pnpm format:check
pnpm check
pnpm test
pnpm assets
xvfb-run -a pnpm test:smoke
```

CI uses this native-only flow, then cross-links with a pinned Linux SDK, inspects/normalizes the
actual ARM IPK and generates checksums/feed. It uploads release/rendering artifacts. Main release
publication retains the version/tag guards: a published version is skipped, a draft can be resumed,
and a tag pointing at another commit requires a new app version. Versions change only on request.
Host CI cannot prove hardware compatibility, small-text quality or presentation frame rate.

Homebrew Channel's details page reads `packages[].shortDescription` (or `fullDescriptionUrl`),
not `manifest.appDescription`. The feed generator writes the root app metadata description into
`shortDescription` and `manifest.appDescription`. It also copies `assets/homebrew-description.html`
to `dist/description.html` and sets the package's `fullDescriptionUrl` to that version's release
asset. Homebrew Channel prefers this HTML description when the URL is present. CI uploads the
HTML alongside the feed and IPK, so the feed and its description must be published together.

Key modules: `state.rs` focus/preferences, `view.rs` geometry, `renderer.rs` GPU caches/draws,
`font.rs` lazy metrics/outlines, `vector.rs` GPU mesh upload, `platform.rs` worker/inventory,
`luna.rs` LS2, `weather*.rs` scheduling/HTTPS/cache, `storage.rs` atomic JSON, and `main.rs` SDL loop.
