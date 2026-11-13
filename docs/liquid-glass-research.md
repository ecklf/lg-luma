# Liquid Glass research and implementation

## What Apple documents

- [Meet Liquid Glass, WWDC25](https://developer.apple.com/videos/play/wwdc2025/219/)
  describes lensing that bends and concentrates light, rather than blur alone; size-dependent
  scattering, adaptive tint/shadows, and interaction-driven highlights. Glass is a control/navigation
  layer, not a treatment for every content surface. Avoid stacking glass on glass.
- [Adopting Liquid Glass](https://developer.apple.com/documentation/technologyoverviews/adopting-liquid-glass)
  explains tvOS focus adoption and warns that custom backgrounds can interfere with the effect.
- [Human Interface Guidelines: Materials](https://developer.apple.com/design/human-interface-guidelines/materials)
  distinguishes Regular's luminosity adjustment from Clear's media-first transparency. It recommends
  Regular for text-heavy popovers/sidebars, and considering 35% dimming beneath Clear on bright media.
  The tvOS section explicitly identifies Control Center, Top Shelf and focus as Liquid Glass uses.
- [Apple's tvOS 26 announcement](https://www.apple.com/newsroom/2025/06/apple-tv-brings-a-beautiful-redesign-and-enhanced-home-entertainment-experience/)
  emphasizes real-time reflection/refraction and keeping content visible behind Control Center.

Apple distinguishes adaptive Regular material from transparent Clear material. Clear calls for
bright/bold foreground content and dimming beneath media; it is not simply Regular with reduced
opacity. Apple has not published the shader, filter graph or numeric material parameters.

## Independent technical references

- [OverShifted/LiquidGlass](https://github.com/OverShifted/LiquidGlass/tree/3797fa541c1f026c521a75885ee271f03bdf9f0f):
  inspected `BatchRenderer2D.glsl` and `Blur.glsl`. Uses a superellipse distance approximation,
  nonlinear backdrop remapping and a separate separable blur. Useful as an optical demonstration,
  not an Apple implementation; GLSL 450, 32 samplers and its framebuffer workflow cannot be used
  directly in Luma's GLES2 renderer. No source code was copied.
- [Independent optical-model analysis](https://liquidglass.shayxo.dev/research/optical-model/):
  useful hypotheses include accurate SDF gradients, bounded displacement, sharper rims and softer
  interiors. Its numeric values and fidelity claims are not Apple documentation. Refraction
  direction differs among independent models and should not be described as an established Apple fact.

## Reference-image comparison

The supplied dock preserves recognizable wallpaper colors/details behind the app artwork. Its
edge is a fine light-catching contour, not a broad blue bevel. The supplied Control Center uses
independent rounded floating controls without a large outer glass container. The white selected
control is distinct from the translucent unselected controls.

The first Luma implementation hid the wallpaper behind 66% dark blue tint and blurred its only
backdrop level. A 22px bevel and incorrect flat-edge normals made it look like frosted plastic.

## Luma approximation

- Neutral, luminance-dependent absorption replaces the fixed blue tint. Focus remains an explicit
  pale surface with dark foreground content for remote legibility. This is a Regular-like adaptive
  approximation, not a claim to implement Apple's Clear material; bright backgrounds need stronger
  absorption to retain white-label contrast.
- A two-level cached wallpaper atlas retains sharp detail at the rim and increases scattering
  inside. Both levels crossfade with wallpaper changes. Atlas-half clamps prevent seam leakage.
- Analytical rounded-box gradients give horizontal/vertical normals on flat edges and radial
  normals at corners. Displacement is bounded to 9 logical pixels in a 4–10px rim.
- A narrow directional contour replaces the uniform white outline and wide painted bevel.
- Settings entry uses two independent right-side controls for existing actions only. No fake
  power, network, audio or timer controls are added.

These are authored approximation parameters, not reverse-engineered Apple constants. Wallpaper
sampling does not include UI behind a surface, does not respond to live video, and does not model
Apple's full reflection, vibrancy or fluid morphing. Cached filtering is performed when a wallpaper
is first used; it is not a per-frame blur. Each atlas costs about 4 MiB before driver overhead,
versus about 0.5 MiB previously. The existing texture budget/eviction still applies. GLES2 now
uses four backdrop fetches per glass fragment rather than two. Actual older-TV GPU performance
must be measured manually; desktop captures cannot establish either hardware speed or exact tvOS fidelity.
