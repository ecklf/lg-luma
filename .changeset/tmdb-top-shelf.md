---
"lg-luma": minor
---

Add a TMDB Top Shelf on Home (on by default) with optional API key override, streaming region and suggestion language. Reattach cached backdrop files after restart so shelves (including Stremio) do not stay on the gray fallback. Fade the backdrop out when focus leaves a mapped streaming app so the wallpaper returns. Keep title/overview clear of the Home channels strip. Make Stremio use weekly trending (with a short provider fallback), skip empty Amazon provider ids, and require watch monetization types so Top Shelf actually fills. Keep the dwell clock while visibility is 0 so fetches are not reset every frame. Dwell only before the first fetch; already-cached apps crossfade series→series when focus moves (no wallpaper dip).
