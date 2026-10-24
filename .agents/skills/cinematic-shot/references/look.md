# Look recipes: readable cinematic UI

These are starting values, not overrides of the user brief. They describe the restrained look
approved during the Luma film iterations. Pixel values assume a **1920×1080 output**; multiply
blur radii, margins, and type sizes by `outputHeight / 1080`.

## Stage and light

| Product  | Stage                                  | Treatment                                                    |
| -------- | -------------------------------------- | ------------------------------------------------------------ |
| Dark UI  | True black outside the actual viewport | Preserve native wallpaper, chrome, translucency and colors   |
| Light UI | Neutral light ground, e.g. `#EAE7E2`   | Subtle separation/shadow if needed; no artificial dark theme |

Use broad neutral illumination. The real UI should retain contrast across the action area.
A faint vignette is optional; narrow pools, flags and large black masks are not the default.
No fixed mean-luminance or black-coverage target should make controls harder to read.

Do not add colors, reflections, browser/device frames, panel borders, glows, or grain simply
to make a frame feel cinematic. Keep source viewport edges distinct from accidental crop seams.

## Camera setups

Magnification is relative to the product's logical UI size. Angles are pitch / in-plane roll / yaw.
Perspective distances below are in logical pixels for a 1920-wide UI.

| Setup                        | Magnification                      | Pitch  | Roll         | Yaw   | Perspective                       |
| ---------------------------- | ---------------------------------- | ------ | ------------ | ----- | --------------------------------- |
| Frontal approach             | 0.60× → 0.99×                      | 0°     | 0°           | 0°    | Orthographic-looking/front-facing |
| Frontal browsing or modal    | About 0.99×                        | 0°     | 0°           | 0°    | Match the preceding shot          |
| Contextual settings          | About 1.08–1.12×                   | 24–26° | −5° to −4.5° | 4–7°  | About 4200                        |
| Angled configuration montage | About 0.94–0.96×                   | 18–22° | Around −3°   | 1–2°  | About 3600                        |
| Optional detail accent       | Only as tight as readability needs | 20–30° | Within ±6°   | Small | About 3600–4400                   |

The goal is a mild, legible perspective, not a 5–7× macro with extreme foreshortening. Entire
views are legitimate hero shots. Allow a front-facing opening; a separate Settings shot may
then introduce the diagonal. Don't flip the diagonal on every category/configuration change.

### Fast opening with overlapping action

```text
p(t)  = smoothstep(clamp(t / 0.8, 0, 1))
mag   = 0.60 + (0.99 - 0.60) * p(t)
pitch = roll = yaw = 0
```

When the flow calls for it, switch to the next real view at **t = 0.4 s**. The zoom continues
through that navigation. Keep the transform continuous and let the native selection animation
play. Source capture, not compositing substitute controls, supplies the new view.

### Base pose versus rotation animation

A roll of −3° gives the frame a diagonal. A change from −3° to −1.8° is only **1.2° total
animated rotation**. These are different parameters. If the user asks for less rotation,
reduce the animation delta while preserving an appropriate base pose.

For a calm montage, try:

```text
p     = clamp(timeInMontage / montageDuration, 0, 1)
mag   = 0.945 + 0.008 * sin(pi * p)
pitch = 20 + 0.6 * sin(pi * p)
roll  = -3 + 1.2 * p
yaw   = 1.5 - 0.6 * p
```

Use this one camera function for every configuration. Tiny translation is enough. A near-fixed
variant can reduce roll/yaw deltas to 0.2°/0.1°; larger orbits require a specific brief.
Re-timing a montage should preserve or reduce angular speed, not automatically make it spin.

### Framing both Settings columns

Choose the camera center, scale and anchor using the combined bounds of the left categories
and right options, including the lowest gallery row. Project their corners through the actual
homography at several times across the shot. Keep every important control inside the frame.

A slight horizontal anchor offset can balance the two columns. If the tighter crop excludes
the header clock, exclude it cleanly rather than leaving one clipped digit. A close-up that
loses the corresponding options is a framing failure, not a stronger hero shot.

## Blur

| Use                                                | Maximum Gaussian sigma at 1080p | Sharp region                                              |
| -------------------------------------------------- | ------------------------------- | --------------------------------------------------------- |
| Frontal/context view                               | About 1.25 px                   | Central 80% or more                                       |
| Closer Settings view                               | About 1.6 px                    | Categories and relevant options                           |
| Angled montage, optional peripheral depth softness | About 2.2 px                    | Broad region covering greeting, navigation and main cards |
| Brief blur-through edit                            | Peak around 4–6 px              | Settles rapidly to the next clear state                   |

For light edge softness, blend the original with a lightly blurred copy:

```text
r       = max(abs(x - width/2)/(width/2), abs(y - height/2)/(height/2))
edgeMix = smoothstep(clamp((r - 0.8) / 0.2, 0, 1))
output  = original * (1 - edgeMix) + gaussian(original, sigma) * edgeMix
```

For a tilted plane, optional depth softness can use a broad dead zone around the subject's
depth. Fit it to the whole useful subject. Heavy near/far bokeh, a narrow strip of sharp text,
and strong dimming of adjacent controls are opt-in, not defaults.

### Configuration transition

Use captured finished states on the **same projected plane**. A short blur-through match cut
avoids two greetings or two sets of logos appearing simultaneously:

```text
p       = smoothstep(clamp(timeSinceChange / transitionDuration, 0, 1))
state   = previous if p < 0.5 else next
sigma   = peakSigma * 4 * p * (1 - p)
output  = gaussian(project(state, continuousCamera(time)), sigma)
```

Typical transition duration is **0.05–0.12 s**. A very fast last look still needs clear output
frames. Keep film transitions separate from native UI animations: a wallpaper selection, tab
highlight, scroll, or carousel should use its actual rendering and timing.

