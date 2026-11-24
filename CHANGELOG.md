# lg-luma

## 3.13.3

### Patch Changes

- f8f7eba: Restore favorite focus on Home returns, reorder General settings, clarify drawer labels and icons, and simplify the Attributions action. Refresh inputs on native EIM notifications and preserve polling deadlines across interactions.

## 3.13.2

### Patch Changes

- b3f7115: Handle native app lifecycle registration explicitly for ordinary and Home Chooser launches, returning to the Home scene on relaunch while preserving legacy LG Home takeover paths.
- b7aec89: Prevent uninstalling Home Chooser through Luma or its privileged helper.
- 9f7da39: Fix the native startup black screen caused by duplicate lifecycle bus-name registration by sharing SDL/libhelpers' existing app connection.

## 3.13.1

### Patch Changes

- ea94e4d: Switch to Settings → Apps faster on webOS TVs: draws entirely outside the current clip (rows scrolled out of a list) are no longer submitted, and the app cards there are grouped like the drawer's.
- d0149ba: Handle the native foreground Home key in ordinary Luma launches as well as legacy Home takeover. Home returns directly from Apps, Settings or an overlay to Luma's Home tab, and stays there at the root, without requiring the Home Chooser launcher API.
- 5866c28: Open the app options menu and Settings subpages without a first-frame stutter on webOS TVs: their labels are prepared while the launcher is idle. When a shadow finishes its idle preparation, only that shadow is redrawn instead of every shadow on screen, and drawer logos load before those of the Settings sections.

## 3.13.0

### Minor Changes

- 3ff03c5: Enable native relaunch handling for Luma selected through Home Chooser, suppress the native splash, and document migration from the standalone takeover scripts. Home Chooser manages stock-Home suppression; Luma retains its existing Homebrew Channel platform integration.

  Home Chooser remains optional. Luma does not declare its applications API permission or require it for installation.

### Patch Changes

- 69eae29: Require explicit confirmation before an in-app upgrade from Luma v1, v2 or v3 to v4 or later. Default to Close and explain the Home Chooser migration steps before downloading.

## 3.12.1

### Patch Changes

- 3085234: Default unconfigured weather widgets to the One line layout below Greeting without changing saved styles or positions. Retry weather more quickly when the network is unavailable during startup, using bounded backoff until the first reading succeeds.

## 3.12.0

### Minor Changes

- 4f6a4ad: Add per-widget location-name visibility, a Show icon toggle and weather icon style settings. Default to Kuankuan, with Breezy, Lucide and Kickstand alternatives, preserving saved choices and reflecting appearance settings in layout previews. Add a read-only, scrollable System → Attributions page and bundle the new artwork's licenses and editable sources.

## 3.11.1

### Patch Changes

- 65dfa47: Center the app move arrows inside their indicator badge.

## 3.11.0

### Minor Changes

- 4e2617a: Wrap left/right navigation between the first and last app in favorites and the app drawer by default, without a setting.

### Patch Changes

- af1da29: Shorten the main-thread pause when Luma prepares a new shadow. GLES 3 TVs upload a single coverage channel for each shadow and reuse the existing white color when sampling it.

## 3.10.1

### Patch Changes

- 8b23549: Restore bright glass settings highlights and apply text tint directly in the fragment shader, with contrast coverage for popup actions as well as settings.

## 3.10.0

### Minor Changes

- 8e73bc6: Show USB storage reported by webOS in Inputs, with naming and visibility controls. Selecting storage rechecks its presence and opens an installed LG media browser rather than treating USB as an HDMI source. TV discovery and media opening still need hardware verification across firmware versions.

### Patch Changes

- 867cd5b: Keep focused settings labels readable with an opaque dark fill, light text and icons, and a clear white outline instead of a bright white focus fill.

## 3.9.0

### Minor Changes

- cbd2d76: Center the Home dock and size it to fit when it contains one to five apps. App cards keep their usual size and spacing.

### Patch Changes

- cbd2d76: Keep the selected Home dock tile and widgets in place when pressing Down would open an empty app drawer.

## 3.8.3

### Patch Changes

- 48bc9e0: Bundle only the Midnight wallpaper and make it the default. Existing selections of removed bundled wallpapers fall back to Midnight while uploaded backgrounds and rotation remain supported.
- a62a16b: Remove the obsolete wallpaper WebAssembly runtime and foreground PNGs from cached assets and native packages.
- 50cda86: Replace wallpaper-matched launch splashes with one dark-gray splash featuring the centered Luma logo. Remove dynamic splash updates and per-wallpaper splash assets.

## 3.8.2

### Patch Changes

- 4f6f39a: Smoother first opening of Settings and its sections on TVs: their images, logos and text are prepared in small steps while Home is idle.
- 297acdc: Less CPU per frame on TVs: logo, text and paint draws keep their attribute setup in vertex array objects, and card logos are drawn after all faces so the GPU program switches far less often.
- 279ee57: Add an opt-in native paint shader path for faster drawer animations. Set `LUMA_PAINT_SHADERS=1` to enable it.
- 07bd338: Reject malformed decoded-image caches and reload the source image, preserving Liquid Glass atlas dimensions and bounding cache allocations.
- a681a4e: Faster TV startup: the dock's surfaces decode on the startup workers, and large decoded startup images are reused between launches.

## 3.8.1

### Patch Changes

- b22b4fc: Pace animations when VSync is unavailable so headless rendering and scripted captures allow exit animations to finish.

