# Luma agent context

## Project

Luma is a native Rust/SDL2/OpenGL ES 2 launcher for rooted LG webOS TVs. Rust source is in
`src/`, original assets/translations in `assets/`, shared vector geometry in `vector-geometry/`,
the host SVG compiler in `tools/vector-assets/`, packaging/tooling in `tools/`, and desktop smoke
scripts in `tests/`. There is no React/JSX/browser application. Read `development.md` for details.

Use pnpm, Oxlint and Oxfmt for build tools; Cargo/rustfmt/Clippy for Rust. `nix develop` supplies
pinned Rust 1.93, ARMv7 std, Node 24, pnpm 10.34.5, SDL2, woff2 and Python/fontTools. The webOS
SDK is separate: `pnpm sdk`, then set `WEBOS_NDK` to `.build/toolchain/arm-webos-linux-gnueabi_sdk-buildroot`.

```sh
pnpm install --frozen-lockfile
pnpm format:check
pnpm check
pnpm test
pnpm assets
pnpm test:smoke
pnpm run package
```

`pnpm check` does not execute tests. `pnpm test` is workspace unit tests; smoke scripts render
desktop OpenGL and mock weather HTTP. Linux can use `xvfb-run -a pnpm test:smoke`. Tests must not
contact real weather APIs or a TV. Respect any user instruction to use manual TV verification
instead of running automated tests. Builds/lint may still be run. Do not claim hardware behavior
from desktop checks. Add a `pnpm changeset` file for release-worthy changes and leave version
bumps to the automated Version Packages PR. Its `pnpm version:release` command synchronizes
`package.json`, `appinfo.json`, the root `Cargo.toml`, the `luma` entry in `Cargo.lock` and README
IPK examples. Supporting Rust crates retain independent versions. `pnpm version:check` checks
consistency without writing files. Code or behavior changes should use semantic versioning:
patch for backward-compatible fixes, minor for backward-compatible features, and major for
breaking changes. Documentation-only or maintenance changes that do not need a release can omit
a changeset.

## Build and identity

`com.ecklf.luma` is the native app's ID and `luma` its executable. Root `appinfo.json` is the
package metadata source. It installs separately from the former web app `com.luma.launcher` and
prototypes `com.luma.launcher.native` and `com.luma.native`, all excluded from app inventory. Preferences are local JSON
per installation; browser local storage and preferences from other IDs are not migrated.

ARM builds must use the NDK's gnueabi soft-float calling convention and explicit sysroot, with
isolated pkg-config. Preserve the Cortex-A9/VFP Rust flags and `getauxval` compatibility.
The SDK's SDL has dummy video: use checksum-pinned SDL-webOS with standard SDL ABI and a real
webOS/Wayland backend. Do not use rust-sdl2's generic bundled SDL or ship host libraries. Native
packages use `Architecture: arm`, preserve executable mode and include licenses/checksums/feed.

## Runtime constraints

Keep input/rendering on the SDL thread and TV/HTTP work off it. Luna requests originate from the
native process, not `luna-send-pub -a` impersonation. Inventory failure must never substitute demo
data on TV. Explicit `--test-extra-apps` is a labeled manual preview applied only after successful
inventory reads, with separate preferences and no placeholder launch requests.

Pointer hover/down must not move a clicked control. Home favorites use one horizontal strip;
remote focus/reordering reveals the whole selected card. Apps Up/Down follows six-column grid
indices, including offscreen rows, so headers cannot steal mid-grid navigation. Back restores
setting subpage parents and supports SDL-webOS Back. OK finishes move mode without launching.

Vector icon/logo assets are standalone SVGs compiled to `.lvec` at build time. Font outlines
are tessellated only as needed; text runs use bounded GPU caches, lazy CJK and no glyph bitmap
atlas. Text AA prefers MSAA of intact outlines, with a narrow fringe fallback; keep physical
origin/baseline snapping and logical 1920×1080 layout. `--resolution 4k` is experimental, not a
shipping-default claim. Profiles count presentations; ordinary `--frames` is event-loop iterations.

Weather uses ureq/rustls, fixed Open-Meteo origins, bounded replies/timeouts, no proxy/redirects,
separate workers/cache and generation IDs. Cache Celsius and convert units without replacing it.
Only configured/enabled/visible Home starts forecasts. Shared translations are bundled locally.
Match the current native design without on-screen weather credits; attribution remains in docs.

## Assets

SVG sources and fonts retain licenses in `assets/`. TV app artwork is resolved at runtime from
inventory/installed manifests by `src/app_artwork.rs`, with an embedded Node 8-compatible elevated
reader for jailed installations. Reads/decoding stay on its worker; GPU uploads stay on SDL.
Refresh invalidates cached results. No collected icon map or card-colour JSON is used or shipped.
Desktop previews use curated vectors and generic/category fallbacks. Do not add dumped TV images
or per-TV icon maps to the repository.
Do not replace vector logos with PNG exports.
Curated vectors use the exact app-ID allowlist in `src/branding.rs`, never title/substring matching.
Every logo in `assets/logos/` must have a desktop demo app. `pnpm test:brands` renders the full
gallery to `.build/brands-gallery.png`; source restoration via `pnpm assets:brands` is optional.

## CI and releases

`.github/workflows/ci.yml` is native-only: format/lint/workspace tests, asset generation and
mocked desktop rendering, SDK setup, ARM cross-build, actual IPK verification/checksum/feed,
artifacts and guarded publication. Keep existing published-version/draft/tag-commit checks.
`.github/workflows/changeset-status.yml` maintains an informational changeset status comment on
PRs, including forks, with semantic-version guidance for code changes. It skips the generated
version PR. Its `pull_request_target` jobs must not install dependencies or run code from the PR.
`.github/workflows/release-pr.yml` maintains the Changesets version PR and dispatches its CI
checks with the built-in token. Only a merged `changeset-release/main` PR with a version increase
can publish after the main-branch CI build passes. Ordinary PRs, other merges and manual CI runs
still build/check/package but do not publish. See `development.md#releases` for setup and retries.
Only create commits or PRs when explicitly requested. For PR descriptions follow the global
`commands/pr.md` format instructions.
