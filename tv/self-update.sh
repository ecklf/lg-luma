#!/bin/sh
# Runs in its own systemd unit so installation can close/restart Luma safely.
# Usage: self-update.sh IPK SHA256 VERSION APP_DIR ATTEMPT
set -u
IPK=$1
SHA=$2
VERSION=$3
APP_DIR=$4
ATTEMPT=$5
STATE=/var/lib/luma-update
HOME_STATE=/var/lib/luma-home
case "$ATTEMPT" in ''|*[!0-9a-f]*) exit 1 ;; esac
[ ${#ATTEMPT} = 32 ] || exit 1
case "$SHA" in ''|*[!0-9a-f]*) exit 1 ;; esac
[ ${#SHA} = 64 ] || exit 1
case "$VERSION" in ''|*[!0-9.]*) exit 1 ;; esac
case "$APP_DIR" in
  /media/developer/apps/usr/palm/applications/com.ecklf.luma|/media/cryptofs/apps/usr/palm/applications/com.ecklf.luma|/usr/palm/applications/com.ecklf.luma) ;;
  *) exit 1 ;;
esac
[ "$(id -u)" = 0 ] || exit 1
umask 077
[ ! -L "$STATE" ] || exit 1
if [ ! -d "$STATE" ]; then mkdir -m 700 "$STATE" || exit 1; fi
[ "$(stat -c '%u %a' "$STATE")" = '0 700' ] || exit 1
RESULT="$STATE/result.$ATTEMPT.json"
OWN_LOCK=false
publish() {
  # Only fixed internal messages reach this function. The random attempt ID is validated above.
  temporary=$(mktemp "$STATE/result.XXXXXX") || exit 1
  printf '{"attempt":"%s","state":"%s","message":"%s"}\n' "$ATTEMPT" "$1" "$2" >"$temporary"
  mv -f "$temporary" "$RESULT"
  if [ "$OWN_LOCK" = true ]; then
    cp "$RESULT" "$STATE/result.json.new" && mv -f "$STATE/result.json.new" "$STATE/result.json"
  fi
}
fail() { publish failed "$1"; exit 1; }
exec 9>"$STATE/lock"
flock -n 9 || fail 'Another update is running; wait for it to finish.'
OWN_LOCK=true
WORK=$(mktemp -d "$STATE/attempt.XXXXXX") || fail 'Cannot create update staging directory.'
LOG="$STATE/update.log"
: >"$LOG"
log() { printf '%s %s\n' "$(date +%T)" "$*" >>"$LOG"; }
cleanup() {
  if [ -n "${SEND:-}" ]; then kill "$SEND" 2>/dev/null || true; wait "$SEND" 2>/dev/null || true; fi
  rm -rf "$WORK"
}
trap cleanup EXIT
trap 'fail "Update interrupted."' HUP INT TERM
publish starting 'Preparing the verified package.'
# Install only the root-owned copy: another process cannot change it after verification.
cp "$IPK" "$WORK/package.ipk" || fail 'Cannot stage the downloaded package.'
IPK="$WORK/package.ipk"
[ "$(sha256sum "$IPK" | cut -d' ' -f1)" = "$SHA" ] || fail 'Package checksum mismatch.'
publish installing 'Installing Luma.'
log "installing $VERSION"
OUT="$WORK/install"
luna-send -i -w 240000 luna://com.webos.appInstallService/dev/install \
  "{\"id\":\"com.ecklf.luma\",\"ipkUrl\":\"$IPK\",\"subscribe\":true}" >"$OUT" 2>&1 </dev/null &
SEND=$!
i=0
while [ $i -lt 240 ]; do
  grep -q '"state": *"installed"' "$OUT" && break
  grep -q -E '"returnValue": *false|"state": *"[^"]*(fail|error|cancel)' "$OUT" && break
  kill -0 "$SEND" 2>/dev/null || break
  i=$((i + 1))
  sleep 1
done
kill "$SEND" 2>/dev/null || true
wait "$SEND" 2>/dev/null || true
SEND=
head -c 4000 "$OUT" >>"$LOG"
grep -q '"state": *"installed"' "$OUT" || fail 'Package installation failed or timed out; see /var/lib/luma-update/update.log.'
installed() {
  node -e 'try { process.stdout.write(JSON.parse(require("fs").readFileSync(process.argv[1], "utf8")).version); } catch (e) { process.exit(1); }' "$APP_DIR/appinfo.json"
}
i=0
while [ "$(installed)" != "$VERSION" ] && [ $i -lt 30 ]; do i=$((i + 1)); sleep 1; done
[ "$(installed)" = "$VERSION" ] || fail 'The installed version could not be verified.'
launch() {
  reply=$(luna-send -n 1 -w 5000 luna://com.webos.applicationManager/launch "{\"id\":\"$1\"}" 2>>"$LOG") || return 1
  printf '%s\n' "$reply" >>"$LOG"
  printf '%s\n' "$reply" | grep -q '"returnValue": *true'
}
HOME_ID=$(node -e 'try { var c = JSON.parse(require("fs").readFileSync(process.argv[1], "utf8")); process.stdout.write(c.homeId || "com.webos.app.home"); } catch (e) { process.stdout.write("com.webos.app.home"); }' "$HOME_STATE/state.json")
case "$HOME_ID" in
  com.webos.app.home|com.webos.app.fullhome) ;;
  *) fail 'The saved Home target is invalid.' ;;
esac
recover_home() {
  # Disable must complete, then independently verify both takeover methods are detached.
  if sh "$HOME_STATE/home.sh" disable >>"$LOG" 2>&1 </dev/null &&
     ! awk '$2 == "/usr/bin/com.webos.app.home" || $2 == "/usr/palm/applications/com.webos.app.home" || $2 == "/usr/bin/com.webos.app.fullhome" || $2 == "/usr/palm/applications/com.webos.app.fullhome" { found=1 } END { exit !found }' /proc/mounts &&
     [ ! -e /var/run/luma-home ]; then
    launch "$HOME_ID" || fail 'LG Home was restored but could not be opened.'
    fail 'Update installed, but Home takeover failed; LG Home restored. Enable Luma Home again to retry.'
  fi
  fail 'Update installed, but Home recovery needs attention; see /var/lib/luma-update/update.log.'
}
if grep -q '"enabled": *true' "$HOME_STATE/state.json" 2>/dev/null; then
  # Use the NEW scripts: refresh copied helpers and staged manifest as well as the executable.
  if ! printf 'y\n' | sh "$APP_DIR/home.sh" enable "$APP_DIR" "$HOME_ID" >>"$LOG" 2>&1; then recover_home; fi
  launch "$HOME_ID" || recover_home
else
  launch com.ecklf.luma || fail 'Update installed, but Luma could not be opened.'
fi
publish success 'Luma was updated and reopened.'
