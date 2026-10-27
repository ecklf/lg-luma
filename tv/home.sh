#!/bin/sh
# Optional native Home replacement. The webosbrew hook has no .sh suffix.
STATE=/var/lib/luma-home
SCRIPT_DIR=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd) || exit 1
COMMAND=${1:-boot}
case "$COMMAND" in
  enable|disable|restore|status|apply|boot) ;;
  *) printf '%s\n' 'Usage: home.sh enable [APP_DIR] [HOME_ID] | disable | restore | status' >&2; exit 1 ;;
esac
[ "$(id -u)" = 0 ] || { printf '%s\n' 'Run as root.' >&2; exit 1; }
[ -x /usr/bin/node ] || { printf '%s\n' 'The TV Node.js runtime is missing.' >&2; exit 1; }
if [ "$COMMAND" = enable ] && grep -q 'release [56]\.' /etc/starfish-release 2>/dev/null; then
  printf '%s\n' \
    'Enabling Luma Home (webOS 5/6) will:' \
    '  1. Close any running normal Luma instance.' \
    "  2. Create $STATE with integration scripts and saved state." \
    "  3. Bind-mount Luma's executable over the selected LG Home executable." \
    "     LG's Home manifest and role stay; Luma registers with SAM itself." \
    '  4. Stop the previous Home process and launch Luma as Home.' \
    '  5. Install /var/lib/webosbrew/init.d/64-luma-home to reapply the mount at cold boot.' \
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
if [ "$COMMAND" = boot ]; then
  (
    sleep 5
    exec sh "$STATE/home.sh" apply
  ) </dev/null >>"$STATE/boot.log" 2>&1 &
  exit 0
fi
exec 9>"$STATE/lock"
flock -n 9 || { printf '%s\n' 'Luma Home integration is already being changed.' >&2; exit 1; }
exec /usr/bin/node "$SCRIPT_DIR/home.cjs" "$COMMAND" "${2:-$SCRIPT_DIR}" "${3:-}"
