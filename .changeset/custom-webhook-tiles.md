---
"lg-luma": minor
---

Add custom webhook tiles: a home-screen tile that fires a one-way HTTP
request instead of launching an app. Configured in `preferences.json` under
a new `custom_tiles` array (`{ "id": "custom.<slug>", "title": "...", "url": "..." }`,
id must start with `custom.`). The request runs off the SDL thread with a
5s timeout, no redirects and no proxy, and only success/failure is reported
via a toast - the response body is never read or shown.
