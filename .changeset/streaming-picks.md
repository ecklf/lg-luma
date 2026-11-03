---
'lg-luma': minor
---

Add a Streaming picks widget: a "Top Picks" row on Home that recommends movies and shows from
TMDB, filtered to the streaming apps installed on the TV so every card opens in an app you have.
Titles blend this week's trending with recent releases, fetched per service so smaller catalogues
(e.g. Crunchyroll) are represented. Each focused card shows genre, runtime or seasons, and the
streaming service, wrapping onto a second line when needed.

Settings (Widgets › Streaming picks): toggle the widget, enter a TMDB API key, choose the content
type (movies, shows or both), the refresh interval, how many cards show on the home screen (5–10),
and whether to hide the animation category. The last card refreshes the row with a fresh page of
titles. The feed is cached on disk so the row appears instantly at launch.
