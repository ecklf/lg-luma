---
'lg-luma': patch
---

Fix Home takeover enable failing with "no app matched by pid" by closing the normal Luma instance through the app-ID endpoint. Allow takeover to continue if that instance already exited during the close request.
