#!/bin/sh
# Installs a Luma update that Luma downloaded and verified (src/update.rs). Luma starts this as
# root through Homebrew Channel's exec, inside a systemd unit: exec ends its children when the
# call returns, and installing closes Luma itself.
# Usage: self-update.sh IPK SHA256 VERSION APP_DIR
IPK=$1
SHA=$2
VERSION=$3
APP_DIR=$4
LOG=/tmp/luma-update.log
log() { printf '%s %s\n' "$(date +%T)" "$*" >>"$LOG"; }
installed() {
  sed -n 's/.*"version": *"\([^"]*\)".*/\1/p' "$APP_DIR/appinfo.json" 2>/dev/null | head -n 1
}

# The file sat in /tmp between Luma's check and now: verify it again.
if [ "$(sha256sum "$IPK" 2>/dev/null | cut -d' ' -f1)" != "$SHA" ]; then
  log "checksum mismatch; not installing"
  rm -f "$IPK"
  exit 1
fi
log "installing $VERSION"
# Follow the install subscription until appinstalld reports the result, so the log says why an
# install failed. luna-send -i itself only exits at -w.
OUT=/tmp/luma-update.install
luna-send -i -w 300000 luna://com.webos.appInstallService/dev/install \
  "{\"id\":\"com.ecklf.luma\",\"ipkUrl\":\"$IPK\",\"subscribe\":true}" >"$OUT" 2>&1 </dev/null &
SEND=$!
i=0
while [ $i -lt 240 ]; do
  grep -q '"state": *"installed"' "$OUT" && break
  grep -q -E '"returnValue": *false|"state": *"[^"]*(fail|error|cancel)' "$OUT" && break
  i=$((i + 1))
  sleep 1
done
kill "$SEND" 2>/dev/null
tr -d '\n' <"$OUT" | head -c 2000 >>"$LOG"
echo >>"$LOG"
rm -f "$IPK" "$OUT"
# Wait for sam to read the new appinfo as well.
i=0
while [ "$(installed)" != "$VERSION" ] && [ $i -lt 30 ]; do
  i=$((i + 1))
  sleep 1
done
if [ "$(installed)" != "$VERSION" ]; then
  log "$VERSION not installed"
  exit 1
fi
log "installed $VERSION"
# As Home, the mount still holds the replaced executable: apply mounts the new one and reopens
# Home. Otherwise open the normal app again.
if grep -q '"enabled": true' /var/lib/luma-home/state.json 2>/dev/null; then
  sh /var/lib/luma-home/home.sh apply >>"$LOG" 2>&1 </dev/null
  luna-send -n 1 -w 5000 luna://com.webos.applicationManager/launch '{"id":"com.webos.app.home"}' \
    >/dev/null 2>&1 </dev/null
else
  luna-send -n 1 -w 5000 luna://com.webos.applicationManager/launch '{"id":"com.ecklf.luma"}' \
    >/dev/null 2>&1 </dev/null
fi
