# Home integration on webOS 4

webOS 4 embeds its ribbon launcher in the compositor rather than a separate Home
executable. This optional integration installs a compositor key filter and a
Homebrew startup hook without replacing LG's files.

## Enable or restore

Run in a **root shell on the TV** after installing and opening Luma:

```sh
cd /media/developer/apps/usr/palm/applications/com.ecklf.luma
python2 legacy-home.py enable
python2 /var/lib/luma-legacy-home/legacy-home.py status
```

For installations under `/media/cryptofs`, use the installed application directory
instead. The manager uses stock Python 2.7 without third-party packages. It saves
the original key filters, copies recovery files into `/var/lib/luma-legacy-home`,
inserts its filter before LG's Home handler, and installs
`/var/lib/webosbrew/init.d/64-luma-legacy-home`.

Restore LG routing before uninstalling:

```sh
python2 /var/lib/luma-legacy-home/legacy-home.py disable
```

Disable removes only Luma's filter and startup hook, preserving later changes to
other filters and retaining the backup and recovery script. After package updates,
disable the old integration and enable it from the updated installation directory
to refresh the copied files.

## Compatibility behavior

- Home/Menu/Recent and OEM ribbon requests open Luma. A rejected launch falls back
  to LG Home without a redirect loop.
- Home resets Luma to its Home page only after its surface receives input focus.
- Back reaches focused Luma while LG overlays retain their own Back handling.
- Closing a foreground app returns to Luma without stealing focus during app
  switching or background memory reclamation.
- Screen-saver and accessibility filters retain their order. Prime, Voice and
  unrelated keys pass through unchanged. No personal mappings are installed.
- Homebrew calls resolve the old public LS2 registration dynamically. Newer unified
  LS2 keeps its existing registration. Mutations are not retried on another bus.
- If the recent-history service does not exist, Recent apps uses real running card
  apps and single-app close uses SAM. Inputs, Home, Live TV, AirPlay, the hidden
  system container and overlays are excluded. Permission errors and timeouts remain
  errors. The newer history backend is unchanged.

There are no navigation settings or Close all controls in this change.

## Hardware evidence and limits

The original contributor, @Tapuz97, reported Home routing, physical Home/Prime/Voice
keys, app navigation, software-restart startup and disable/re-enable restoration
on an LG **65UK6300YVB**, firmware **05.50.70**, OS **4.4.3-22**, SDK **4.4.0** in
PR #271. Those checks used the original combined contribution and earlier local
builds, not this extracted compatibility-only patch.

This extraction still needs TV verification, including Home, Back, single-app
closing, restart and restoration. Other webOS 4 models, hard power removal and all
Quick Start/standby combinations remain unverified. Desktop tests do not establish
hardware compatibility.
