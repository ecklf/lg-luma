# Luma widgets from the store

Niche Home widgets are not compiled into Luma. They are installed from a signed catalog as
**data**: a manifest that says where the widget's data comes from, how to turn it into Luma's
widget document, and which of Luma's fixed templates draws it. Nothing downloaded is ever run as
code on the TV; the only logic in a widget is a [jq](https://jqlang.org) transform, a pure
function from JSON to JSON, run with hard CPU, memory and size limits.

This page is the reference for the runtime, for the people reviewing the catalog and for widget
authors. The esports widget is the first widget expressed this way (section 8).

## 1. Why, and what it is not

A niche widget compiled into Luma, such as esports scores (PandaScore polling, a shared feed,
lineups, a key entry, scoreboard cards and a match dialog: about 4,700 lines of Rust in a fork),
would be carried by every build and need a release for every change. The same would hold for
every next one. A store keeps them out: Luma has a small generic runtime and a fixed set of
templates, and a widget is a few kilobytes of JSON and jq that can be updated without a Luma
release.

Constraints that shaped the design:

- **Free to host, no server.** The catalog and shared feeds are static files on GitHub Pages,
  built by GitHub Actions. No widget has a backend of ours.
- **Keys stay on the TV.** A personal API key (the user's own PandaScore key for live scores) is
  stored on the TV and sent only to the API it belongs to, straight from the TV. A proxy was
  rejected: every user's key would pass through it, and a shared free quota runs out quickly.
- **No downloaded code.** Native plugins (`.so`) were rejected: arbitrary code on a rooted TV, no
  stable Rust ABI, one build per Luma version and CPU. WebAssembly may come later; it is not part
  of this design.
- **Luma's look.** Widgets are drawn by Luma's own templates following [design.md](design.md);
  a widget cannot bring sizes, colours, fonts or layout of its own.

Not in this design: glance widgets (weather and crypto stay built in), POST requests or OAuth
sign-in flows, push or WebSocket data, widgets that launch apps or call Luna services, custom
drawing or animation, side-loading unsigned widgets on the TV.

## 2. Overview

```
catalog on GitHub Pages                         the TV
──────────────────────                          ──────
index.json + index.json.sig ── ed25519 check ─► store list
manifests/<id>-<version>.json ─ SHA-256 check ─► <preferences dir>/widgets/<id>.json

every refresh of a widget on Home (widget worker thread):
  source ─► requests (GET, HTTPS, the manifest's origins only, secret in a header)
         ─► replies ─► jq transform (separate process, limited) ─► widget document
SDL thread:
  widget document ─► template: row header + cards (and a dialog for the focused card)
```

Words used below:

| Word          | Meaning                                                                                            |
| ------------- | -------------------------------------------------------------------------------------------------- |
| **Manifest**  | One JSON file describing a widget: identity, origins, settings, sources, template.                 |
| **Widget**    | An installed manifest.                                                                             |
| **Instance**  | One block of a widget on Home, with its own settings. A widget may allow several.                  |
| **Source**    | A way to get the widget's data: a chain of requests plus a transform. The first that applies runs. |
| **Step**      | One request of a source, possibly fanned out over a setting's values.                              |
| **Transform** | A jq program turning the replies into a widget document.                                           |
| **Document**  | Luma's widget format: the items a widget shows, in template terms (section 4).                     |
| **Template**  | Luma's fixed renderers: three row cards and one dialog (section 5).                                |

## 3. The manifest

A manifest is one JSON file of at most 256 KiB. Unknown fields are an error, so a typo is caught
when the widget is checked rather than silently ignored.

```json
{
  "schema": 1,
  "id": "esports",
  "version": "1.0.0",
  "luma": "2.9.0",
  "name": { "en": "Esports", "ru": "Киберспорт" },
  "description": { "en": "Live and upcoming matches of eight disciplines.", "ru": "…" },
  "authors": ["eluz1ve"],
  "icon": "gaming",
  "origins": [
    "https://eluz1ve.github.io",
    "https://api.pandascore.co",
    "https://cdn-api.pandascore.co"
  ],
  "size": { "default": [6, 2], "min": [6, 2], "max": [12, 3] },
  "instances": 4,
  "card": "scoreboard",
  "strings": { "map": { "en": "Map {number}", "ru": "Карта {number}" } },
  "settings": [ … ],
  "library": "def team: …;",
  "sources": [ … ]
}
```

| Field                 | Rules                                                                                                                                                                                      |
| --------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `schema`              | `1`. Luma refuses manifests of a schema it does not know.                                                                                                                                  |
| `id`                  | `[a-z][a-z0-9-]{1,31}`, unique in the catalog, never reused for another widget.                                                                                                            |
| `version`             | `MAJOR.MINOR.PATCH`. The catalog only ever offers one version per id.                                                                                                                      |
| `luma`                | The oldest Luma that can run it. Older Lumas list the widget as "Needs Luma 2.10" and do not install it.                                                                                   |
| `name`, `description` | Localised text (below). Name at most 24 characters, description at most 140.                                                                                                               |
| `authors`             | GitHub logins; shown in the store and used for the catalog's code owners.                                                                                                                  |
| `icon`                | One of Luma's bundled UI icons (`assets/ui-icons`, e.g. `gaming`, `tv`, `globe`, `clock`, `sparkles`). The store shows no remote images.                                                   |
| `origins`             | 1–8 origins, each exactly `https://host` or `https://host:port`: no path, no wildcard, no IP literals, no `localhost`. Every request and every image of the widget must be on one of them. |
| `size`                | Home cells as `[w, h]`: `default`, `min`, `max`. Width 4–12, height 1–3; a height of 1 is a compact row without a header ([design.md](design.md#rows)).                                    |
| `instances`           | How many blocks of this widget Home may hold, 1–4. Default 1.                                                                                                                              |
| `card`                | The row's card template: `scoreboard`, `media` or `ticker` (section 5).                                                                                                                    |
| `strings`             | Messages for the transforms: id → localised text with `{placeholders}`; at most 200.                                                                                                       |
| `settings`            | At most 12 (below).                                                                                                                                                                        |
| `library`             | Optional jq definitions (`def …;`) put in front of every program of the widget, so its sources share code. At most 64 KiB.                                                                 |
| `sources`             | 1–4 sources (section 3.3).                                                                                                                                                                 |

**Localised text** is an object from Luma's language codes (`en`, `de`, `zh`, `es`, `pt`, `it`,
`ru`) to strings, the same shape as `assets/messages.json`. `en` is required; a missing language
falls back to it.

### 3.1 Settings

Each widget instance has its own values for every setting except secrets, which belong to the
widget and are shared by all its instances (as today's PandaScore key is shared by every esports
widget).

| `type`    | Extra fields                                                              | Value seen by transforms         | On the TV                                             |
| --------- | ------------------------------------------------------------------------- | -------------------------------- | ----------------------------------------------------- |
| `toggle`  | `default` (bool)                                                          | `true` / `false`                 | a row showing On/Off; OK flips it                     |
| `choice`  | `options` (`[{value, label}]`, 2–16), `default`                           | the chosen value                 | a row with the current label; OK opens a list         |
| `choices` | `options`, `default` (list), `min` (default 1), `max`                     | list of values, in options order | a row listing the labels; OK opens a checklist        |
| `text`    | `default`, `max_length` (≤ 64), optional `pattern`                        | the text                         | a row; OK opens the on-screen keyboard                |
| `secret`  | `send_to` (origins), `max_length` (≤ 256, default 128), `pattern`, `hint` | `true` when set, else `false`    | three rows: type it, enter it from a phone, forget it |

Every setting has `id` (`[a-z][a-z0-9_]{0,31}`) and `label` (localised). Values are checked
against the manifest when read: an unknown choice falls back to the default, so a widget update
that drops an option cannot break an instance.

```json
"settings": [
  { "id": "games", "type": "choices", "label": { "en": "Disciplines", "ru": "Дисциплины" },
    "options": [ { "value": "csgo", "label": { "en": "Counter-Strike 2" } }, … ],
    "default": ["csgo"] },
  { "id": "major_only", "type": "toggle", "label": { "en": "Major events only" }, "default": true },
  { "id": "key", "type": "secret", "label": { "en": "PandaScore key" },
    "hint": { "en": "Free at pandascore.co. With your key, live scores update every 30 s." },
    "send_to": ["https://api.pandascore.co"], "pattern": "^[A-Za-z0-9._-]{16,128}$" }
]
```

### 3.2 Secrets

- A secret is stored in `secrets.json` next to `preferences.json` (mode 0600), as
  `{"<widget id>": {"<setting id>": "<value>"}}`. It is never written to `preferences.json`, never
  logged and never shown again in full (the settings row shows `•••• abcd`).
- It can be placed only in **request headers** (`{secret.<id>}`), only of requests to an origin
  in its `send_to`, which must be one of the widget's origins. It cannot appear in a URL, so it
  never ends up in a log line or a server's access log.
- Transforms never see it: `$settings.<id>` of a secret is just `true` or `false`.
- It is entered with the remote's on-screen keyboard or from a phone: a one-time page served by
  `src/phone_key.rs` and opened by a QR code (its title and field are the setting's label, its
  check is the setting's `pattern` and `max_length`). The page is served on the home network only while
  its QR dialog is open.
- Changing or forgetting it drops the widget's cached replies and refreshes its instances.
  Removing the widget deletes its secrets.

### 3.3 Sources

```json
"sources": [
  { "id": "shared", "when": { "no_secret": "key" },
    "refresh": { "every": 300, "min": 300 },
    "requests": [
      { "id": "feed", "for_each": "games",
        "url": "https://eluz1ve.github.io/luma-feeds/esports/{item}.json" } ],
    "transform": "… jq …" },
  { "id": "live", "when": { "secret": "key" },
    "refresh": { "every": 300, "min": 30 },
    "requests": [ … section 8 … ],
    "transform": "… jq …" }
]
```

| Field       | Rules                                                                                                                                                                                                |
| ----------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `id`        | `[a-z][a-z0-9_]{0,31}`; transforms see it as `$source`.                                                                                                                                              |
| `when`      | `{"secret": id}` (that secret is set), `{"no_secret": id}`, or absent (always). The first source whose `when` holds is used.                                                                         |
| `refresh`   | `every`: seconds between refreshes unless the document asks otherwise; `min`: the shortest interval the document may ask for. Luma never goes under 30 s or over 3600 s, whatever the manifest says. |
| `requests`  | 0–8 steps, run in order.                                                                                                                                                                             |
| `transform` | The jq program (section 3.5), at most 64 KiB. In the catalog's source tree it (and `library`) may be `{"file": "live.jq"}`; packing inlines it.                                                      |

### 3.4 Steps

```json
{
  "id": "teams",
  "for_each": "games",
  "max_age": 1800,
  "required": false,
  "vars": {
    "ids": "[shown($item)[].opponents[]?.opponent.id] | unique | map(tostring) | join(\",\") | select(. != \"\")"
  },
  "url": "https://api.pandascore.co/{item}/teams?filter%5Bid%5D={var.ids}&per_page=100",
  "headers": { "Authorization": "Bearer {secret.key}" }
}
```

| Field      | Rules                                                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| ---------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `id`       | The reply's name in the transform's input.                                                                                                                                                                                                                                                                                                                                                                                                                             |
| `for_each` | Optional id of a `choices` setting: one request per chosen value, the value is `{item}` / `$item`, and the reply becomes an object keyed by the value (`{"csgo": …, "dota2": …}`).                                                                                                                                                                                                                                                                                     |
| `url`      | A template. Its origin part (up to the first `/` after the host) must be literal text and one of `origins`. Placeholders: `{item}`, `{settings.<id>}` (a toggle is `true`/`false`, a list is its values joined by commas), `{var.<name>}`. Substituted values are percent-encoded (only RFC 3986 unreserved characters stay as they are), so a value can never change the host, the path's structure or add a parameter.                                               |
| `headers`  | At most 8. Values may use the same placeholders plus `{secret.<id>}`; a value containing a control character makes the request fail. `User-Agent: LumaNative` and `Accept: application/json` are set unless given.                                                                                                                                                                                                                                                     |
| `vars`     | Named jq expressions evaluated before the request (input: the replies so far; `$item`, `$settings`, `$t`, `$now`, `$lang`, `$source` as for transforms; the `library` is in front, so `shown` above is the widget's own definition). Each must give one string or number of at most 1 KiB. If one gives nothing or `null`, the step is skipped and its reply is `null` (e.g. no teams to look up). A step's vars for all its items are evaluated in one transform run. |
| `max_age`  | Seconds a reply is reused for the same URL and headers, across refreshes and across instances. Default: the source's `refresh.every`; `0` always fetches.                                                                                                                                                                                                                                                                                                              |
| `required` | Default `true`: a failed request fails the refresh. `false`: its reply is `null` and the transform copes.                                                                                                                                                                                                                                                                                                                                                              |
| `limit`    | Largest reply in bytes; default and maximum 1 MiB.                                                                                                                                                                                                                                                                                                                                                                                                                     |

Every request is a `GET` over HTTPS with Luma's existing rules for network access (as for
weather and today's feeds in `src/feeds.rs`): rustls, no proxy, no redirects, 10 s timeout,
bounded replies, no cookies. The URL is checked against `origins` again after substitution. The
reply must be JSON. A refresh may make at most 32 requests: the sum of every step's fan-out,
checked before anything is sent, so a source that could make more fails with "too many requests"
rather than sending some. (Esports with all eight disciplines needs four steps × eight.)

Failures map to what the row shows:

| What happened                                                       | Result                                                                     |
| ------------------------------------------------------------------- | -------------------------------------------------------------------------- |
| 401 or 403 from a request that carried a secret                     | **key refused**: the row says the key was refused, settings can fix it     |
| 429                                                                 | **busy**: keep the last document, wait at least 5 minutes                  |
| network error, timeout, other status, not JSON, too large           | **unavailable**: keep the last document; retry after 1, 2, 4… up to 15 min |
| the transform failed, ran out of limits or gave an invalid document | **broken**: as unavailable, and logged with the widget's id and version    |

### 3.5 Transforms

Transforms and `vars` are jq, as implemented by [jaq](https://github.com/01mf02/jaq) 3
(`jaq-core`, `jaq-std`, `jaq-json`). jaq is a pure interpreter: it has no file, network or
process access. Luma loads its standard library without the filters that read the outside world
or produce side effects: `input`, `inputs`, `$ENV`/`env`, `now`, `debug`, `stderr`, `halt`,
`halt_error`, `input_filename`, `include` and `import`. A program using one of them is refused
when the widget is checked or installed. Regular expressions, dates (`fromdateiso8601`,
`strftime`, …), maths and `@uri`/`@base64`/`@html` formats are available.

The input is the replies, by step id (`.feed.csgo`, `.teams.dota2`, `.running` …); a skipped or
failed optional step is `null`. Variables:

| Variable    | Value                                                                  |
| ----------- | ---------------------------------------------------------------------- |
| `$settings` | The instance's settings by id (secrets as `true`/`false`).             |
| `$t`        | The manifest's `strings` in the current language, by id.               |
| `$lang`     | The current language code, e.g. `ru`.                                  |
| `$now`      | Unix seconds when the refresh started (`now` itself is not available). |
| `$source`   | The id of the source that ran.                                         |
| `$item`     | In `vars` of a fanned-out step: its value.                             |

Luma adds one definition to the standard library, `fmt($args)`, which fills a string's
`{placeholders}`: `$t.map | fmt({number: 3})` gives "Map 3" ("Карта 3" in Russian).

A transform must produce exactly one output: the document (section 4). The same input always
gives the same output, so fixtures test a widget completely. Changing Luma's language reruns the
transform on the cached replies; it does not refetch anything.

**Limits.** A buggy jq program can loop forever, recurse until the stack overflows, or build a
huge array. Run inside Luma, any of these could freeze or kill the launcher, so transforms and
`vars` run in a child process: Luma starts its own executable with a hidden
`--widget-transform` flag (before SDL is initialised), writes the program and input to its stdin
and reads the result from its stdout.

| Limit                                  | Value                                                                    |
| -------------------------------------- | ------------------------------------------------------------------------ |
| CPU time (`RLIMIT_CPU`)                | 2 s                                                                      |
| Address space (`RLIMIT_AS`, on the TV) | 256 MiB                                                                  |
| Wall clock                             | 3 s, then the child is killed                                            |
| Input                                  | the replies, at most 4 MiB in all                                        |
| Output                                 | at most 512 KiB of JSON                                                  |
| Program                                | `library` + transform at most 128 KiB; parsed and checked at install too |

A child that is killed, crashes or exceeds a limit fails that refresh (**broken**); Luma itself
is never affected. On the desktop, where `RLIMIT_AS` is not enforced, the wall clock still
applies. Starting the child costs a few tens of milliseconds on the TV; at refresh intervals of
30 s and more this is negligible.

## 4. The widget document

What every transform returns: plain data in the templates' terms. Luma parses it on the worker
thread into typed structures, drops what it does not know and truncates what is too long, so a
document can never make the renderer misbehave.

```json
{
  "title": "Counter-Strike 2",
  "updated": 1793021218,
  "refresh": 30,
  "empty": "No major matches right now",
  "items": [ { "id": "1753707", "card": { … }, "dialog": { … } } ]
}
```

| Field     | Meaning                                                                                                                                                                                          |
| --------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `title`   | Optional; follows the widget's name in the row header: "Esports · Counter-Strike 2".                                                                                                             |
| `updated` | Optional Unix seconds when the data was built (a shared feed). The header then shows "Data as of 17:46", and elapsed times count up to it instead of to now.                                     |
| `refresh` | Optional seconds until the next refresh, clamped to the source's `refresh.min` … 3600.                                                                                                           |
| `empty`   | Text for the row's ghost card when there are no items.                                                                                                                                           |
| `items`   | At most 50. `id` (at most 64 characters, unique) keeps focus and an open dialog on the same item across refreshes. `card` is the manifest's card template; `dialog` is optional and OK opens it. |

**Text.** Wherever a field holds text it is a plain string (no markup, at most 200 characters,
control characters removed) or one of these values that Luma formats in the TV's language and
time zone:

| Value                       | Shows                                                               |
| --------------------------- | ------------------------------------------------------------------- |
| `{"start": "<RFC 3339>"}`   | a start time with its day: "18:00", "Tomorrow 18:00", "27.10 18:00" |
| `{"elapsed": "<RFC 3339>"}` | whole minutes since then, up to the data's time: "12 min"           |
| `{"as_of": <Unix seconds>}` | "Data as of 17:46"                                                  |

**Images** are URLs on one of the widget's origins, at most 512 characters; anything else is
ignored and the template draws its placeholder. Luma fetches them on the worker (2 MiB at most),
decodes and shrinks them there (logos and photos to 160 × 160, posters to 300 × 450) and uploads
them on the SDL thread, as it does for today's logos and posters.

**Tones** colour a few marks from a fixed set mapped to [design.md](design.md#colour)'s tokens:
`live` (`LIVE` red), `gold` (`GOLD`), `up` (`UP`), `down` (`DOWN`); absent is neutral. A widget
cannot choose any other colour. Team and person colours (the glow behind a logo, an avatar
without a photo) come from Luma: the loaded image's own accent, else `name_tint` of the name.

**Caps.** 8 chips, 7 tiles, 2 lists of 12 rows with 4 extra lines each, 2 sides; anything beyond
is dropped.

## 5. Templates

Templates are Luma's code, built from the primitives [design.md](design.md) already defines:
`row_header`, `track_cards`, `card`, `ghost`, `chip`, `live_badge`, `emblem` and `fit`. A widget
picks one card template for its row; any item may also carry a dialog.

### 5.1 The row

Every store widget is a **row** in design.md's sense:

- Header: the widget's name (and the document's `title`) in `section()`, then meta at the right:
  which cards show ("3–5 / 12") and "Data as of …" when the document has `updated`. A compact
  row (one cell tall) has no header.
- Track: the cards in a carousel, `TILE_W` or `WIDE_W` wide depending on the template, Left and
  Right stepping one card at a time; one control per visible card, registered with
  `State::row_of` so carousel stepping and reveal work as for every other row.
- Nothing yet: one ghost card with "Loading…", "Can't load right now", "The key was refused —
  check it in the widget's settings" (naming the secret's label), "This widget stopped working",
  "Enter your PandaScore key in the widget's settings" (when no source applies because a
  secret is missing), or the document's `empty`.

### 5.2 `scoreboard` — a wide card for a contest between two sides

```json
"card": {
  "live": true,
  "status": { "start": "2026-10-26T18:00:00Z" },
  "logo": "https://cdn-api.pandascore.co/images/league/image/5442/….png",
  "caption": "PGL Masters Bucharest 2026",
  "sides": [
    { "name": "9z", "short": "9z", "image": "https://cdn-api.pandascore.co/…/9z.png", "score": "1", "lead": true },
    { "name": "TYLOO", "short": "TYLOO", "image": "…", "score": "1", "lead": true }
  ],
  "footer": "BO3 · Group Stage",
  "progress": { "label": "Map 3 · Mirage", "value": "7:5", "steps": ["done", "done", "live"] }
}
```

Top line: LIVE badge when `live` (and a `LIVE` 32 % hairline on the card), else `status`; then
`logo` (24 px) and `caption` in what is left. Middle: the two `sides`, each a logo (or a tinted
placeholder), `name` when it fits else `short`, and `score` at the right in `figure()`, white
when `lead` else 42 %; a side that is `null` is "TBD". The score column exists only if a side has
a `score`. Bottom: `progress` when present (its label, its `value` in bold, a dot per step at
the right: `done` filled, `live` red, `next` hollow, at most 7), else `footer`. A compact row
draws the same parts tighter, as today's compact match card.

### 5.3 `media` — a wide card with a poster

```json
"card": { "image": "https://static.tvmaze.com/….jpg", "title": "Andor", "subtitle": "Season 2 · 22.04", "live": false }
```

The poster (2:3) flush with the card's left edge, a tinted placeholder with the `tv` icon without
one; `title` in `card_title()` on up to two lines (three on a tall card); `subtitle` in `body()`
at the bottom; `live` adds the red hairline and a LIVE badge over the poster. This is today's
premieres card.

### 5.4 `ticker` — a tile for a figure that changes

```json
"card": { "label": "BTC", "value": "67 120", "change": { "text": "+2.4%", "tone": "up" }, "detail": "65 900 – 67 800" }
```

`label` in `caps()`, `value` in `figure(40)`, `change` in `body()` with its tone, `detail` in
`meta()`. A `TILE_W` card.

### 5.5 The dialog

The item's details in the dialog panel the match dialog uses today (1328 × 976, the same
surface, close button and Back). Sections come in this order; each is optional and a missing one
gives its height to the lists:

```json
"dialog": {
  "header": {
    "logo": "…/league.png",
    "title": "PGL Masters Bucharest 2026",
    "subtitle": "Counter-Strike 2 · Group Stage",
    "chips": [ { "text": "BO3" }, { "text": "Tier A" }, { "text": "LAN" }, { "text": "$1 000 000", "tone": "gold" } ]
  },
  "hero": {
    "live": true,
    "sides": [
      { "name": "9z", "image": "…", "caption": "AR", "score": "1", "lead": true },
      { "name": "TYLOO", "image": "…", "caption": "CN", "score": "1", "lead": true }
    ],
    "lines": [ "Map 3 · Mirage · 7:5", { "as_of": 1793021218 } ]
  },
  "tiles": {
    "title": "Maps",
    "slots": 3,
    "items": [
      { "number": "01", "title": "Inferno", "note": "Pick: 9z", "state": "done", "value": "13:11",
        "result": { "image": "…/9z.png", "name": "9z", "text": "9z · 55 min" } },
      { "number": "03", "title": "Mirage", "note": "Decider", "state": "live", "value": "7:5",
        "result": { "text": { "elapsed": "2026-10-26T13:02:50Z" } } }
    ]
  },
  "lists": [
    { "title": "9z lineup", "image": "…/9z.png", "name": "9z",
      "rows": [ { "image": "…/meyern.png", "title": "meyern", "subtitle": "Ignacio Meyer", "meta": "AR · AWP" } ],
      "extra": [ { "label": "Bench", "text": "dgt · luchov" }, { "label": "Coach", "text": "rox" } ],
      "empty": "No lineup yet" },
    { … }
  ],
  "footer": {
    "label": "Streams",
    "text": "EN Twitch (official) · RU Twitch",
    "credit": "Maps: Liquipedia (CC-BY-SA)",
    "note": "Live scores every 30 s with your key."
  }
}
```

- **header**: `logo` 58 px, `title` 28/650, `subtitle` dim, then `chips` (neutral, or `gold`).
- **hero**: the two sides in columns (a `null` side is "TBD"), each a 112 px logo with a glow in
  its colour, its `name` and a `caption` (country, region). Between them the big score `1 : 1` (`lead` white, otherwise
  42 %) when a side has a `score`, else "VS". When `live`, a red-tinted panel, the LIVE badge,
  `lines[0]` in 16/600 and `lines[1]` small and dim; otherwise `lines[0]` in 24/650 (a start
  time) and `lines[1]` small.
- **tiles**: `title` in `caps()`, then `slots` equal tiles in one line (1–7; `items` fill them
  by order, the rest are empty tiles). A tile: `title` (or `placeholder`, dimmed), a faint
  `number` at the right of a wide tile, `note` under the title, and a result row: `value` at the
  right in bold; at the left LIVE and `result.text` for a `live` tile, else `result`'s emblem
  (its image or a tinted initial from `name`) and `text`, else "—". `state` styles the tile:
  `live` red, `done` lifted, `next` faint.
- **lists**: one or two columns side by side. Each: a small logo and `title`, then up to 12
  `rows` (avatar from `image` or a tinted initial, `title` bold, `subtitle` dim after it, `meta`
  at the right in caps), sharing the column's height at most 40 px each; then the `extra` lines
  together on one dim line under a hairline ("BENCH dgt · luchov COACH rox"). `empty` replaces
  the rows when there are none.
- **footer**: `label:` and `text` on one line, `credit` at its right, `note` on up to two dim
  lines below.

These sections are the parts of a match dialog, named for what they are rather than for
esports, so other contests (football, a chess round) and lists of people fit them too. Section 8
maps every element of the esports widget onto them.

## 6. The catalog and trust

### 6.1 Files

The catalog is a static site: `https://eluz1ve.github.io/luma-widgets/` (a public repository
`eluz1ve/luma-widgets`, section 9). Luma has its URL compiled in.

```
index.json
index.json.sig                 ed25519 signature of index.json's exact bytes, base64
manifests/esports-1.0.0.json
```

```json
{
  "schema": 1,
  "sequence": 42,
  "published": "2026-10-26T12:00:00Z",
  "widgets": [
    { "id": "esports", "version": "1.0.0", "luma": "2.9.0",
      "name": { … }, "description": { … }, "icon": "gaming", "authors": ["eluz1ve"],
      "origins": [ … ], "secrets": [ { "label": { … }, "send_to": ["https://api.pandascore.co"] } ],
      "url": "manifests/esports-1.0.0.json", "size": 23456, "sha256": "9f2c…" }
  ]
}
```

The index repeats what the store shows and what the user consents to (name, description, icon,
origins, where secrets go), so the store list needs one request and nothing is installed just to
be displayed.

### 6.2 What Luma checks

1. `index.json` (at most 256 KiB) and its signature are fetched over HTTPS with the usual network
   rules. The signature must verify with one of the public keys compiled into Luma (a list, so a
   key can be rotated: a new key ships in a release before the old one stops being used).
2. `sequence` must not be lower than the highest one this TV has accepted (kept in preferences),
   so an old signed index cannot be replayed to bring back a withdrawn version.
3. A manifest is fetched from the catalog's own origin by its relative `url`; its size and SHA-256
   must match the index entry, and it must declare the same id, version, origins and secret
   destinations as the entry.
4. The manifest is validated in full: schema, ids, origins, URL templates, settings, sizes, and
   every jq program parsed without the forbidden filters.
5. It is written atomically to `<preferences dir>/widgets/<id>.json`; preferences record its id,
   version and SHA-256. At start Luma validates installed manifests again; one that fails is
   shown as broken instead of running.

Any failure leaves what was installed untouched. A catalog that cannot be verified shows "The
widget catalog could not be verified" and offers nothing to install.

### 6.3 Updates and removal

Luma fetches the index when the store opens, and once a day while a store widget is on Home.

- A newer version that needs **no new access** (same or fewer origins, same or fewer secret
  destinations) is installed automatically. Settings are kept by id; new settings get their
  defaults; settings that disappeared are dropped.
- A newer version that **adds** an origin or a secret destination waits in the store as "Update
  — needs access to …" until the user accepts it. The old version keeps running.
- A widget the catalog no longer lists keeps running, marked "No longer in the catalog".
- **Remove** in the store deletes the widget's blocks from Home, their settings, its secrets and
  its manifest. Removing one block from Home only deletes that instance.

### 6.4 The security model in one table

| Threat                                              | What stops it                                                                                                   |
| --------------------------------------------------- | --------------------------------------------------------------------------------------------------------------- |
| Someone serves a fake catalog or manifest           | ed25519 signature over the index, SHA-256 of each manifest, keys compiled into Luma                             |
| An old catalog is replayed                          | `sequence` never goes down                                                                                      |
| A widget runs code on the TV                        | There is no code: data, fixed templates, and jq without IO in a separate limited process                        |
| A widget freezes or crashes Luma                    | Network and transforms are off the SDL thread; transforms are a killable child with CPU, memory and time limits |
| A widget talks to servers the user didn't agree to  | Origins are shown at install, every URL and image is checked against them after substitution, no redirects      |
| A widget phishes a key and sends it elsewhere       | Secrets go only into headers to their `send_to` origins, shown at install; transforms never see them            |
| A widget tracks the user through a new origin later | Updates adding origins or secret destinations need the user's consent                                           |
| Huge replies or documents exhaust memory            | Reply, image, input, output, document and count limits                                                          |
| Misleading or offensive content                     | Catalog review (section 9); a widget has no colours, fonts or layout of its own                                 |

What remains: a widget sees what its origins send and tells its origins the TV's address, like
any app; the review is where that is judged. The phone page for secrets uses plain HTTP on the
home network for the minute it is open, as today.

## 7. Luma's side

### 7.1 Home, settings and the store

- **Add widget** lists the built-in kinds, then every installed widget that may have another
  instance, then **Widget store…** last.
- **Widget store** opens in the dialog panel: a list of the catalog's widgets as rows (the icon
  on a tinted square, the name, the description on one line, and a chip at the right:
  "Installed", "Update", "Needs Luma 2.10"). Up and Down move, OK opens one.
- **A widget's page**: its name, authors, version, description, **Connects to** with each origin
  as a chip, for each secret "Sends your PandaScore key only to api.pandascore.co", then
  **Install and add to Home** (or **Add to Home**, **Update**, **Remove**). Back returns to the
  list. Loading, offline and unverified catalog states use the same ghost card as rows.
- A store widget's block menu has **Settings**: a settings page built from the manifest's
  settings (section 3.1), with the secret's `hint` under its rows, in the layout of Luma's other
  settings pages.
- Home blocks of store widgets are `BlockKind::Widget(slot)`: a slot (0–15) names one instance
  and maps, in preferences, to its widget id and settings. Their size limits come from the
  manifest. Focus targets are `Target::WidgetItem(slot, index)`; the open dialog remembers the
  item's `id`.

Preferences gain:

```json
"widgets": {
  "catalog_sequence": 42,
  "installed": { "esports": { "version": "1.0.0", "sha256": "9f2c…" } },
  "slots": { "0": { "widget": "esports", "settings": { "games": ["csgo", "valorant"], "major_only": true } } }
}
```

### 7.2 Threads and scheduling

- One **widget worker** thread owns the HTTP agent, the reply cache, catalog requests and the
  transform children. It takes `Refresh { slot, generation, … }` and answers with
  `Document { slot, generation, Result<Document, WidgetError> }`; a reply for an old generation
  (settings or secret changed meanwhile) is dropped. Images go through the existing image path,
  allowed by the widget's origins instead of a fixed list.
- The SDL thread schedules refreshes like today's feeds: only for store widgets on a visible
  Home, when the interval from the last document (or its back-off) has passed, and immediately
  after install, a settings change or a secret change. It keeps the last good document when a
  refresh fails and shows an error only while it has none.
- Identical requests from several instances within `max_age` are sent once.
- Images wanted are those of the cards in view and the next ones, and of the open dialog; each
  URL is asked for once per session, as today.

### 7.3 Code layout

- A new workspace crate `widgets/` (`luma-widgets`, no SDL or GL): manifest and document types
  and validation, the catalog index and signature, request planning (placeholders, encoding,
  origin checks), the jq runner and its child process, fixtures. Its binary `luma-widget` is the
  authors' tool (section 9). Dependencies: `jaq-core` 3, `jaq-std` 3 (without `log`), `jaq-json`
  2, `ed25519-compact` 2, `sha2`, and `libc` on Unix for the limits.
- In Luma: `src/widget_store.rs` (install, update, remove, files, secrets), `src/widget_worker.rs`
  (the worker), `src/widget_view.rs` (templates, dialog, store and settings pages), the `State`
  additions above, scheduling in `main.rs`, and the `--widget-transform` child entry before SDL
  starts.
- Desktop-only flags for development and screenshots: `--widget-catalog <https URL or folder>`
  with `--widget-catalog-key <public key>` to use a test catalog, `--widget-dev <folder>` to run
  a widget straight from its source folder, unsigned (section 9), and `--widget-fixtures
  <folder>` to answer its requests from a fixture case. A secret for one run comes from
  `LUMA_WIDGET_SECRET_<WIDGET>_<SETTING>` (e.g. `LUMA_WIDGET_SECRET_ESPORTS_KEY`): used, never
  saved. Transform children start with an empty environment.
- New messages in `assets/messages.json` for the store, the generic row states, "Data as of",
  minutes and the secret rows, in all seven languages. Everything a widget says itself comes
  from its manifest.

## 8. Esports as a store widget

The esports widget is the manifest `esports` in the catalog. Its shared feed is what
`eluz1ve/luma-feeds` publishes on GitHub Pages (`esports/<game>.json`); the widget's transforms
turn it, or PandaScore's replies, into a document. It began as a widget built into a fork of Luma
and renders the same from the store (the desktop screenshots compare them byte for byte).

**Settings:** `games` (choices: the eight disciplines, default Counter-Strike 2), `major_only`
(toggle, default on), `key` (secret: "PandaScore key", sent only to `api.pandascore.co`, checked
like today's phone page: letters, digits, `-`, `_`, `.`, at most 128).

**Origins:** `https://eluz1ve.github.io`, `https://api.pandascore.co`,
`https://cdn-api.pandascore.co` (logos and photos).

**Source `shared`** (no key): one step, `feed`, fanned out over `games`, refreshed every 300 s.
The transform does what `parse_feed`, `esports_matches` and the views do today: takes up to 20
matches per discipline, filters S- and A-tier when `major_only`, sorts live first then by start,
keeps 12, and turns each match into an item. `updated` is the oldest feed's build time.

**Source `live`** (with a key):

| Step       | Request                                                                                       | `max_age` | Required |
| ---------- | --------------------------------------------------------------------------------------------- | --------- | -------- |
| `running`  | `api.pandascore.co/{item}/matches/running?per_page=25&sort=begin_at`                          | 0         | yes      |
| `upcoming` | `api.pandascore.co/{item}/matches/upcoming?per_page=50&sort=begin_at`                         | 0         | yes      |
| `shared`   | `eluz1ve.github.io/luma-feeds/esports/{item}.json`                                            | 300       | no       |
| `teams`    | `api.pandascore.co/{item}/teams?filter[id]={var.ids}&per_page=100` (the shown matches' teams) | 1800      | no       |

All fanned out over `games`; the PandaScore ones carry `Authorization: Bearer {secret.key}`. The
transform parses PandaScore's matches as `parse_matches` does, merges map names, round scores,
picks and deciders from the shared feed by match id and map position (`merge_maps`), marks
substitutes and coaches in PandaScore's lineups by the shared feed's (`merge_substitutes`), and
builds the same items. It asks for a refresh every `max(8 × disciplines, 30)` s while a match is
live and every 300 s otherwise, which keeps two lists per discipline within the free plan's
1,000 requests an hour. Lineups come with the list (cached for 30 minutes), so a dialog opens
with them.

**Where the built-in version's parts land in the templates:**

| Built in (the fork's `src/view.rs`)                                                                     | Template field                                                                                  |
| ------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------- |
| Header "Esports · CS2, Valorant", "3–5 / 12", "Data as of"                                              | widget name + `title`, row meta, `updated`                                                      |
| Card: LIVE badge or start time, league logo, event                                                      | `live`, `status: {start}`, `logo`, `caption` (discipline prefixed when several)                 |
| Card: teams with logos, name or acronym, maps won, leader white                                         | `sides[].name/short/image/score/lead`                                                           |
| Card: "Map 3 · Mirage", rounds, a dot per map                                                           | `progress.label/value/steps`                                                                    |
| Card: "BO3 · Playoffs" before the start                                                                 | `footer`                                                                                        |
| Dialog header: league logo, event, discipline · stage, chips BO/Tier/LAN/prize                          | `header.logo/title/subtitle/chips` (prize `gold`, formatted by the transform as `prize()` does) |
| Hero: logos with team glow, names, country, score or VS, LIVE, live map, data time, or the start        | `hero.sides`, `hero.live`, `hero.lines` (`{start}`, `{as_of}`)                                  |
| Map tiles: name or "Map 1", 01, pick/decider, LIVE + minutes, winner + length, rounds                   | `tiles.items[].title/placeholder/number/note/state/result/value` (`{elapsed}`)                  |
| Lineups: logo, "NAVI lineup", photo, nickname, real name, country · role, BENCH/COACH line, "No lineup" | `lists[].image/title/rows/extra/empty`                                                          |
| Footer: streams (official first), Liquipedia credit, data note                                          | `footer.label/text/credit/note`                                                                 |

All esports wording (the disciplines' names, "Map {number}", "Pick: {team}", "Decider", "BENCH",
the notes and credits, in seven languages) is the manifest's `strings`; none of it is in
`assets/messages.json`.

## 9. Making and publishing a widget

Widgets live in a public repository, `eluz1ve/luma-widgets`, whose GitHub Pages site is the
catalog:

```
widgets/
  esports/
    manifest.json        sources may say "transform": {"file": "live.jq"}
    shared.jq
    live.jq
    library.jq
    fixtures/
      shared/context.json              a case: settings, secrets set, language, time
      shared/index.json                which file answers which exact URL
      shared/csgo.json …               sample replies
      live/…
    expected/
      shared.json                      the document each case must produce
      live.json
    preview.png          a desktop screenshot of the row and the dialog
    README.md            what it shows, where the data comes from, its licence and credits
  _template/             a minimal widget to copy
.github/
  workflows/check.yml    on pull requests: check every widget, run its fixtures
  workflows/publish.yml  on main: pack, sign, publish the site
  CODEOWNERS             widgets/<id>/ → its authors
  pull_request_template.md
```

**The tool.** `luma-widget` (from the `widgets/` crate of lg-luma, installable with
`cargo install --git https://github.com/eluz1ve/lg-luma luma-widgets`) runs the same code as
the TV:

```sh
luma-widget check widgets/esports          # schema, limits, origins, templates, jq without forbidden filters
luma-widget run widgets/esports live       # run a fixture case (or --network), print the document
luma-widget test widgets/esports           # every case against expected/
luma-widget eval '<jq>' '<json>'           # try an expression with Luma's jq
luma-widget pack widgets out --sequence N  # inline .jq files, hash, write manifests/ and index.json
luma-widget sign out                       # publish workflow only, key in $CATALOG_SIGNING_KEY
luma-widget verify out --key <public key>  # what Luma checks, before deploying
luma-widget keygen --secret-out <file>     # a new catalog key; prints the public half
```

**Seeing it.** `luma --widget-dev widgets/esports` on the desktop shows the widget from its
folder as if installed, with the real templates and real requests to its origins, so an author
sees exactly what a TV will draw. `--widget-fixtures widgets/esports/fixtures/live` answers its
requests from a fixture case instead, with no network.

**Submitting.** Fork the repository, add `widgets/<id>/`, open a pull request. The check workflow
validates it and runs its fixtures; it needs no secrets, so it runs safely on forks.

**Review.** A widget cannot run code on a TV, read its files or reach anything outside its
origins, whatever it contains; that is enforced by Luma, not by review. What a person still
checks, quickly:

- the origins: whom every TV with the widget will contact, and that they are the data's real
  source;
- where secrets are sent: only to the API they belong to;
- the data's licence and the credit it needs (e.g. Liquipedia's CC-BY-SA), and that refresh
  rates respect the API's limits;
- the name and description: not misleading, not someone else's brand.

New widgets, and updates that add origins or secret destinations, need the maintainer's approval.
Other updates of a widget only need one of its own authors (CODEOWNERS), and can be merged
automatically when the checks pass. So "accept as it is" holds for updates within the access a
widget already has, and new access always gets a human look.

**Publishing.** Merging to `main` runs the publish workflow: it packs every widget, increases
`sequence`, signs `index.json` with the catalog key, and deploys the site. The private key is a
GitHub Actions secret of a protected `publish` environment, used only by that workflow on `main`;
its public half is compiled into Luma. TVs pick the change up the next time they look (store
opened, or daily).

## 10. Verification

- Tests come first (`cargo test --locked -p luma -p luma-widgets`): catalog signature, wrong key,
  hash mismatch, replayed sequence, oversize; manifest validation (origins with paths, a
  placeholder in the host, a secret in a URL or sent elsewhere, forbidden jq filters); request
  planning (encoding, fan-out, `vars`, request cap, cache); the child process stopping
  `def f: f; f`, `repeat(1)` and `[range(1e9)]` without harming the parent; document caps; the
  store, install, settings and secret rows on `State`; each template's controls and layout.
- The esports widget's fixtures: a real shared feed snapshot and PandaScore replies give the
  expected documents, with map names merged and substitutes marked.
- Desktop screenshots: the store-installed esports widget next to the fork's built-in one, from
  the shared feed and live with a key supplied at run time (never written to a file, a commit or
  a build), row and dialog.
- On the TV, before anything else is built on it: starting the transform child inside webOS's
  app jail, and the esports transform's time on the TV's CPU (aim: under 200 ms).

## 11. Limits at a glance

| What                                  | Limit                                                              |
| ------------------------------------- | ------------------------------------------------------------------ |
| Index / manifest                      | 256 KiB each                                                       |
| Origins per widget                    | 8                                                                  |
| Settings / strings                    | 12 / 200                                                           |
| Sources / steps per source            | 4 / 8                                                              |
| Requests per refresh                  | 32, counted as the steps' fan-out before sending                   |
| Reply                                 | 1 MiB; 4 MiB in all into one transform                             |
| Image                                 | 2 MiB, shrunk to 160 × 160 or 300 × 450                            |
| Refresh                               | 30 s … 1 h; back-off up to 15 min, at least 5 min on 429           |
| jq program                            | 64 KiB each, 128 KiB with the library                              |
| Transform                             | 2 s CPU, 256 MiB (TV), 3 s wall clock, 512 KiB output              |
| `vars` value                          | 1 KiB                                                              |
| Document                              | 50 items; 200-character texts; 8 chips, 7 tiles, 2 lists × 12 rows |
| Installed widgets / instances on Home | 16 / 16                                                            |

## 12. How this fits design.md

Store widgets add no new visual language. A store widget is a **row**: the standard header, the
track of `card()`s at `TILE_W` or `WIDE_W`, the ghost card for nothing-yet, carousel stepping
through `State::row_of`. Its dialog is the existing dialog panel. Type, colours and radii are
design tokens; tones map to `LIVE`, `GOLD`, `UP`, `DOWN`; tints come from images or `name_tint`.
The store's list and pages are built from the same parts. design.md's table of widget kinds gains
"store widgets (rows)" and a pointer to section 5 of this page; tokens the templates need and
design.md lacks are added there first.
