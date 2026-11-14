# Overlay window probe

This is an opt-in diagnostic, not a shipping Luma feature. It packages a separate
`com.ecklf.luma.overlayprobe` QML app using the TV's `Eos.Window` host. It follows
the transparent `WebOSWindow` / `_WEBOS_WINDOW_TYPE_OVERLAY` mechanism in
[Homebar](https://github.com/rawinek/webos-homebar/blob/main/webos10/app/main.qml).
No Home takeover, system mounts, services, mapper rules or Luma preferences change.
It makes no network requests and never launches or closes another app.

**This tests firmware overlay support, not SDL support.** Success does not prove
that Luma's native renderer can make an overlay, that protected video will keep
playing, or that a global Back shortcut exists. Firmware compatibility is unverified.

## Build and install

With the normal project dependencies installed:

```sh
pnpm probe:overlay
pnpm exec ares-install --device tv .build/overlay-probe/ipk/com.ecklf.luma.overlayprobe_0.0.1_all.ipk
```

The package is deliberately outside `dist/` and the release feed. It does not need
an ARM build, generated Luma assets or the webOS NDK. Use a rooted test TV with an
ares profile named `tv`. If the firmware rejects QML developer apps or lacks
`Eos.Window`, record the error rather than changing system roles to force it.

## Manual TV verification

1. Start an HDMI source or Live TV first, then launch the probe remotely while
   leaving that source in front:

   ```sh
   pnpm exec ares-launch --device tv com.ecklf.luma.overlayprobe
   ```

2. Check that the source stays visible through the untouched upper screen and the
   translucent lower panel. A black background or replacement of the current card
   is a failure, even if the test cards appear.
3. Press Left/Right and check that only the highlighted test card changes. Press
   Back and check that the panel disappears and input returns to the underlying
   source without relaunching it. Repeat with OK and a pointer click.
4. Launch the probe again while hidden. Check that it reappears and can be
   dismissed again. Leave it untouched once to check the 20-second auto-hide.
5. Repeat over YouTube and a streaming app, recording whether video/audio continue,
   pause, turn black or resume after dismissal. Do not infer streaming support
   from HDMI alone. Check LG Settings and Home as well for stacking problems.
6. Record TV model, firmware/webOS version, source/app, cold versus warm launch,
   spinner/flashes, focus restoration and pointer behavior. Console messages use
   the `Luma overlay probe:` prefix when firmware logs expose QML output.

The three cards are labeled placeholders, not recent apps. There are no Luna
calls. Back here is only a dismissal key while the probe is focused, not a global
hook. Opening it from its tile first is not sufficient to test overlaying a video
app, because navigating to the tile may already background that app.

## Cleanup and next step

The window hides rather than exiting, so its QML process may remain resident for
warm relaunch testing. To terminate it or remove it:

```sh
pnpm exec ares-launch --device tv --close com.ecklf.luma.overlayprobe
pnpm exec ares-install --device tv --remove com.ecklf.luma.overlayprobe
```

If this passes, the next experiment is a per-window overlay role in our pinned
SDL-webOS backend, EGL transparency, and a transparent native draw path. The
current Luma renderer clears opaque and draws wallpaper. Neither that renderer
nor the existing Recent apps interaction is changed by this probe.