## 3.8.0

### Minor Changes

- bd418ae: Add SDL game controller navigation, including PS5 DualSense where recognized by the TV backend, with D-pad/left-stick navigation, OK and Back holds, Options, and hot-plug handling.

### Patch Changes

- 7de7cfc: Cheaper drawer shadows and fewer draw calls on TVs: shadows are drawn from precomputed coverage textures prepared while idle, and card shadows and borders share GPU batches.

## 3.7.0

### Minor Changes

- d9511c1: Add a Show app names setting under General, off by default. Enable it for selected drawer app labels, or leave it off to retain the original compact spacing and scroll padding.
- a21485b: Show the selected app's name beneath its card in the app drawer, with more space between rows and without adding labels to favorites. Keep the last row's name visible with bottom padding when scrolling.

### Patch Changes

- 83b3bb5: Faster startup on TVs: the first scene's images decode on worker threads while SDL, the window and the shaders start, and linked shader programs are reused between launches.
- 81dcdf4: Smoother app drawer and Settings on single-core TV GPUs: shadows and borders skip pixels that cannot change, opaque faces let the GPU discard hidden fragments, the glass interior takes a shorter shader path, and redundant GL state changes are skipped.
- a288544: Smoother first app drawer opening on TVs: bundled surfaces decode in the background and logos load while Home is idle. Shared surface paths decode only once, with bounded buffering while Luma is hidden or animating.
- b5f33fe: Stop showing the Home Opening toast when launching an app or Device settings on TV, while keeping desktop preview notices.
- 53ca20c: Ask for the app inventory once per Home press instead of twice.
- 2fda3d7: Round font sizes down to whole physical pixels before text layout and outline scaling, preserving fitted text bounds. Add a diagnostic override for comparing the existing fractional sizes; TV sharpness still needs matched hardware captures.

## 3.6.2

### Patch Changes

- 9664872: Retry Home lifecycle registration after a release change or an old empty fallback marker, so an earlier focus failure does not keep forcing slow full restarts after updates. If a retry still fails to regain focus, preserve the restart safety fallback for that Luma version.

## 3.6.1

### Patch Changes

- a137e43: Keep Home-replacement preload launches hidden instead of interrupting the active app or HDMI input. Ordinary Home relaunches continue to show and raise Luma.

## 3.6.0

### Minor Changes

- 0bd2ad1: Restore the locally saved greeting name, with Name and Clear name controls in the Greeting widget settings and personalized greetings throughout the day.

### Patch Changes

- f87b78c: Keep hidden Apps and Settings rendering demand-driven instead of preparing them on the SDL thread after inventory refreshes, where they could delay Home relaunches and input. Return directly to Home without replaying the previous drawer's closing animation.

## 3.5.1

### Patch Changes

- 2ac0eda: Keep the Home favourites inside their glass card. The cut-off neighbouring tile no longer shows beside the card, and the space beside it no longer takes clicks or wheel input; the app drawer's peek below now shows that more apps follow.
- 706b19b: Show live stock and crypto symbol edits with the TV keyboard, and accept Yahoo Finance index symbols such as ^GSPC.

## 3.5.0

### Minor Changes

- 82e72bb: Add optional 1×1 Crypto and Stocks widgets with configurable symbols, crypto quote currency, key-free background price updates, daily percentage changes with colored arrows and relative quote ages. Provider attribution and stock-delay guidance remain in documentation rather than widget labels.
- 0526c11: Optionally open Luma instead of the last TV input at power-on and after Back/Exit on webOS 5 and 6. Ordinary Home takeover preserves the TV input policy; `home.sh startup-home on` explicitly enables the read-only last-input override and Back/Exit filter. `startup-home off` or disabling takeover restores the saved HDMI/Live TV input. Restoration parses JSON independently of key order and reloads the live filter before returning control to LG.
- 0eadcab: Add a Recent apps setting for short press on Home, long press, or Off, preserving long press as the default.
- 0f7d2e3: Add a small 1×1 weather widget size with temperature, condition icon and location, including placement at grid edges and safe resizing.
  Change the size directly from Weather's widget popup or its settings page.
- e041e17: Support multiple independently configured Weather, Crypto and Stocks widgets. Keep the move preview stationary with compact arrow, OK and Cancel controls, clarify unconfigured market widgets, simplify market guidance into an info card, and use a nested Recent apps picker.
- c8935b5: Add a saved General → App drawer setting with default tvOS styling and a non-interactive app-row peek, or a Minimal layout with a lower dock and no peek. Both styles keep the same drawer navigation.

### Patch Changes

- 0f7d2e3: Remove the Live TV icon's stand and ground shadow, and center the display while preserving its colors, play symbol and live indicator.
- 0f7d2e3: Replace the misleading nighttime “One more moment.” message with a real greeting in every language. Greetings remain time-based only, without name customization.
- a3e4234: Reduce native release binary size with full link-time optimization and only the font-parser features used by the renderer, retaining variable-font support and panic recovery.
- d7972d1: Replace widget move buttons with non-interactive remote instructions for Move, Confirm and Cancel.

## 3.4.0

### Minor Changes

- 9afa3ba: Add a Steam input icon with manual selection and automatic detection for Steam device labels.

### Patch Changes

- 0ec5651: Add Hungarian localization.

## 3.3.0

### Minor Changes

