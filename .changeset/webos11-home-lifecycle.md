---
'lg-luma': minor
---

Register the Home takeover with SAM on webOS 25/26, so the Home key can relaunch the running Luma instead of SAM killing and restarting it. LG's Home role there is app-ID based, so Luma cannot register on its own connection; as root it holds the registration through `luna-send -a com.webos.app.home` with lifecycle v1 (`registerNativeApp`), without delaying startup. The helper ends with Luma. A registration SAM never accepts leaves the previous behaviour; one that is lost later quits Luma so SAM starts Home afresh. If a relaunch does not bring Luma to the front within 3 s, Luma turns registration off with `/var/lib/luma-home/no-lifecycle` and restarts. Relaunch now also restores and shows the window before raising it, and the staged Home logs to `/tmp/luma-home.log`.
