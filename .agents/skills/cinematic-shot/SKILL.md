---
name: cinematic-shot
description: Create readable cinematic product films and stills from real app, native, or website UI, with restrained angles and blur, fast interaction-led editing, and a preview-to-HQ workflow. Use when asked for launch videos, feature demos, product trailers, cinematic UI footage, hero images, key visuals, posters, or social cards.
---

# Cinematic shot

Shoot the real product as a readable object on a stage. Combine a short frontal approach,
restrained angled shots, meaningful UI actions, and a brisk edit. The product is the subject;
camera movement, blur, lighting, and sound must support understanding it.

This local edition incorporates the approved Luma film direction. The latest user brief and
approved cut take precedence over every numeric recipe here. Generalize the filming principles;
use Luma's exact app names, languages, backgrounds, and timings only for Luma work.

Read [look.md](references/look.md) for camera, blur, timing, and rendering recipes, and
[shot-list-example.md](references/shot-list-example.md) for the approved flow and reusable examples.

## 1. Define the story and the invariants

Before rendering, write a compact shot list with:

| Cut | Length | Real view/state | Camera | Action | Result to verify |
| --- | ------ | --------------- | ------ | ------ | ---------------- |

- **Flow:** the requested view order, interactions, and stopping points. Distinguish a category,
  configuration, background, and camera setup; don't apply a change to the wrong one.
- **Payoff:** the finished product view or configuration variety the film should demonstrate.
- **Demo content:** concrete, believable names, favorites, input labels, and locations. Preserve
  exact user-provided spelling, capitalization, and Unicode characters.
- **Constraints:** required artwork, language cycle, retained/deleted entries, end copy, audio,
  aspect ratio, and output destination. Record these alongside the shot list.

Post the plan briefly and continue working. Incorporate later messages into the active edit.
When the user revises a shot, preserve the other approved choices and compare against the
approved edit manifest before rendering the final master.

## 2. Capture the actual product

- Inspect the application first. A native SDL/OpenGL app needs its native renderer; a website
  needs browser capture. Do not recreate a native interface as a web mockup.
- Drive real navigation and state changes. A focused choice is not a saved preference. Verify
  the save and the subsequent views that depend on it.
- Browse, launch, and close are different actions. Browsing recents must retain their entries.
  Never invent a bulk-close button or animate a feature the product doesn't provide.
- Keep app artwork faithful. Prefer curated SVG/vector-backed cards where available and avoid
  generic placeholders in the demo. If SVG-only is requested, enforce it in every view,
  including scrolled rows and recents, using exact app-ID mappings and real asset existence.
- Use isolated preferences and deterministic local fixtures for inventory, weather, or other
  external data. Desktop samples do not establish TV/hardware behavior.
- Keep the product's layout, typography, colors, and supported languages. User-requested
  configuration changes are content; artistic restyling of the interface is not.

Make frames reproducible functions of shot/time. Record source IDs, preferences, actions,
focus, parent view, scroll offset, and expected state. Capture-only hooks belong in isolated
tooling or a worktree, with the application's actual drawing and state logic doing the work.

### Scrolling and native transitions

Capture the real scroll viewport and its clipping/fades. Moving an old full screenshot would
also move the navbar and footer. Keep fixed chrome fixed, verify the intended final row is
reached, and check all newly revealed artwork against the same constraints.

For modals and recents, match the preceding parent view, background, selection, and scroll.
Reordering shots may require new sources: opening recents before Settings must not briefly
reveal Settings or its changed wallpaper underneath.

For browser products, use the page's own animation states and a controlled clock where possible.
For native products, deterministic animation-phase capture is appropriate. Capture the actual
transition layers rather than painting substitute controls or doubling labels in a dissolve.

## 3. Camera: readable, restrained, motivated

### Opening

Start on the familiar landing view, front-facing and already large enough to recognize.
Default starting magnification is about **0.60×**, approaching **0.99× in 0.8 seconds**.
When appropriate, navigate to the next view **halfway through the approach**. Keep this setup
frontal through its main-view navigation instead of inserting an abrupt angle change.

### Angled interaction shots

Use a separate, gently oblique setup for settings or a related surface. Typical starting
values are **20–26° pitch**, **2–5° in-plane roll**, a little yaw, and a longer perspective.
Wide/context shots dominate; close-ups are selective accents, not a requirement.

Zoom only enough to improve the subject. With a left category list and right settings panel,
keep both visible, including the tallest gallery, throughout navigation. Check transformed
bounds, not just the center frame. Avoid clipped fragments of text at the edges.

Group related interactions into one continuous camera shot when requested. Category changes
and configuration changes are not reasons to reset the camera. Distinguish the **base angle**
from **animated rotation**: reducing rotation should make the shot calmer without flattening
the requested diagonal composition.

Use a small purposeful slide or push. Keep rotation amplitude low, usually **0.5–2° total**
across a montage; near-fixed angles are valid. Avoid large orbits, shake, and unnecessary flips
of the diagonal between states. Reducing shot duration must not accidentally increase spin.

## 4. Focus, stage, and lighting

- Default to **very slight edge softness** and a broad sharp region. Both the category names
  and relevant options should remain readable at preview size.
- Use approximately **1–2 px maximum Gaussian sigma at 1080p** for settled frame edges.
  A broad, mildly depth-softened peripheral plane may reach about **2.2 px**.
- Reserve stronger blur for brief transitions. Heavy bokeh and razor-thin focal bands are
  opt-in treatments, not the default definition of cinematic.
- Choose the stage from the real product theme: black outside a dark UI, or a neutral light
  stage around a light UI. Preserve the product's own wallpaper and translucency.
- Use broad neutral illumination and a faint vignette if useful. Don't hide navigation with
  a black spotlight mask or chase a fixed mean-luminance/black-area quota.
