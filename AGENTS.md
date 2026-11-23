# Documentation screenshots

The webOS Homebrew apps repository links directly to `docs/home.png`, `docs/apps.png`,
`docs/settings.png` and `docs/inputs.png` on `main`. Keep these images current when changing
the UI they depict, including layout, styling, navigation or default settings.

- Regenerate affected images with the actual native renderer using `pnpm screenshots`.
  Use `pnpm screenshots --only=home` (or `apps`, `settings`, `inputs`) for a targeted refresh.
  On headless Linux, run inside `nix develop` with `xvfb-run -a` and
  `LIBGL_ALWAYS_SOFTWARE=1`.
- Use the screenshot tool's isolated sample data and fixed capture settings. Do not substitute
  generated artwork or browser mockups for native captures.
- Inspect regenerated images for correct content, clipping and missing assets before committing
  them alongside the UI changes. Keep the 1920×1080 dimensions and existing paths so upstream
  screenshot URLs remain valid.
- These are desktop previews, not evidence of physical TV behavior. If a capture cannot be
  refreshed, disclose that limitation rather than claiming it is current.
