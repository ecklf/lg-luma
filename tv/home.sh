#!/bin/sh
# Optional native Home replacement. The webosbrew hook has no .sh suffix.
STATE=/var/lib/luma-home
# The cold-boot hook is a symlink to the copied $STATE/home.sh: find luma-helper beside its target.
SCRIPT=$0
if [ -L "$SCRIPT" ]; then
  LINK=$(readlink "$SCRIPT") || exit 1
  case "$LINK" in /*) SCRIPT=$LINK ;; *) SCRIPT=$(dirname -- "$SCRIPT")/$LINK ;; esac
fi
SCRIPT_DIR=$(CDPATH='' cd -- "$(dirname -- "$SCRIPT")" && pwd) || exit 1
COMMAND=${1:-boot}
case "$COMMAND" in
  enable|disable|restore|status|apply|boot|early|pin|unpin|startup-home) ;;
  *) printf '%s\n' 'Usage: home.sh enable [APP_DIR] [HOME_ID] | disable | restore | status | startup-home on|off' >&2; exit 1 ;;
esac
[ "$(id -u)" = 0 ] || { printf '%s\n' 'Run as root.' >&2; exit 1; }
# boot only schedules the copied $STATE/home.sh apply, which checks for the helper itself; the
# early and pin fast paths are shell only.
case "$COMMAND" in
  boot|early|pin) ;;
  *) [ -x "$SCRIPT_DIR/luma-helper" ] ||
    { printf '%s\n' 'The native Luma TV helper is missing.' >&2; exit 1; } ;;
esac
if [ "$COMMAND" = enable ] && grep -q 'release [56]\.' /etc/starfish-release 2>/dev/null; then
  printf '%s\n' \
    'Enabling Luma Home (webOS 5/6) will:' \
    '  1. Close any running normal Luma instance.' \
    "  2. Create $STATE with integration scripts and saved state." \
    "  3. Bind-mount Luma's executable over the selected LG Home executable." \
    "     LG's Home manifest and role stay; Luma registers with SAM itself." \
    '  4. Stop the previous Home process and launch Luma as Home.' \
    '  5. Install /var/lib/webosbrew/init.d/64-luma-home to reapply the mount at cold boot.' \
    '  6. Register the LS2 service com.webos.service.lumahome (luma-early) and a compositor key' \
    '     filter, so a cold boot opens Luma at about 20 s instead of after the Homebrew hooks.' \
    '  Power-on and Back/Exit keep the TV input policy unless startup-home is explicitly enabled.' \
    '' \
    "LG Home's original executable remains underneath the mount." \
    'Home Auto Launch and Quick Start+ retain their current settings.' \
    "Restore LG Home with: sh $STATE/home.sh disable" \
    ''
elif [ "$COMMAND" = enable ]; then
  printf '%s\n' \
    'Enabling Luma Home will:' \
    '  1. Close any running normal Luma instance.' \
    "  2. Create $STATE with integration scripts, saved state and a staged native Home app linked to Luma." \
    "  3. Bind-mount $STATE/app over /usr/palm/applications/com.webos.app.home." \
    '  4. Temporarily block and release Home so SAM rereads its manifest.' \
    '  5. Stop the previous Home process and launch Luma as Home.' \
    '  6. Install /var/lib/webosbrew/init.d/64-luma-home to reapply the mount at cold boot.' \
    '' \
    "LG Home's original files remain underneath the mount." \
    'Home Auto Launch and Quick Start+ retain their current settings.' \
    "Restore LG Home with: sh $STATE/home.sh disable" \
    ''
fi
if [ "$COMMAND" = enable ]; then
  printf '%s\n' "Selected Home target: ${3:-saved target, otherwise com.webos.app.home}"

  while :; do
    printf '%s' 'Apply these changes? [y/n]: '
    if ! IFS= read -r answer; then
      printf '\n%s\n' 'Cancelled. No Home integration changes applied.'
      exit 1
    fi
    case "$answer" in
      y|Y) break ;;
      n|N) printf '%s\n' 'Cancelled. No Home integration changes applied.'; exit 0 ;;
      *) printf '%s\n' 'Please answer y or n.' ;;
    esac
  done
fi
[ ! -L "$STATE" ] || exit 1
if [ ! -d "$STATE" ]; then mkdir -m 755 "$STATE" || exit 1; fi
[ "$(stat -c %u "$STATE")" = 0 ] || exit 1
MARKER=/var/run/luma-home
# bootd opens physicalLastSourceAppId from EIM's lastinput at every power-on: Live TV before Luma.
# A read-only copy naming Home is bind-mounted over it (EIM's in-place rewrite then fails
# harmlessly; EIM keeps the real input in memory). bootd reads the file ~14 s into a cold boot,
# before any hook, so the file underneath names Home too.
LASTINPUT=/var/lib/eim/lastinput
PINNED=$STATE/lastinput
luna_setting() {
  /usr/bin/luna-send -n 1 -w 4000 "luna://com.webos.settingsservice/$1" "$2" </dev/null 2>/dev/null
}
pin_lastinput() {
  [ -f "$STATE/startup-home" ] || return 0
  grep -q " $LASTINPUT " /proc/mounts && return 0
  [ -f "$LASTINPUT" ] && [ -f "$PINNED" ] || return 1
  cmp -s "$PINNED" "$LASTINPUT" || cat "$PINNED" >"$LASTINPUT" || return 1
  if ! { mount --bind "$PINNED" "$LASTINPUT" && mount -o remount,ro,bind "$LASTINPUT"; }; then
    umount "$LASTINPUT" 2>/dev/null
    return 1
  fi
}
# The boot fast path uses the JSON prepared when the owner opts in; restoration uses the native
# helper's JSON parser, never a key-order-dependent text substitution.
if [ "$COMMAND" = pin ]; then pin_lastinput; exit; fi
if [ "$COMMAND" = early ] || [ "$COMMAND" = boot ]; then
  # The Home target chosen at enable (luma-helper home); configurations without one use home.
  HOME_ID=$(sed -n 's/^  "homeId": "\([^"]*\)".*/\1/p' "$STATE/state.json" 2>/dev/null)
  case "${HOME_ID:=com.webos.app.home}" in
    com.webos.app.home|com.webos.app.fullhome) ;;
    *) printf '%s\n' "Unsupported Home target: $HOME_ID" >&2; exit 1 ;;
  esac
  EXE=/usr/bin/$HOME_ID
