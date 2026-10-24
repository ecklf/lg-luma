# Shot-list examples: readable, fast, real UI

The first example records the approved Luma film after iteration. It is a worked example,
not a requirement to use a launcher, these apps, or these languages in unrelated products.
Use [look.md](look.md) for the reusable numeric recipes.

## Approved Luma film: 15.7 seconds

| Cut    | Length | Setup                                        | Actual action/result                                                                                           |
| ------ | ------ | -------------------------------------------- | -------------------------------------------------------------------------------------------------------------- |
| 0.0 s  | 2.1 s  | Frontal Home → Apps, 0.60× → 0.99×           | Zoom for 0.8 s; Apps begins at 0.4 s; scroll the complete SVG-only app list                                    |
| 2.1 s  | 2.2 s  | Same frontal pose, Recent Apps               | Browse Netflix → YouTube → Apple TV; retain all three entries                                                  |
| 4.3 s  | 4.0 s  | Closer angled Settings, both columns visible | Enter Background, choose Forest from Slate, return to the sidebar, continue through Weather and Inputs         |
| 8.3 s  | 5.4 s  | One continuous angled Home setup             | Nine finished configurations, faster with each change, small drift/rotation and brief blur-through transitions |
| 13.7 s | 2.0 s  | Crisp static end card                        | Original Luma mark plus `Luma for webOS` in Inter                                                              |

### Why these choices mattered

- The recognizable Home screen is already reasonably close at the start. Navigation overlaps
  the zoom, so the opening doesn't feel like waiting for an animation to finish.
- Recent Apps belongs immediately after the Apps scroll. Its native parent is the scrolled
  Apps page on Midnight. A previous capture from Settings/Forest would flash the wrong parent.
- The recent-app shot is browse-only. No app is removed, launched or replaced, and no fictional
  Close all button is introduced. Native IDs, focus and entry count confirm the interaction.
- The Settings category list and the entire right-hand gallery remain in frame. The capture
  shows a real preference save, not merely a focus ring over Forest. Later categories use Forest.
- Settings stops on Inputs. System remains an ordinary visible menu item, but it is never focused.
- The Home looks change directly without showing nine sets of configuration steps. They share
  one camera function instead of cutting between new angles or resetting rotation each time.
- The diagonal base pose remains visible, while its animated rotation is restrained. Light edge
  softness and small transition blur preserve readability instead of obscuring the interface.
- The silent file has no audio stream. A single two-second title/mark card finishes the short cut.

## Exact Luma configuration montage

Hold lengths at 30 fps are **30, 27, 24, 21, 18, 15, 12, 9, 6 frames**. These create the
requested stagger: 1.0 s down to 0.2 s. Incoming transition times also shorten. The first and
last _complete configurations_ were swapped, not the first and last Settings categories.

| Look | Background      | Language   | Name / weather      | Distinct content                                       |
| ---- | --------------- | ---------- | ------------------- | ------------------------------------------------------ |
| 1    | Prism           | English    | No name/weather     | Sports/streaming SVG cards; three inputs               |
| 2    | San Francisco   | German     | Flo / Munich        | Regional SVG favorites in a different order; one input |
| 3    | Northern Lights | Chinese    | 珞辰 / Shanghai     | Streaming SVG set; Apple TV and Switch inputs          |
| 4    | Monument Valley | Portuguese | Lucía / no weather  | Captions enabled; another favorite order               |
| 5    | Forest          | Italian    | Rafa / no weather   | Music/streaming SVG favorites; one input               |
| 6    | Yosemite        | English    | Giulia / no weather | Plex/Jellyfin streaming set; three inputs              |
| 7    | Slate           | German     | Sam / no weather    | Five SVG favorites; one media-player input             |
| 8    | Summit          | Chinese    | 珞辰 / Shanghai     | Asian streaming SVG set; one input                     |
| 9    | Midnight        | Portuguese | No name/weather     | Original favorite set; automatic inputs                |