## Timing and sequence

| Event                             | Starting recipe                                                 |
| --------------------------------- | --------------------------------------------------------------- |
| Frontal approach                  | About 0.8 s; next view can begin halfway through                |
| Full short Apps/list scroll       | About 0.9–1.2 s, smooth start/end, native viewport masks        |
| Browse a recent card              | About 0.4–0.7 s to register each app                            |
| Sidebar category                  | About 0.6–0.9 s, with both columns visible                      |
| A meaningful preference selection | Enough time to enter, select, confirm, and return to the parent |
| End card                          | About 2 s, crisp and stable                                     |

These are not universal limits. Native animation duration and the brief control the result.
Do not extend a short product demo to 45–55 seconds to satisfy a template.

### Accelerating configuration montage

For nine variants at 30 fps, a useful stagger is:

```text
holdFrames = [30, 27, 24, 21, 18, 15, 12, 9, 6]
holdSeconds = [1.0, 0.9, 0.8, 0.7, 0.6, 0.5, 0.4, 0.3, 0.2]
```

Derive state boundaries from cumulative frame counts. Shorten incoming transition durations
too, while preserving at least a few settled frames. Do not reset position/rotation per state.
Vary meaningful supported settings; record every preset. Use an explicit supported language
cycle with no adjacent repeats when requested, rather than arbitrary random language choices.

## Actual UI motion

Use native selection/pressed states, actual scroll bounds, real carousel positions and real
saved preference changes. Scrolling content must pass through the app's own header/footer
mask while fixed chrome stays fixed. Match a modal's parent after any edit-order change.

Native/browser phase capture can freeze animation values for deterministic frames. Do not
impose typing chips, stippled text, new buttons, simulated streaming, bounce, or synthetic
closing behavior on a product that doesn't implement them. No cursor or keycap overlays by
default. Browse-only recent-app shots must preserve their entries.

## Sound and end card

Default sound is **none**, with **no audio stream** in the file. Foley and music need a request.

Use one short end card, not a long copy sequence followed by another long logo hold:

- Exact approved copy, usually product name plus platform/short descriptor.
- Inter when available; otherwise the real product sans-serif. Verify the family is loaded.
- Medium weight, roughly 80–100 px at 1080p; restrained kerning/tracking.
- Optional original SVG app mark, roughly 80–110 px high, retaining its brand colors.
- Center the complete group; align the visible mark with the type's optical bounds.
- Hard cut or a very short entrance, then hold about two seconds. No mandatory unblur/glow.

Rasterizing an SVG as a video intermediate is fine; don't replace source vectors with exports.
Check the entire logo is present. In tools that coalesce resize operations, use a single explicit
viewBox/resize or separate intermediate buffers so a later resize doesn't accidentally crop it.

## Preview and HQ rendering

| Setting          | Preview                        | Approved HQ master                                   |
| ---------------- | ------------------------------ | ---------------------------------------------------- |
| Output           | 960×540                        | 1920×1080 unless otherwise requested                 |
| Frame rate       | Usually 30 fps                 | Preserve the approved rate and exact frame counts    |
| Shutter sampling | 2 samples, 180°                | Prefer 8 samples, 180°; at least 4                   |
| Encoding         | H.264 fast/CRF 18 is practical | H.264 slow/CRF 16, `yuv420p`, BT.709, fast-start MP4 |
| Audio            | No stream by default           | No stream by default                                 |
| Grain            | Off unless requested           | Off unless requested                                 |

Capture the actual product at a sufficient source resolution. A 5760×3240 native capture can
support a restrained 1920×1080 projection. Larger macros may need larger captures. Determine
source detail from the maximum projected pixel footprint, including perspective, not only a
nominal magnification or the preview's dimensions.

Build area-filtered source mips, then choose a mip with enough detail for the actual destination
resolution. Perspective resampling can use a high-quality filter such as cubic. Never reuse a
540p mip-selection rule blindly at 1080p, and never make the master by resizing the preview.
Apply blur at physical output scale; do not accidentally double its relative visual strength.

Temporal sample offsets for a deterministic 180° shutter are:

```text
offset(s) = (((s + 0.5) / sampleCount) - 0.5) / (2 * fps)
frame(n)  = mean(render((n + 0.5) / fps + offset(s)))
```

Native scroll samples must be dense enough for the fastest approved re-time. Stream frames
directly into per-shot encoders instead of writing enormous uncompressed frame sequences.
Re-use verified segments during preview edits; re-render every shot from adequate sources
for HQ. Keep the approved shot-order/timing manifest immutable during mastering.

Validate resolution, duration, frame count, codec, source detail, requested audio absence and a
full decode. Review full-size text/Unicode/logo frames. After copying the master to the requested
docs path, compare checksums. Keep the approved preview separately for comparison.

## Stills, contact sheets, and publication

- Compose around a whole readable subject with a real UI/card edge and intentional negative space.
- Use the same restrained camera/blur defaults. Dedicated still composition is useful, but a clean
  intentional film frame is a valid video poster when it already fits the brief.
- Pick the output format before framing: 1920×1080 hero, 1200×630 social card, 1080×1080 square,
  or 1080×1350 portrait are common. Keep key content within the central 80% for social crops.
- Render source/detail at adequate resolution, spatially supersample and downsample. No temporal
  blur in a dedicated still. Grain is optional, not a required final pass.
- Review contact sheets at lightweight size. Deliver PNG and a web format if requested.
- Contact sheets are review aids; don't put a tall sheet above the README by default. On request,
  publish the approved video inline with controls and put existing screenshots in a details toggle.
- Verify the destination renderer's video handling and actual source URL. Don't mistake a valid
  HTML tag for verified remote playback, or assume video HTML is always stripped.
