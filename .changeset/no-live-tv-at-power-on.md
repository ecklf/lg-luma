---
'lg-luma': minor
---

Optionally open Luma instead of the last TV input at power-on and after Back/Exit on webOS 5 and 6. Ordinary Home takeover preserves the TV input policy; `home.sh startup-home on` explicitly enables the read-only last-input override and Back/Exit filter. `startup-home off` or disabling takeover restores the saved HDMI/Live TV input. Restoration parses JSON independently of key order and reloads the live filter before returning control to LG.