fi
if [ "$COMMAND" = early ]; then
  # From luma-early, ~18 s into a cold boot (see src/bin/luma-early.rs): the executable method's
  # mount without Node, which would cost seconds under boot load. Opens nothing; the key filter
  # launches Home once this succeeded. Any failure leaves the boot hook below to do it.
  exec 9>"$STATE/lock"
  flock -w 10 9 || exit 1
  config=$(cat "$STATE/state.json" 2>/dev/null) || exit 1
  case "$config" in *'"enabled": true'*) ;; *) exit 1 ;; esac
  case "$config" in *'"method": "executable"'*) ;; *) exit 1 ;; esac
  app=$(printf '%s\n' "$config" | sed -n 's/^  "appDir": "\([^"]*\)".*/\1/p')
  [ -x "$app/luma" ] || exit 1
  if ! grep -q " $EXE " /proc/mounts; then
    # LG's Home, restored from the boot snapshot, still runs LG's program.
    old=$(pgrep -f "^$EXE( |\$)")
    mount --bind "$app/luma" "$EXE" || exit 1
    for pid in $old; do kill "$pid" 2>/dev/null; done
    i=0
    for pid in $old; do
      while [ -d "/proc/$pid" ] && [ $i -lt 20 ]; do i=$((i + 1)); usleep 250000; done
    done
  fi
  [ "$(stat -c '%d %i' "$app/luma")" = "$(stat -c '%d %i' "$EXE")" ] || exit 1
  # Luma registers with SAM under this identity (src/platform.rs), as after the helper's apply.
  printf '%s\n' "$HOME_ID" >"$MARKER-target"
  printf '%s\n' com.ecklf.luma >"$MARKER"
  pin_lastinput || printf '%s early: lastinput not pinned\n' "$(date)" >>"$STATE/boot.log"
  printf '%s early: Luma is Home (uptime %s)\n' "$(date)" "$(cut -d' ' -f1 /proc/uptime)" >>"$STATE/boot.log"
  exit 0
fi
if [ "$COMMAND" = boot ]; then
  # The early helper has already mounted Luma during this boot.
  if [ -f "$MARKER" ] && grep -q " $EXE " /proc/mounts; then exit 0; fi
  (
    sleep 5
    exec sh "$STATE/home.sh" apply
  ) </dev/null >>"$STATE/boot.log" 2>&1 &
  exit 0
fi
exec 9>"$STATE/lock"
flock -n 9 || { printf '%s\n' 'Luma Home integration is already being changed.' >&2; exit 1; }
exec "$SCRIPT_DIR/luma-helper" home "$COMMAND" "${2:-$SCRIPT_DIR}" "${3:-}"