- 6b80570: Add a grid-first Home widget manager with app-style Configure, Move and Remove menus, plus Reset to defaults and Add widget pills above the grid. The six-column, four-row grid mirrors the six-app favorites row. Greeting combines weekday/date and a time-based greeting in four columns and one row, with the original 90.4px greeting and 18.4px date typography; Weather spans two columns and two rows with top-aligned content. By default Greeting sits at the top left and Weather at the top right, spanning the first two rows. Valid saved custom placements are preserved. Preview fixed-size placements with the remote or pointer and reject overlaps or out-of-bounds positions. Removing frees cells while retaining configuration, and adding only saves after confirming placement. Reset restores default widgets and positions without clearing their settings. Configuration no longer has show/hide widget switches. Inputs moves to its own sidebar section below Apps and is not a widget. The app dock is unchanged; date visibility/format remain inside Greeting settings, without a separate Date widget or configurable name.

  Keep the full weather-setup action readable in every language by wrapping long translations inside the fixed-size widget instead of truncating them.

  Preserve the original weather temperature typography and tight spacing instead of shrinking the temperature independently of its layout.

  Show the city before the weather status in Compact style, after the weather icon.

### Patch Changes

- a2acb54: Require holding Back to open Recent apps on every page, including Home. Short presses keep their navigation and dismissal behavior without opening the switcher.

## 3.2.1

### Patch Changes

- 6d96a1b: Darken wallpaper-upload and shortcut-editor modals to match their QR code backgrounds, removing the visible square while preserving QR contrast and quiet zones.
- 792782c: Fix the UI sounds icon rendering black and show amber input badges when a connected input explicitly reports no signal. Disconnected or unknown connections stay gray, and connected inputs without a no-signal report stay green.

## 3.2.0

### Minor Changes

- e5eb588: Add optional tvOS-inspired UI feedback sounds for navigation, selection, and Back, with a General setting that defaults to off. Sounds are originally synthesized and gracefully fall back to silence if audio is unavailable.

### Patch Changes

- 21e5199: Dismiss the app drawer when remote selection moves up to the favorites row, keeping the corresponding favorite selected on Home.
- 88f6431: Restore opaque gray backgrounds for upload and shortcut dialogs, action cards, delete confirmations, and the keyboard instead of Liquid Glass.
- db61888: Precompute lossless Liquid Glass wallpaper backdrops during asset preparation and uploads instead of decoding, resizing and blurring them before the first frame. Add startup diagnostics and regression coverage while preserving legacy wallpaper fallback.
- 91a25bc: Hide input connection badges left behind by mouse hover when remote navigation moves focus away.

## 3.1.0

### Minor Changes

- 6037963: Add Standard, No icon and Compact weather widget styles with native-rendered sample previews in Settings.
- 9b36e99: Add configurable A–Z or usage-based app sorting with persistent local launch counts, preserving favorite order. Replace the Compact Disc wordmark with a generic disc icon.

## 3.0.1

### Patch Changes

- 52e3dca: Fix the Home header clock dropping its final digit by reserving rounding space in its single-line text box.

## 3.0.0

### Major Changes

- 34b7711: Redesign the native interface with Liquid Glass surfaces, a wallpaper-first Home, a floating
  favorites dock, compact weather, and an animated app drawer. Down opens the drawer, Up reaches
  the corner controls, and Back closes the drawer and focuses the first favorite.

  Apps now launch directly from the main interface. Manage favorites, hidden apps, and uninstall
  actions in Settings → Apps. Inputs move into a top-left dropdown with names, icons, and connection
  dots. The top-right clock opens floating Control Center-style actions for Luma Settings or native
  LG settings. Glass uses neutral adaptive tint, sharper edge lensing and softer interiors. Remove the Home greeting,
  standalone date, and navigation tabs while preserving existing saved preferences.

  Keep the drawer artwork-only, without categories/search or extra Apps/Move apps labels. Back from
  any Settings sidebar item now returns Home directly. Apps management uses a three-column card
  gallery with the existing action popup. Bundle colored Breezy Weather icons with day/night variants
  and their license/source notices.

  Enlarge and reorganize the weather widget and its empty state, show direct compact input pills,
  reduce settings action sizes, and place Apps immediately after General. Keep the dock decorative
  with unclipped glass edges and more space before the drawer grid.

  Size input pills to their labels and center their contents vertically. Keep the clock time-only,
  use the Luma logo and settings gear for the corner menu actions, and close the drawer when Up
  leaves the pinned row for the header. Preserve the original System update status card and inner
  action button, including versions, progress and retry behavior.

  Let drawer apps scroll behind the floating dock, clipping at the pinned cards' top edge rather
  than below the glass. Keep remote-focused grid cards fully visible below the dock.

  Remove App names settings and captions across Home, the drawer, and Settings → Apps. Ignore legacy
  caption preferences. Tighten the drawer's dock-to-list gap to 24px and match its bottom padding.

  Backfill unused dock slots with visible apps without saving them as favorites or duplicating them
  in the drawer. Add Move to visible favorites' Settings → Apps menus to start dock reordering;
  only saved favorites can be reordered.

  Add Sony, DVD, Blu-ray, and Disk input icons with manual selection and automatic name matching.
  Use sourced Sony, DVD, Blu-ray Disc, and Compact Disc trademark vectors, with pinned source
  hashes and trademark notices, rather than hand-drawn approximations.

  Show input connection dots only on hover, press, or remote selection. Animate the extra status
  slot width in and out, leaving compact 14px right padding when idle and honoring reduced motion.
  Normalize input artwork by trimming source whitespace and applying optical widths to wide
  wordmarks while preserving their proportions and consistent label spacing.

