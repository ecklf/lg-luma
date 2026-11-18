# Startup performance

## Regression investigated

The 3.0.0 Liquid Glass redesign added a synchronous first-frame path that reopened the
wallpaper JPEG, resized it to 960×540, blurred it, and assembled a 960×1080 atlas before
presenting anything. This was additional to loading the wallpaper itself. It is a
measurable startup cost, not confirmation of the complete reported slowdown on TV.

Bundled wallpapers now ship with lossless precomputed atlases. New uploads prepare them
on the wallpaper worker. Both use the exact same Rust filter as the original rendering
path. Existing uploads without an atlas and missing or corrupt files keep that fallback.
There is no visual change, delayed glass transition, persistent GPU format, or new setting.

## Desktop measurement

Release build on a headless Linux desktop, five process starts per mode, alternating
modes, with a fixed date, disabled weather, and warm filesystem/driver caches:

| Median                              | Original runtime filtering | Prepared atlas |
| ----------------------------------- | -------------------------: | -------------: |
| Process entry to first presentation |                   289.3 ms |       202.5 ms |
| First-frame glass preparation       |                    95.4 ms |        15.3 ms |

This is about 30% less time to first presentation and 84% less glass preparation on this
desktop. These numbers are not TV hardware or cold-boot guarantees. No rooted TV was
contacted. SDL/context setup, font work, wallpaper decoding and platform inventory may
still dominate on a particular TV.

The tradeoff is disk space: the 18 generated shaded/unshaded atlases add about 14.3 MiB
to this 1080p asset tree. They replace work, not textures, so the existing GPU cache budget
and in-memory atlas dimensions do not increase.

## Reproduce

```sh
nix develop
pnpm assets
xvfb-run -a pnpm benchmark:startup
xvfb-run -a pnpm test:startup
```

For desktop-GL-only checks on macOS, where Cocoa cannot create a GLES context, use
`nix develop -c node tests/startup-smoke.mjs --desktop-only`. This explicit option does not
replace the default dual-profile CI coverage.

The benchmark compares the original fallback and optimized path in the same executable,
without modifying assets or disabling glass. It writes `.build/startup-benchmark.json`.
The `--diagnostics` report retains first-frame statistics and cumulative startup stage
timings even though ordinary renderer statistics subsequently describe the latest frame.
It also records inventory completion and the first frame after a successful inventory,
separately from the potentially earlier loading frame.
The measurement ends after the first swap and excludes screenshots and explicitly scripted
page prewarming. It starts inside `run()`, not at OS process creation or the remote keypress.

The isolated startup smoke suite compares every framebuffer pixel on desktop GL and
GLES, verifies that prepared atlases avoid runtime filtering, and exercises missing and
corrupt atlas fallback. In both drawer styles, it also checks that idle Home does not grow
hidden-page caches, while the explicit `prewarm` diagnostic action still works. Its timings have no
machine-dependent pass/fail threshold.

## Returning to Home

On webOS 25/26, the manifest Home integration must register for lifecycle events to resume
the existing process instead of restarting on each Home press. If a relaunch does not gain
focus within three seconds, Luma saves `/var/lib/luma-home/no-lifecycle` and falls back to
restarts to avoid leaving Home unresponsive. The marker now records the failing Luma version:
that version keeps the fallback, while a different release retries registration. Legacy empty
markers also allow a retry. A failed retry records the current version again; successful
registration does not remove the old marker. Malformed or unreadable markers retain the
fallback conservatively. This is handled by Luma itself, regardless of how the package was
updated; no updater hook or manual marker deletion is required for normal release changes.

On a maintainer's LG C5 running webOS 26 and Luma 3.6.1, moving aside an October 28 legacy
marker restored registration, same-PID Home relaunches and subjectively immediate returns.
That manual test confirms the stale-marker failure mode on that TV, not the automatic
migration above or compatibility on other firmware. Verify the automatic retry and failed-retry
fallback on hardware before claiming them as TV-tested.

Issue #269 reports a 1–2 second Home-key delay on a C1 running 3.2.1. Settled Home renders
its widgets, dock and, in tvOS style, a decorative first-row preview, but every inventory
refresh used to schedule full Apps and Settings preparation after 1.5 seconds of idle time.
That work included synchronous glyph outlining,
image decoding and GPU uploads on SDL's input/lifecycle thread. A visible but unfocused
window could also run it behind another app, delaying a Home event that arrived during it.

Hidden-page preparation is now demand-driven; only the explicit `prewarm` script action
opts into it for diagnostics. Installed icon decoding remains on its separate worker.
A system Home relaunch also snaps a previously open drawer closed before composing Home,
without changing ordinary navigation animations or the tvOS Home preview. Unit tests cover
partial/full drawer states in both drawer styles, with and without reduced motion.
The original idle-cache smoke regression failed against the old path
(11 → 54 cached glyphs on the local desktop); that is evidence of hidden work,
not a measurement of C1 Home-key latency.

TV verification is still required: compare repeated Home returns after launching from the
dock and drawer, as well as a fresh process start, with the same wallpaper and preferences.
The earlier glass-atlas optimization above addresses cold-start work separately; these
changes do not establish the full cause or timing of the reported hardware slowdown.