- Preserve original SVG geometry, fonts, and brand colors. No invented glows, accent rings,
  flares, fake device chrome, or mandatory grain.

## 5. Edit with momentum

There is no fixed 45–55-second runtime or minimum number of shots. A compact product film
often works in **15–25 seconds**; the requested actions determine the length.

- Start actions early. Typical category holds are **0.6–0.9 s**, and a short full-list scroll
  can take **0.9–1.2 s** if the resulting movement is smooth and the destination reads.
- In Settings, show a meaningful native action, such as selecting a wallpaper, then continue
  through the requested categories. Stop on the requested category instead of adding System,
  Quit, or other unrelated destinations.
- Browse a small recent-app carousel at a pace that lets each app register. Keep or remove
  entries according to the brief, and verify the final count and focus.
- Show finished configuration variants directly unless the user asks for setup steps. Vary
  real supported wallpapers, languages, names, artwork, favorite order, captions, and inputs.
- Avoid adjacent repeated languages. Follow an explicit cycle when supplied; never fabricate
  a language the app doesn't support. If one look per background is requested, verify coverage.
- Use one continuous angled setup for configuration variants. Optional accelerating holds
  such as **1.0, 0.9, 0.8, …, 0.2 s** create a staggered finish. Store durations as exact frames.
- Match cuts and short blur-through transitions are both valid. Change the real captured state
  at peak blur to avoid superimposed greetings, logos, and controls. Keep transitions short
  enough that even the fastest configuration has a settled frame.
- Use the product's actual timing and visual behavior for UI animations. Film edits between
  finished views should not be presented as new product functionality.

## 6. Sound and ending

**Silent by default.** Encode no audio stream. Add clicks, typing foley, music, or a soundtrack
only when explicitly requested; a silent AAC track is not the same as no audio stream.

Default to a single, crisp **two-second end card** with the exact approved product/platform
copy and, optionally, the original app mark. Use **Inter** if available, otherwise the product's
real sans-serif. Verify the font family, brand casing, complete logo geometry, and alignment.

Type around **80–100 px at 1080p**, medium weight, with restrained spacing. The logo may retain
its brand color. A hard cut or a very short entrance is enough. Separate long copy and logo
holds, blur-to-reveal taglines, glows, CTA buttons, and animated logo reveals are not defaults.

## 7. Preview, approve, render HQ

1. Capture enough source resolution for the final projection before composing the preview.
   Native 3× captures can be sufficient for a restrained 1080p film; calculate rather than
   blindly requiring 6× capture for every shot.
2. Render a **960×540, 30 fps preview**, normally with two 180-degree-shutter temporal samples.
3. Review action/result frames, scroll start/end, every configuration, transition peaks, and the
   title. Make a contact sheet and show the playable preview. Wait for approval before HQ.
4. Change only the affected shots during revisions. Preserve prior previews for comparison.
   Validate a reordered modal's parent context and a shortened shot's final target.
5. On approval, render **1920×1080 at the approved frame rate**, normally 30 fps, from the native
   sources. Use **eight temporal samples**, or at least four, and high-quality encoding such as
   H.264 `slow`/CRF 16, `yuv420p`, BT.709 output and fast-start MP4. Never upscale the preview.
6. Select source mips from projected pixel footprint and output resolution. A mip choice suitable
   at 540p may upscale text at 1080p. Preserve source detail and scale blur with output height.
7. Verify exact shot order, duration, frame count, resolution, audio-stream absence, and a full
   decode. Compare the HQ edit manifest with the approved preview, then review full-size frames.
8. Publish the approved media to the requested location. Update the README-linked copy only
   after verification; compare checksums when copying. Contact sheets are review assets and
   should be publicly linked only when requested. Existing screenshots can live in a toggle.

For inline README video, use an HTML video element with controls and a fallback link, and
verify the target Markdown renderer's handling. Don't assume either universal support or
universal stripping. Keep capture sources, fixtures, manifests, and reproduction commands
available without adding temporary render machinery to the shipping application.

## 8. Stills and posters

For still-only requests, choose the format and whole subject first. The same authentic UI,
restrained perspective, legibility, and light blur apply; heavy bokeh is optional here too.

- Compose a dedicated still when a hero image needs different framing. A clean film frame may
  also serve as a video poster when it already meets the brief; don't force an extra still set.
- Fit the entire subject inside the safe area, with all important text sharp and no half-visible
  neighbouring labels. Include a real panel/card edge where it helps the composition.
- Freeze a deliberate state: a finished preference, a readable result, or a native interaction
  that makes sense without motion. Avoid accidental mid-fade or half-switched labels.
- Render with high-resolution UI and spatial supersampling, then downsample. No fake motion
  streaks, imposed grain, or mandatory blur outside the subject.
- Deliver PNG, plus a web format if requested. Review a small contact sheet before a large still
  batch, and preserve native Unicode text and vector artwork exactly.

## Final quality gate

- The flow and actions match the latest brief, including browsing versus deleting and the final
  focused category. Real saved state agrees with what the screenshots show.
- The UI reads at half size; no active controls disappear into blur or darkness.
- Header/footer stay fixed during real scrolling; no offscreen fallback artwork leaks into view.
- Camera geometry is continuous within a shot. Base angle, animation amplitude, and tempo are
  separately controlled. A stagger accelerates the content changes, not the camera spin.
- Every requested background/configuration appears; language order, Unicode names, exact card
  identities, and SVG-only constraints hold across all relevant views.
- The end card uses the approved copy/font/logo and a short hold. Silent delivery has no audio.
- HQ preserves the approved edit and renders from adequate source detail. Decode and copy checks
  pass. Report the result clearly without claiming hardware behavior from desktop captures.
