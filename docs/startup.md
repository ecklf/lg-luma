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

The benchmark compares the original fallback and optimized path in the same executable,
without modifying assets or disabling glass. It writes `.build/startup-benchmark.json`.
The `--diagnostics` report retains first-frame statistics and cumulative startup stage
timings even though ordinary renderer statistics subsequently describe the latest frame.
It also records inventory completion and the first frame after a successful inventory,
separately from the potentially earlier loading frame.
The measurement ends after the first swap and excludes screenshots and background page
prewarming. It starts inside `run()`, not at OS process creation or the remote keypress.

The isolated startup smoke suite compares every framebuffer pixel on desktop GL and
GLES, verifies that prepared atlases avoid runtime filtering, and exercises missing and
corrupt atlas fallback. Its timings have no machine-dependent pass/fail threshold.