## 2.19.0

### Minor Changes

- 02c5bf7: Consolidate the unpublished changes into the 2.19.0 minor release, using the last published IPK version, 2.18.1, as the release baseline.

  Include the per-input SVG icon picker, Catalan localization, grouped System maintenance actions, and earlier cold-boot Home integration on webOS 5 and 6. Existing input icon choices default to Automatic and remain configurable under Settings → Widgets → Inputs → select input → Icon. The early Home helper is registered by `home.sh enable`, removed by `home.sh disable`, and retains the Homebrew cold-boot fallback.

  Complete the Catalan input-icon translations and align both the unit and desktop smoke navigation tests with Text rendering appearing before Refresh apps & inputs. The smoke test verifies focus on Text rendering before opening its options and exercising mode persistence across restarts.

## 2.18.1

### Patch Changes

- 6dbc626: Publish a standalone application manifest with release assets for central webOS Homebrew repository submissions, while preserving the existing custom repository feed.

## 2.18.0

### Minor Changes

- 2bc5d0a: App icons that can only be read through Homebrew Channel are now kept on disk, so they appear at once after a restart instead of seconds later.

  Add Reset icon cache in Settings → System to clear persisted and decoded icons and reload artwork without changing preferences.

### Patch Changes

- 0504a6f: Show up to four HDMI inputs on one Home row, matching the most ports any LG TV has. Cards keep their size when Auto shows or hides an input. Long names and statuses shrink to fit, and names too long even at a readable size end with an ellipsis instead of being cut off.
- 622f7de: Highlight the Home, Apps and Settings tab under the Magic Remote pointer with a faint pill that never looks like the active page, and clear it when the pointer moves away.
- 73e799f: Recognize PlayStation input logos when TV labels include a category, such as "PS5 Game Console".
- 59f3520: Fix a crash on webOS 4 when some remote keys were pressed: text input now stays off until the on-screen keyboard opens.

## 2.17.0

### Minor Changes

- 7a1e3ec: Add dedicated Nintendo Switch, Switch 2, NVIDIA Shield, and Xbox input icons, selected from detected or custom input names.
- bb1871f: Support Home takeover on webOS 24 (9.x). Its SAM has no `releaseBlockSystemApp`,
  so a mounted Home manifest is never reread; webOS 24 therefore takes the same
  executable method as webOS 5/6 instead of the manifest method used on 10.x/11.x.

### Patch Changes

- e22d48e: Add Czech (`cs`) translation: full `messages.json` catalog and locale registration, including localized date formatting with genitive month names. The bundled Inter subset already covers all Czech letters (á, č, ď, é, ě, í, ň, ó, ř, š, ť, ú, ů, ý, ž and their capitals).

## 2.16.2

### Patch Changes

- c635023: Fix rectangular seams around installed app icons when favorite cards fade at the screen edges.

## 2.16.1

### Patch Changes

- 652d8e7: Choose black or white favorite hearts for contrast against the card background, without a circular backing.
- 33f78cc: Refresh HDMI cable and signal status while Home or input settings are visible, without reloading apps or losing the last valid status on read failures. Preserve the existing cable-based Auto visibility and `hdmiPlugIn` priority, and show No signal when a connected cable has no signal.
- 161a9a3: Replace TV-side Node helpers with a bounded native Rust helper and synchronize splash images directly from privileged Home processes.

## 2.16.0

### Minor Changes

- c47548c: Add an on/off gradient toggle beside the background rotation control, with the choice saved across launches.

### Patch Changes

- c667091: Keep favorite hearts visible on light app artwork with a dark circular backing in the app grid and options preview.

## 2.15.3

### Patch Changes

- 166060e: Use consistent chevron spacing in settings breadcrumbs and highlight only the final item.

## 2.15.2

### Patch Changes

- 7ee2a7e: Fix in-app updates failing before installation because systemd expanded variables in the installer script.

## 2.15.1

### Patch Changes

- 3d881a2: Add Swedish (`sv`) translation: full `messages.json` catalog and locale registration, including localized date formatting. The bundled Inter subset already covers å, ä and ö.

## 2.15.0

### Minor Changes

- a2580d9: Install verified Luma releases from the existing Settings → System update card, with download/install progress and retry after failure. Installation uses Homebrew Channel's root service and a detached systemd unit, persists outcomes across restarts, and refreshes Home takeover with recovery on failure. Desktop and scripted previews cannot install updates.

### Patch Changes

- ad3bb27: Add date widget visibility and localized or numeric date formats. Remove the greeting's empty space when hidden so weather moves directly below the date.

## 2.14.0

### Minor Changes

- 3b610ec: Add a Show on Home toggle under Widgets → Greeting to hide the greeting without clearing its saved
  name or hiding the date and weather. Existing installations keep the greeting enabled.
- 3b610ec: Check the official release feed in the background on TV startup and show an Update available notice
  and System settings card for newer versions. Open Homebrew Channel from the card to update safely
  through the existing installer. Offline failures are non-disruptive and desktop tests stay offline.
- 3b610ec: Consolidate the System version and update information into one status card. Show up-to-date status
  after a successful check, with Check again, checking and retryable failure states. Keep the Homebrew
  Channel CTA for available updates and support up-to-date previews with the current version.

### Patch Changes

- 3b610ec: Remove the Apps page's top clipping and edge fade so cards scroll behind the fixed navigation.
  Keep categories, search and their clear controls fixed above the scrolling grid, with protected
  pointer interaction and remote focus visibility.
