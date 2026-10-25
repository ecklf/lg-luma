#!/bin/sh
# Optional native Home replacement. The webosbrew hook has no .sh suffix.
STATE=/var/lib/luma-home
SCRIPT_DIR=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd) || exit 1
COMMAND=${1:-boot}
case "$COMMAND" in
  enable|disable|restore|status|apply|boot|early|pin|unpin) ;;
  *) printf '%s\n' 'Usage: home.sh enable [APP_DIR] | disable | restore | status' >&2; exit 1 ;;
esac
[ "$(id -u)" = 0 ] || { printf '%s\n' 'Run as root.' >&2; exit 1; }
[ -x /usr/bin/node ] || { printf '%s\n' 'The TV Node.js runtime is missing.' >&2; exit 1; }
if [ "$COMMAND" = enable ] && grep -q 'release [56]\.' /etc/starfish-release 2>/dev/null; then
  printf '%s\n' \
    'Enabling Luma Home (webOS 5/6) will:' \
    '  1. Close any running normal Luma instance.' \
    "  2. Create $STATE with integration scripts and saved state." \
    "  3. Bind-mount Luma's executable over /usr/bin/com.webos.app.home." \
    "     LG's Home manifest and role stay; Luma registers with SAM itself." \
    '  4. Stop the previous Home process and launch Luma as Home.' \
    '  5. Install /var/lib/webosbrew/init.d/64-luma-home to reapply the mount at cold boot.' \
    '  6. Register the LS2 service com.webos.service.lumahome (luma-early) and a compositor key' \
    '     filter, so a cold boot opens Luma at about 20 s instead of after the Homebrew hooks.' \
    '  7. Pin the last input LG opens at power-on (/var/lib/eim/lastinput) to Home, and point' \
    '     Back/Exit from apps to Home, so Luma opens instead of Live TV.' \
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
EXE=/usr/bin/com.webos.app.home
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
  grep -q " $LASTINPUT " /proc/mounts && return 0
  [ -f "$LASTINPUT" ] || return 1
  sed 's/"physicalLastSourceAppId": *"[^"]*"/"physicalLastSourceAppId":"com.webos.app.home"/' \
    "$LASTINPUT" >"$PINNED.new"
  grep -q '"physicalLastSourceAppId":"com.webos.app.home"' "$PINNED.new" || { rm -f "$PINNED.new"; return 1; }
  mv -f "$PINNED.new" "$PINNED" && chmod 444 "$PINNED" || return 1
  cmp -s "$PINNED" "$LASTINPUT" || cat "$PINNED" >"$LASTINPUT" || return 1
  if ! { mount --bind "$PINNED" "$LASTINPUT" && mount -o remount,ro,bind "$LASTINPUT"; }; then
    umount "$LASTINPUT" 2>/dev/null
    return 1
  fi
}
unpin_lastinput() {
  if grep -q " $LASTINPUT " /proc/mounts; then umount "$LASTINPUT" || umount -l "$LASTINPUT"; fi
  # The input EIM recorded ("appId") again, so the next cold boot is LG's.
  grep -q '"physicalLastSourceAppId": *"com.webos.app.home"' "$LASTINPUT" 2>/dev/null &&
    sed -i 's/\("appId": *"\([^"]*\)".*"physicalLastSourceAppId": *"\)com\.webos\.app\.home"/\1\2"/' "$LASTINPUT"
  # The key filter pointed lastInputApp (Back/Exit) at Home.
  luna_setting getSystemSettings '{"category":"general","keys":["lastInputApp"]}' |
    grep -q '"lastInputApp": *"com.webos.app.home"' &&
    luna_setting setSystemSettings '{"category":"general","settings":{"lastInputApp":"com.webos.app.livetv"}}' >/dev/null
  return 0
}
# Called by home.cjs, which holds the lock already.
if [ "$COMMAND" = pin ]; then pin_lastinput; exit; fi
if [ "$COMMAND" = unpin ]; then unpin_lastinput; exit; fi
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
exec /usr/bin/node "$SCRIPT_DIR/home.cjs" "$COMMAND" "${2:-$SCRIPT_DIR}"
