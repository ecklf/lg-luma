---
"lg-luma": minor
---

Add custom webhook tiles: a home-screen tile that fires a one-way HTTP
request instead of launching an app.

Configured via a new `custom_tiles` array in `preferences.json`, each entry
`{ "id": "custom.<slug>", "title": "...", "url": "..." }`. The id must
start with `custom.`.

The request runs off the SDL thread with a 5s timeout, no redirects and no
proxy. Only success or failure is reported via a toast; the response body
is never read or shown.
