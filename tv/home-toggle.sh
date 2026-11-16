#!/bin/sh
# Embedded in an independent systemd unit; replacing Home may close the requesting app.
set -u
APP=$1
ENABLE=$2
ATTEMPT=$3
STATE=/var/lib/luma-home-toggle
case "$APP" in
  /media/developer/apps/usr/palm/applications/com.ecklf.luma|/media/cryptofs/apps/usr/palm/applications/com.ecklf.luma|/usr/palm/applications/com.ecklf.luma) ;;
  *) exit 1 ;;
esac
case "$ENABLE" in true|false) ;; *) exit 1 ;; esac
case "$ATTEMPT" in ''|*[!0-9a-f]*) exit 1 ;; esac
[ ${#ATTEMPT} = 32 ] || exit 1
[ "$(id -u)" = 0 ] || exit 1
umask 077
[ ! -L "$STATE" ] || exit 1
if [ ! -d "$STATE" ]; then mkdir -m 700 "$STATE" || exit 1; fi
[ "$(stat -c '%u %a' "$STATE")" = '0 700' ] || exit 1
publish() {
  temporary=$(mktemp "$STATE/result.XXXXXX") || exit 1
  printf '{"returnValue":%s}\n' "$1" >"$temporary"
  mv -f "$temporary" "$STATE/result.$ATTEMPT.json"
}
fail() { publish false; exit 1; }
exec 9>"$STATE/lock"
flock -n 9 || fail
trap 'fail' HUP INT TERM
# Logs and results stay in a root-owned directory, not a predictable shared /tmp file.
exec >>"$STATE/toggle.log" 2>&1
if [ "$ENABLE" = true ]; then
  printf 'y\n' | sh "$APP/home.sh" enable || fail
else
  sh /var/lib/luma-home/home.sh disable || fail
fi
status=$("$APP/luma-helper" home status) || fail
for key in enabled mounted bootHook; do
  printf '%s\n' "$status" | grep -q "\"$key\":[[:space:]]*$ENABLE" || fail
done
# Only retire the old boot launch after takeover has succeeded and been verified.
# Failed/unsupported takeovers leave the existing autostart hook untouched.
if [ "$ENABLE" = true ]; then
  sh "$APP/autostart.sh" disable || fail
fi
publish true
