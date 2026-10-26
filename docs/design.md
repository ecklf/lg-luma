# Luma Home design standard

Every Home widget follows this page. It is read from the sofa, three metres from a 55" TV: few
things, large, aligned to one rhythm. New widgets reuse the tokens in `src/design.rs` and the
primitives in `src/view.rs` (`row_header`, `card`, `track`); they do not bring sizes, colours,
corner radii or type of their own.

## Two kinds of widget

Home is a page title followed by rows, with small glance widgets beside the title.

| Kind       | Widgets                       | Looks like                                                                                                                                                                                                                                                                             |
| ---------- | ----------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Title**  | Greeting (date and greeting)  | Text straight on the wallpaper, never on a card.                                                                                                                                                                                                                                       |
| **Glance** | Weather, Crypto               | One or two lines of facts in the weather line's type (23.2 px). On the wallpaper in themes without cards (Classic), on the theme's surface card (`widget_card`, `RADIUS` corners) in themes with cards. All glance widgets of a theme look the same: never one bare and one on a card. |
| **Row**    | Favourites, Inputs, Premieres | A row header, then a carousel of cards. No box behind the row: the cards are the surfaces.                                                                                                                                                                                             |

A new widget is a row unless it is one or two lines of facts.

## Rows

```
Section title                                   meta · 3–5 / 12
┌──────────┐ ┌──────────┐ ┌──────────┐ ┌───
│   card   │ │   card   │ │   card   │ │  ← next card cut off at the edge
└──────────┘ └──────────┘ └──────────┘ └───
```

- **Header** (`design::ROW_HEADER`, 66 px): the section title in `design::section()` at the
  block's top-left, optional meta at its right in `design::meta()`: how fresh the data is
  ("Data as of 22:49") and, once the row scrolls, which cards show ("3–5 / 12"). A block one cell
  tall is a _compact row_: no header, the cards fill the cell.
- **Track**: one line of cards, starting at the block's left edge, `design::GAP` apart. It runs to
  the block's right edge, or to the screen's edge when the block reaches the last column, and the
  card after the last whole one shows cut off under an edge fade, so the row clearly goes on.
- **Carousel**: Left and Right step one card at a time through every item; focus that lands on a
  card by any route scrolls it fully into view (`State::reveal_focus`). Right on the last card stays
  there unless another block lies beside the row. Pointer hover never scrolls.
- **Cards** have one height per row: the block's height minus the header (158 px at two cells,
  282 px at three). A taller block makes cards larger, never more lines of small ones. A card
  type may cap it: input cards keep their 104 px (`INPUT_H`), the height of their two lines.

### Card sizes

| Card | Width                                | Used by           |
| ---- | ------------------------------------ | ----------------- |
| Tile | `TILE_W` 256                         | app tiles         |
| Wide | `WIDE_W` 537.6 (two tiles and a gap) | inputs, premieres |

Widths do not stretch to fill a block; the track shows as many as fit and cuts the next one off.

### Card surface

`card()` draws every card: `design::RADIUS` (19.2) corners, a soft shadow, the theme's surface
with a white highlight fading down from the top, and a 1 px white 8 % border. Glance cards and
dialogs use the same radius; themes change the surface and the focus accent, not the shapes. Focus brightens the fill and adds the focus ring
in the theme's accent; it never moves or scales the card. A card's own state uses one accent at
a time: a state such as "live" colours the card's edge, never its whole fill.

Inside a card: `design::PAD` (20 px) padding. Pictures (posters, logos) sit flush with the
card's edge or inside the padding, never floating at arbitrary insets. Text never runs under
another element: lay out the fixed parts first (status, score), then fit the free text into what
is left with `fit()`.

## Type

| Role       | Style                             | Example                  |
| ---------- | --------------------------------- | ------------------------ |
| Display    | greeting only                     | "Good evening."          |
| Section    | `section()` 23.2 / 550            | row titles               |
| Card title | `card_title(tall)` 22 or 28 / 650 | a show, a team           |
| Body       | `body()` 18.4 / 500               | secondary lines          |
| Meta       | `meta()` 15.2 / 500, 55 % white   | event, freshness, counts |

Nothing on Home is smaller than 14.4 px. Font weights exist for 350–650 and 800 only.

## Colour

Text is `TEXT` (#f5f5f5) at 100 %, 85 %, 60 % or 45 %. Accents: `UP` green and `DOWN` red for
changes; add any other accent to `src/design.rs` first. The theme's accent is reserved for
focus. Placeholders without an image (a premiere without its poster) use a tint from the
name (`name_tint`), never grey discs.

## Empty and loading states

A row that has nothing yet shows one ghost card (white 4 %, dashed feel by a 1 px white 10 % border)
with the message inside, in `body()` at 62 %. The header stays, so the row keeps its place.

## Checklist for a new widget

1. Is it a glance or a row? Pick from the table above.
2. Rows: `row_header()`, then `track()` with `TILE_W` or `WIDE_W` cards drawn by `card()`.
3. Register one control per visible card; give its `Target` a row in `State::row_of` so the
   carousel stepping and reveal work.
4. Use only `design` tokens and styles. If something is missing, add it to `src/design.rs` and
   this page first.
5. Check a desktop screenshot next to the favourites row: same title, same gap, same radius,
   same card height.