Language order is **English → German → Chinese → Portuguese → Italian**, then repeat.
No adjacent looks use the same language. Every native background appears exactly once.
Names preserve the requested capital **F** in Flo and the exact Chinese characters **珞辰**.
City names remain the product's saved lookup results; surrounding UI and weather conditions
use the selected locale.

## Capture and verification lessons

1. **Use the renderer that exists.** Luma is Rust/SDL/OpenGL, so the film uses native 5760×3240
   captures, not a browser reconstruction. A separate capture worktree controls timing and phases.
2. **Exercise the existing interaction.** Capture the native carousel, wallpaper fade, viewport
   clipping and scroll masks. Preserve fixed chrome. Verify the final app row was reached.
3. **Use explicit fixtures.** Preferences are isolated; weather is supplied locally and recent
   history is a three-entry desktop fixture. This demonstrates UI behavior, not TV verification.
4. **Enforce SVG-only globally.** Check exact IDs against the native branding registry and require
   both SVG source and compiled vector asset. Filter fallback cards from Apps and scrolled rows
   as well as favorites and recents. Kodi/Moonlight-style fallbacks must not leak into a later look.
5. **Record the state assertions.** Verify saved background, focused category, every favorite order,
   input visibility, local weather reading, native translated text and retained recent IDs.
6. **Treat reordering as a context change.** Match the modal's background/parent/scroll and remove
   all frames that enter a skipped category. Re-encoding only the changed shots makes iteration fast.
7. **Master from adequate detail.** The approved HQ film is 1920×1080, 30 fps, 471 frames, with
   eight shutter samples, resolution-aware source filtering, H.264 slow/CRF 16 and no audio.
   Re-check the approved edit order/timing rather than upscaling the 960×540 preview.

## Adapt the shape to another product

Use this compact template when the next brief has different views:

| Section               | Treatment                                              | Verification                                                 |
| --------------------- | ------------------------------------------------------ | ------------------------------------------------------------ |
| Familiar landing view | Short frontal approach; next view may start mid-zoom   | Recognizable start; continuous transform                     |
| Browse/list view      | Native scrolling or a short meaningful interaction     | Fixed chrome; intended rows; correct real artwork            |
| Related surface/modal | Browse or act according to the brief                   | Correct parent context; expected retained/changed state      |
| Settings/detail view  | Restrained angle, necessary context visible            | Real save and subsequent result; requested stopping point    |
| Finished variants     | Continuous angled setup, optional accelerating cadence | Supported presets, exact variety coverage and language order |
| Product/platform card | Clean original mark/type, about two seconds            | Approved text/font, complete logo and exact hold             |

Skip sections the product or brief doesn't need. A typing/generation feature may justify a
longer process shot; a configuration showcase may need none. Do not invent a streaming reveal
or add setup steps just to fill a template. Actual product behavior determines the content.

## Review and delivery example

- Render a 960×540 preview at 30 fps with two shutter samples and a labeled contact sheet.
- Review every interaction peak, first/last scroll row, configuration, transition and title.
- On edits, record the latest order and use explicit preset IDs to avoid confusing categories
  with configurations. Keep the prior approved movie for comparison.
- After approval, inspect source dimensions and projected sampling requirements, render all
  shots at 1080p with eight samples, then verify frame counts and a complete decode.
- Promote the verified master to the requested docs video path and compare checksums. If the
  README uses inline HTML video, verify its renderer preserves the player and controls.
- Link a contact sheet publicly only if requested. Put existing screenshots behind a simple
  `<details><summary>Screenshots</summary>…</details>` toggle when that is the chosen presentation.

For stills, choose a whole readable subject and compose deliberately for its output format.
A clean film frame can be a useful video poster; a dedicated hero/social image may need a
separate composition. Neither needs compulsory heavy bokeh or a multi-image promotional set.