- 3b610ec: Give app categories pill surfaces matching search, with larger, heavier category and search labels.
- 3b610ec: Place background Upload left of Rotation with matching remote navigation. Enlarge app captions
  on Home and Apps and increase their weight, retaining space for wrapped names.
- 3b610ec: Enable live update checks and Check again in interactive desktop runs using the same endpoint as
  TV. Keep scripted captures and simulated-version previews offline.
- 3b610ec: Move Greeting's visibility toggle to the first row and label it Show widget, matching Weather.
- 3b610ec: Enlarge the clock, Home weekday/date and input descriptions for better TV readability.
- 3b610ec: Refresh the Homebrew Channel listing to describe uploaded backgrounds, rotation, QR-managed webhook
  shortcuts and current customization options.
- 3b610ec: Update the Homebrew Channel description to list all supported languages and add regression coverage
  to keep future language additions reflected in release metadata.
- 3b610ec: Place Language above Reduced motion, sort language choices alphabetically by their displayed names,
  and show a readable empty-state card when no shortcuts are saved.
- 3b610ec: Let Settings lists scroll behind fixed navigation, sidebar and background actions without viewport
  clipping or edge fades. Preserve fixed-control pointer priority and remote focus visibility.
- 3b610ec: Increase version and shortcut row opacity, match shortcut description cards to HDMI settings rows,
  and make shortcut removal buttons circular.
- 3b610ec: Simplify the shortcut empty-state hint to explain that Configure adds the first shortcut.
- 3b610ec: Open shortcut test/removal actions in a popup from selectable Settings rows, lighten the System
  version surface, and refine category focus/active styling with a full-height clear button.
- 3b610ec: Reduce Languages and Backgrounds bottom scroll padding from 64 to 48 logical pixels.
- 3b610ec: Add Traditional Chinese (繁體中文) immediately after Simplified Chinese, with translated interface
  text, persisted language selection, Chinese date/time formatting and verified bundled glyph coverage.
  Update Homebrew metadata and use the provider's Chinese locale for geocoding.
- 3b610ec: Enlarge the TV Settings button to 60px and center it alongside the top navigation bar.
- 3b610ec: Add --test-update-version and LUMA_TEST_UPDATE_VERSION to preview the update card without contacting
  GitHub or persisting simulated update metadata.
  Display the running binary's version rather than a potentially stale generated-artwork version.

## 2.13.0

### Minor Changes

- 13211ee: Add shortcut tiles that send a GET or POST request instead of launching an app, for example to a
  Home Assistant webhook. Create, edit, test and remove them in Settings › Shortcuts, or scan a QR
  code there to edit them on a phone or computer instead of with the remote.
- ffcf8ed: Group Greeting, Weather, Shortcuts and Inputs under Settings → Widgets. Create and edit shortcuts
  through Configure's phone/computer QR page, with passive shortcut rows and separate TV removal
  buttons. Preserve existing preferences, increase Settings list bottom spacing and use a flat
  background on the shortcut web page.

### Patch Changes

- 61b673d: Focus the rotation button when pressing Right from the Background settings sidebar instead of the active background.
- 61b673d: Remove excess right padding from the Upload button by sizing it to its localized label.
- 61b673d: Keep the background upload modal and session open when the desktop preview loses focus, so switching to a browser does not interrupt uploads. TV focus-loss and window hiding behavior are unchanged.
- 61b673d: Remove the background gradient from the wallpaper upload page for a flat dark appearance.
- 13211ee: Keep shortcut context-menu actions on the webhook path, prevent platform app deletion of shortcuts,
  and preserve accepted phone-editor changes when its dialog is reopened before pending saves are applied.
- 61b673d: Shorten the Background settings upload button label to “Upload” in every language.

## 2.12.0

### Minor Changes

