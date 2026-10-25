---
'lg-luma': minor
---

Open Luma instead of Live TV at power-on and after Back/Exit on webOS 5 and 6. `home.sh enable` pins LG's last-input entry (`/var/lib/eim/lastinput`) to Home with a read-only bind mount and in the file underneath, which LG reads before any hook in a cold boot, and the key filter points `lastInputApp` at Home while an app is in front. The cold-boot hook opens Home over an empty screen, and `home.sh disable` restores the recorded input.
