# Luma

A native launcher for rooted LG webOS TVs, inspired by Apple TV.

> [!NOTE]
> Tested on **LG OLED55C57** running **webOS 26**.\
> External testing confirms compatibility with **webOS 6 and newer**.

<table>
  <tr>
    <td width="50%" align="center">
      <img src="docs/home.png" alt="Luma native Home screen, desktop preview">
    </td>
    <td width="50%" align="center">
      <img src="docs/apps.png" alt="Luma native app library, desktop preview">
    </td>
  </tr>
  <tr>
    <td width="50%" align="center">
      <img src="docs/settings.png" alt="Luma native General settings with Quit app, desktop preview">
    </td>
    <td width="50%" align="center">
      <img src="docs/inputs.png" alt="Luma native input settings, desktop preview">
    </td>
  </tr>
</table>

## Installation

### Homebrew Channel

```text
https://github.com/ecklf/lg-luma/releases/latest/download/repo.json
```

### SSH

On your computer, replace `TV_IP`:

```sh
scp -O dist/com.ecklf.luma_2.0.3_arm.ipk root@TV_IP:/tmp/luma.ipk
ssh root@TV_IP
```

In the TV's root shell:

```sh
luna-send -i luna://com.webos.appInstallService/dev/install \
  '{"id":"com.ecklf.luma","ipkUrl":"/tmp/luma.ipk","subscribe":true}'
```

## Replacing LG Home

### Takeover Method

> [!WARNING]
> Home replacement is experimental.\
> Confirmed working on **webOS 26**.\
> Ensure to disable takeover before uninstalling Luma.

Install Luma and verify that it opens normally. Disable any Home mapping e.g.
[LG Input Hook](https://repo.webosbrew.org/apps/org.webosbrew.inputhook/) and any other Home
replacement, then run in the TV's root SSH shell:

```sh
APP=/media/developer/apps/usr/palm/applications/com.ecklf.luma
# If you enabled Lumas autostart method earlier, disable it first
# sh "$APP/autostart.sh" disable
sh "$APP/home.sh" enable
```

Takeover mounts Luma over the system Home app and reapplies the mount at cold boot. The LG splash
shown before Luma's window opens is the wallpaper you selected in **Settings → Background**, staged
from the saved preferences at each boot (the plain package uses the default background).
LG Home's original files remain underneath, using the same approach as
[Glasshouse Home](https://github.com/rorygallagher2024/glasshouse/blob/main/server/lib/home-takeover.js).

To see the status or disable the home override use:

```sh
sh /var/lib/luma-home/home.sh status
sh /var/lib/luma-home/home.sh disable
```

`restore` is an alias for `disable`.

Boot log: `/var/lib/luma-home/boot.log`. See the
[development guide](development.md#optional-native-home-integration) for implementation details.

### Autostart Method

Open Luma 25 seconds after a full cold boot. Run in the TV's root SSH shell:

```sh
sh /media/developer/apps/usr/palm/applications/com.ecklf.luma/autostart.sh enable
```

Use this when takeover is disabled. It launches Luma as a regular app and does not remap the Home button.

Disable before enabling takeover or uninstalling Luma:

```sh
sh /media/developer/apps/usr/palm/applications/com.ecklf.luma/autostart.sh disable
```

### LG Input Hook Method

You can use [LG Input Hook](https://repo.webosbrew.org/apps/org.webosbrew.inputhook/), configure an Execute action with this one-line command:

```sh
/usr/bin/luna-send -n 1 -w 5000 luna://com.webos.service.applicationmanager/launch '{"id":"com.ecklf.luma"}'
```

Test on a spare button first. Disable the mapping in Input Hook to restore normal Home behavior.

## Development

### Setup & Building

From the repository root, with Nix flakes enabled:

```sh
nix develop
pnpm install --frozen-lockfile
pnpm sdk
export WEBOS_NDK="$PWD/.build/toolchain/arm-webos-linux-gnueabi_sdk-buildroot"
pnpm run package
```

The first native runtime preparation also downloads a
checksum-pinned SDL-webOS library; both downloads are cached under `.build/`.

Output:

```text
dist/com.ecklf.luma_2.0.3_arm.ipk
dist/sha256sums
dist/repo.json
dist/app-icon-large.png
```

Open [webOS Dev Manager](https://github.com/webosbrew/dev-manager-desktop), connect to the TV's
IP on port **22** as **root**, and install the IPK. Open **Luma** from the TV menu. Homebrew
Channel must be elevated; on webOS 26 use **0.8.0-rc1 or newer** for its `lunabus.query` mapping.

The native package uses **`com.ecklf.luma`** and installs separately from the old
`com.luma.launcher` web app and the former `com.luma.launcher.native` / `com.luma.native` prototypes. Native preferences
are local JSON files and are separate for each installation; browser preferences are not migrated.
Homebrew Channel treats this ID as a separate app rather than an update to the old launcher.

Wait for installation success; Ctrl+C closes an ongoing subscription. Fully close and reopen
Luma after updating. The [development guide](development.md#tv-startup-diagnostics) has a jailed
launch command that exposes errors hidden by the normal menu launch.

## Preview

```sh
nix develop
pnpm install --frozen-lockfile
pnpm assets
pnpm dev
```

The desktop preview uses sample inventory and never calls TV services. To manually exercise a
longer app grid:

```sh
pnpm dev --test-extra-apps 120
```

Preview preferences are separate from normal preferences. See [development.md](development.md)
for controls, vector rendering, diagnostics, tests and the source layout.

## Privacy and scope

App/input discovery uses fixed inventory commands through Homebrew Channel's elevated service.
App launches use the native Luna bus with declared permissions. There is no demo inventory
fallback on the TV. Native caller permissions and lifecycle behavior can differ by firmware.

Favorites, hidden apps, settings, weather location and cached readings stay in local JSON files
in an app-owned persistent directory outside the installed payload.
Weather search/forecast requests go to Open-Meteo, with embedded TLS roots and bounded responses.
There is no browser geolocation, IP-based location lookup, telemetry or runtime font download.

Luma is licensed under MIT.
Luma is not affiliated with LG, Apple, Plex or any app brands shown in its UI.