- efb6f3b: Return to Luma's Home tab when the remote Home button relaunches an existing Home-takeover instance, dismissing overlays and preserving keyboard edits. Normal-tile launches retain LG's system Home-button behavior.
- 8430690: Add a Rotation On/Off button beside the upload action, defaulting to Off with a crossed-out rotation icon. When enabled, all available backgrounds advance on each Luma restart and every 15 minutes, with a slow cross-fade while preserving the manually chosen LG splash (#96).

### Patch Changes

- cb47502: Allow an explicit fullhome takeover target, preserve its native lifecycle identity, and distinguish mount status from Home-key verification.
- 1abd414: Handle the remote Home key directly while Luma runs as the Home replacement, returning from Apps, Settings and overlays even when webOS does not send a lifecycle relaunch for an already-foreground Home. Normal app-tile launches retain system Home-key routing.

## 2.11.0

### Minor Changes

- 90aa848: Upload named custom wallpapers from a phone or computer using a temporary local URL and QR code in Background settings. Preview and adjust a 16:9 crop before adding it to the TV's persistent wallpaper gallery.
- 90aa848: Add app-style side popups for backgrounds, with selection and confirmed deletion of any background except the active one. Persist removal of bundled backgrounds from the gallery and delete uploaded files on the wallpaper worker.

### Patch Changes

- 90aa848: Balance background upload button and dialog padding, and make the non-interactive System version row more transparent.
- 90aa848: Separate QR and browser instructions in the background upload dialog, simplify the displayed URL, and remind users to keep the dialog open during upload.
- 90aa848: Place the background upload action in a compact, right-aligned button above the gallery.
- 90aa848: Complete Vietnamese and Dutch translations for background selection, deletion, and upload messages.
- 148d4eb: Add Dutch (`nl`) translation: full `messages.json` catalog and locale registration in `i18n.rs` (language list, display name and localized date formatting such as « WOENSDAG 21 OKTOBER »). The bundled Inter subset already covers Dutch, so no font changes are needed.
- 90aa848: Scroll only the background gallery while keeping the settings sidebar and upload button fixed. Clip gallery drawing and pointer targets below the upload action and keep remote-focused cards fully visible.
- 90aa848: Make the background upload URL larger and heavier for easier reading on TV.
- 90aa848: Display the System version in a full-width, non-interactive settings row with clearer text.
- 90aa848: Rename the background upload button and dialog title to “Upload background” across all languages.

## 2.10.3

### Patch Changes

- df0f4bb: Scale long Home greetings down to fit their existing single-line area without cropping names or moving weather and favorites. Keep LG's system keyboard and its existing name-entry behavior unchanged.
- 49df652: Add Vietnamese (`vi`) translation: full `messages.json` catalog, locale registration in `i18n.rs` (language list, display name, and localized date formatting), and Latin Extended Additional (U+1EA0-1EF9) glyphs in the bundled Inter font so Vietnamese letters no longer fall back to the CJK font.

## 2.10.2

### Patch Changes

- 93afffa: Reuse existing app-owned preferences and weather storage when running as LG Home, preserving normal-app access after saves. Match the staged Home splash to the same saved preferences instead of a root-owned fallback.

## 2.10.1

### Patch Changes

- 8dfa7cb: chore: trigger release

## 2.10.0

### Minor Changes

- 157d23c: On the Apps page, open a side popup beside the selected app with a normal OK press or click, with Open first and focused by default. Home favorites continue launching directly. Add app deletion with a Cancel-focused confirmation, protecting Luma, Homebrew Channel and apps in the webOS, Palm and LG system namespaces from removal. LG's Settings and More buttons retain their system behavior.

  Reconcile uncertain uninstall replies with the TV's installed-app inventory and refresh after errors so completed removals do not leave stale cards in Luma.

  Use the normal appInstallService/remove endpoint through an elevated, completion-aware helper so Content Store apps can be removed instead of failing through Homebrew's developer-only removal path.

### Patch Changes

- 75d0a87: Transparent app icons get a gradient card in the logo's own hue with at least 3:1 contrast instead of near-black, and are scaled with premultiplied alpha so their edges no longer pick up a dark fringe.

## 2.9.0

### Minor Changes

- a1838e6: Restore MSAA text rendering with no emboldening as the default and add Settings → System → Text rendering to choose analytic rendering. The saved choice applies when Luma next starts.

### Patch Changes

- 973ddf7: Add French (`fr`) translation: full `messages.json` catalog and locale registration in `i18n.rs` (language list, display name, and localized date formatting such as « MERCREDI 21 OCTOBRE » with the ordinal « 1ER » for the first day of the month). The bundled Inter subset already covers French accents and ligatures, so no font changes are needed.

## 2.8.5

### Patch Changes

- b1dcbcf: Let LG's screen saver start while Luma is in front. SDL-webOS answers the TV's screen saver requests on Luma's behalf and declined them, because SDL suspends the screen saver unless `SDL_VIDEO_ALLOW_SCREENSAVER` allows it.
- e353196: Add Polish (`pl`) translation: full `messages.json` catalog, locale registration in `i18n.rs` (language list, display name, and localized date formatting with genitive month names), and Latin Extended-A glyphs in the bundled Inter font so Polish letters such as ą, ę, ł, ś and ż no longer fall back to the CJK font.

## 2.8.4

### Patch Changes

- 2a03caa: Read the app and input inventory directly from the TV when Luma runs in place of LG's Home, so an early root integration shows apps before Homebrew's service has started; other cases still use Homebrew.
- 945598a: Retry the TV inventory while the TV service starts. Right after a cold boot or when the TV is short of memory, the Homebrew service that reads the app and input inventory can take longer than the 6.5 s Luna timeout. While no apps are shown yet, Luma now keeps loading and retries up to 5 times every 2 s before it shows the error and its Retry button.

## 2.8.3

### Patch Changes

- 2abfbdf: Hide the updating status label while closing recent apps and refreshing history afterward, including when closing the last card. Keep loading feedback limited to the initial history read and hide it when dismissing the switcher.

## 2.8.2

### Patch Changes

- d5f778c: Fix black or partly black screens after presses that change nothing, such as Right at the end of a row: an unchanged frame named a damage pixel outside the surface, which left `EGL_KHR_partial_update` nothing to preserve. It now names, clears and redraws one pixel inside the drawable.
- d408d35: Add Russian (`ru`) translation: full `messages.json` catalog, locale registration in `i18n.rs` (language list, display name, and localized date formatting with genitive month names), and Cyrillic glyphs in the bundled Inter font so Russian text no longer falls back to the CJK font.
- 5317139: Redesign Recent apps as a layered card stack with depth-aware browsing transitions. Closing a card with Up animates it off the top of the screen and brings the remaining cards forward, with a return animation if closing fails and immediate updates when reduced motion is enabled.

## 2.8.1

### Patch Changes

- a0eddd9: Draw crisp shape edges. The paint shader's edge filter ran from 0.85 px inside a shape to its edge, so every edge had a half-covered pixel and one-pixel borders never became opaque. Coverage is now a one-pixel box filter centred on the edge, and rectangles and logo placement snap to whole drawable pixels, so straight edges stay a single step through a TV's 2x scale-up of its 1080p graphics plane.
- 3359618: Render crisper text and logos on the TV. The webOS window is created without MSAA so text and logos use their analytic edge filters, which keep stems sharper through the panel's 2x scale-up of a 1080p graphics plane than 4x sample coverage, at the same cost. Small text is emboldened by 0.25 physical pixels per edge so thin stems get an opaque core. Desktop previews keep multisampling.
- 7b413b0: Fold full-screen translucent fills into the wallpaper. The Recents backdrop and gradient each cost a full screen of fill per frame; the wallpaper is now drawn once with their combined effect, and during the Recents crossfade they sit beneath the outgoing interface so they fold in as well. On an LG webOS 5 TV the Recents carousel runs at 48 instead of 37 fps and opening or closing Recents drops from 100 to 63 ms at the 95th percentile.
- 55f8b9c: Prepare the Apps and Settings pages while the screen is idle, so their first visit does not pay for text layout, new glyph outlines, logo meshes and icon uploads. On an LG webOS 5 TV the worst frame of the first Apps visit drops from 179 to 40 ms.
- 8592248: Redraw only what changed since the back buffer's frame. Consecutive scenes are compared command by command and only the changed area is cleared and drawn, using EGL buffer age with `EGL_KHR_partial_update` where available. On an LG webOS 5 TV, focus moves on Home run at 60 instead of 39 fps at 1080p (p95 17 instead of 34 ms) and mixed navigation redraws a fifth of the screen on average.
- 32086d9: Stop the launch splash update failing when Luma is Home on webOS 5 and 6. The executable takeover method stages no Home app copy, so the update now leaves out the staged splash and only refreshes the normal tile's splash.

## 2.8.0

### Minor Changes

- af18b35: Register the Home takeover with SAM on webOS 25/26, so the Home key can relaunch the running Luma instead of SAM killing and restarting it. LG's Home role there is app-ID based, so Luma cannot register on its own connection; as root it holds the registration through `luna-send -a com.webos.app.home` with lifecycle v1 (`registerNativeApp`), without delaying startup. The helper ends with Luma. A registration SAM never accepts leaves the previous behaviour; one that is lost later quits Luma so SAM starts Home afresh. If a relaunch does not bring Luma to the front within 3 s, Luma turns registration off with `/var/lib/luma-home/no-lifecycle` and restarts. Relaunch now also restores and shows the window before raising it, and the staged Home logs to `/tmp/luma-home.log`.

### Patch Changes

- 9f3fa1f: Draw square images, above all the full-screen wallpaper, through the paint shader's plain texture path instead of the rounded-rectangle distance field. On an LG webOS 5 TV the Home scene renders at 37.5 instead of 29 fps at 1080p.

## 2.7.0

### Minor Changes

- 9b38438: Support running in place of LG's Home executable on webOS 5, where a root integration bind-mounts Luma over `/usr/bin/com.webos.app.home`. Luma then registers with SAM itself through lifecycle v2, so Home launches bring the running instance back instead of restarting it. Production and developer installations are supported by pre-start library search paths and native package discovery that matches the mounted executable, keeping assets and splash updates tied to its source installation. Registration failures are reported at startup, a lost registration quits so SAM can start Home afresh, and a panic in a subscription handler no longer unwinds into LS2.
- c6dd60f: Support `home.sh enable` on webOS 5 and 6. There, SAM cannot reread a mounted Home manifest, so the takeover bind-mounts Luma's executable over LG's Home executable instead, keeps LG's manifest and role, and relies on Luma's own lifecycle registration. Enabling, the cold-boot hook, status and restoring LG Home work as on webOS 25/26; a failed launch restores LG Home, package updates are remounted, and a normal tile launch opens the running Home instance.

## 2.6.0

### Minor Changes

- e2b5531: Let the Home favourites row run to the screen edges like Apple TV's rows: tiles are slightly narrower (256 px) so six show in full while the seventh peeks in and fades out at the right edge, and once the row scrolls the left edge fades in. The visible edge fragments support pointer selection and horizontal wheel scrolling without moving clicked targets. A new horizontal edge fade joins the vertical one in the paint, vector and text shaders. The strip still ends after the sixth tile, so a focused tile never sits in the fade.

### Patch Changes

- 2ed5213: Decode every installed app icon in the background right after the app inventory loads, so the Apps page opens with its artwork ready. Prefetched icons are fed to the artwork worker a few at a time through their own list. Visible cards take priority over queued background work, including when a prefetched icon becomes visible, while request queues and decoded caches stay bounded.

## 2.5.0

### Minor Changes

- 1cc543f: Simplify Recents to bare app cards: remove app captions, close buttons and the empty-state subtitle. Close the focused app directly with Up. Make the overlay more transparent and add a restrained, reversible crossfade between the switcher and its parent screen, with immediate input restoration and reduced-motion support.
- 1cc543f: Add an Apple TV–inspired recent-app switcher, opened by holding Back for 600 ms or selecting the new header shortcut. Browse the TV's actual recent history, launch an app, close one card, or use LG's own Close all operation. Keep motion restrained, support reduced motion and pointer navigation, and load available recent-card artwork off the UI thread. Include a labeled, isolated desktop preview.
- 1cc543f: Open Recents with a short Back press on Home and keep long-Back access from other pages. Remove the header shortcut, card counter and instructions, and move a more compact Close all button to the top-right. Reuse the Apps card renderer for consistent vector logos, installed icons and surfaces, fixing mismatched YouTube artwork in Recents.

### Patch Changes

- e084e94: Remove the desktop preview label from the recent-app switcher.
- 1cc543f: Fix native remote Back dismissal and long-Back handling in the app switcher. Preserve SDL-webOS Back/Exit scancodes, claim the Magic Remote's long-Back Exit key, and consume dismissal presses through release so they cannot also quit Luma. Remove the switcher's heading and subtitle for a quieter card-only layout.

## 2.4.0

### Minor Changes

- df483bf: Add Italian (`it`) translation: full `messages.json` catalog, plus locale registration in `i18n.rs` (language list, display name, and localized date formatting).
- 395cdac: Add curated vector cards for NRK TV and TV 2 Play, with exact app-ID matching, card colours, and desktop demo support.

### Patch Changes

- 5b5c4a3: Recognize Apple Music's verified `com.apple.applemusic` app ID so the installed TV app uses its curated vector card.
  Match the Apple Music card's dark background gradient to Apple TV.

## 2.3.1

### Patch Changes

- db4fece: Fix Home takeover enable failing with "no app matched by pid" by closing the normal Luma instance through the app-ID endpoint. Allow takeover to continue if that instance already exited during the close request.
- db4fece: Update the normal app launch splash to match the saved background, including after changing it in Settings. Refresh the matching Home takeover splash as well.

## 2.3.0

### Minor Changes

- 3bf0ad6: Show a wallpaper splash at launch and match the saved background when staging the native Home integration.

## 2.2.2

### Patch Changes

- 072b7e9: Show installed app icons as soon as they are decoded instead of one per idle wake-up.
- fed795c: Keep decoded app icons across the automatic inventory refreshes at startup and whenever Luma becomes visible, reading an icon again only when its app's version or icon paths change. Refresh apps still reloads every icon.
- 969a6fd: Keep the neutral card colour for transparent app icons whose logo reaches the edges, instead of filling the card with the logo's own colour.

## 2.2.1

### Patch Changes

- d81cb66: Publish the changes from the unreleased 2.2.0 version after improving CI dependency installation and build timeouts.

## 2.2.0

### Minor Changes

- 201c030: Add an optional seconds display to the header clock, configurable with the remote in Clock format settings.
- e8f9085: Add 27 curated vector logos for streaming, music, and regional TV apps, including NOW / Now TV, plus dedicated Live TV and browser vector cards. Select branded cards by exact app IDs instead of keywords, so unrelated apps such as Amazon Alexa retain their installed artwork. Include every SVG-backed app in the desktop demo and add a rendered gallery preview.

  Remove dumped TV artwork, icon/colour maps, and their collection tools and tests. Desktop previews now use curated vectors or generic/category fallbacks, and builds no longer depend on collected icon fixtures.

- 589044b: Add custom input names: rename any input with the remote in Settings → Inputs, or reset it to the
  port name. Home cards, settings rows and the breadcrumb show the custom name, keeping the TV label,
  port and connection status as detail.
- 978b946: Discover app icons directly from installed TV applications instead of requiring collected artwork and icon-map updates. Load visible artwork in the background, derive matching card colours automatically, and refresh cached icons with app inventory. Exclude desktop-only collected icons and maps from TV packages.
- 9463b56: Add an optional name to the Home greeting, with remote-friendly name replacement and reset controls.
- e2c2a12: Add separate Home and Apps caption settings, with names hidden on Home and visible in Apps by default.
  Legacy caption settings do not override the new per-view defaults.
  Group greeting name editing under Greeting, use Clear name for greeting and input names, and reserve
  the bright settings-row highlight for the focused control.

  Group clock preferences under Clock, with separate nested Clock format and Time format selections.
  Time format offers HH:MM by default or HH:MM:SS while retaining existing saved seconds preferences.
  Move Refresh apps & inputs and Quit app into a dedicated System sidebar category.
  Keep the top-level Clock row free of format values and remove the chevron from input Rename actions.

### Patch Changes

- dcf1b03: Read the real HDMI cable state from `hdmiPlugIn` on TVs that report every port as `connected`, so
  inputs in Auto mode only appear when a device is actually plugged in.

## 2.1.0

### Minor Changes

- d417291: Add Brazilian Portuguese as a selectable interface language, including translated app labels,
  localized dates and weather location searches.

### Patch Changes

- 27802ac: Match brand keywords at ASCII word boundaries so unrelated apps such as dashboards retain their
  own artwork and Utilities category instead of being mistaken for ARD/Das Erste.
- 14b18f8: Render Korean app and input names using bundled Hangul font subsets loaded on demand. Add
  Korean-labelled LG Gallery and Room Connect apps to the desktop preview inventory.
- 3e256f6: Add installed-app artwork collected from an LG C1 for Korean-market apps, with matching card
  background colours. Make icon collection work on older webOS Node versions and follow the
  launcher's app visibility and exclusion rules.

  Include Coupang Play, Watcha Play, Wavve, Rainbow TV and LG TV VOD in the desktop demo inventory
  so their artwork can be previewed during development.

- ee2f6ff: Coordinate app releases through Changesets with synchronized Rust and webOS versions, publishing
  only after the checked Version Packages PR is merged.

  Add updating PR comments that report missing changesets and explain semantic version bumps for
  code changes.
