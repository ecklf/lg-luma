//! Store widgets on screen (docs/widgets.md §5): a row of `scoreboard`, `media` or `ticker`
//! cards, the item dialog, the store's list and pages, and a widget's settings page. Built
//! from the same primitives and tokens as every Home row (docs/design.md).
use super::*;
use crate::widget_state::{StoreAction, WidgetRow};
use crate::widget_store::CatalogState;
use luma_widgets::document::{
    Card, Dialog, Document, Mark, Media, Scoreboard, Side, StepState, Text, Ticker, Tone,
};
use luma_widgets::manifest::{CardKind, Manifest, SettingKind};
use luma_widgets::refresh::WidgetError;

/// Formats a document text in the TV's language and time zone. Elapsed minutes count up to
/// the data's time, which a shared feed's `updated` sets.
pub(super) fn show(
    value: &Text,
    language: Language,
    now: OffsetDateTime,
    data_time: OffsetDateTime,
) -> String {
    match value {
        Text::Plain(text) => text.clone(),
        Text::Start(at) => match_time(at, now, language),
        Text::Elapsed(since) => minutes_since(since, data_time)
            .map(|minutes| language.format("widgetMinutes", &[("number", &minutes.to_string())]))
            .unwrap_or_default(),
        Text::AsOf(at) => OffsetDateTime::from_unix_timestamp(*at)
            .map(|at| language.format("widgetAsOf", &[("time", &local_time(at, now, language))]))
            .unwrap_or_default(),
    }
}

fn data_time(document: &Document, now: OffsetDateTime) -> OffsetDateTime {
    document
        .updated
        .and_then(|at| OffsetDateTime::from_unix_timestamp(at).ok())
        .map_or(now, |at| at.min(now))
}

fn tone_color(tone: Tone) -> Option<Color> {
    match tone {
        Tone::Live => Some(design::color(design::LIVE_TEXT, 1.)),
        Tone::Gold => Some(c(0xf7d88a, 1.)),
        Tone::Up => Some(design::color(design::UP, 1.)),
        Tone::Down => Some(design::color(design::DOWN, 1.)),
        Tone::Neutral => None,
    }
}

/// A side's colour: its image's accent once loaded, else a steady one from its name.
fn side_tint(state: &State, image: &Option<String>, name: &str) -> Color {
    feed_image(state, image)
        .map(|image| image.backdrop[0])
        .filter(|rgb| *rgb != crate::feeds::NO_ACCENT)
        .map_or_else(
            || name_tint(name),
            |[r, g, b]| [r as f32 / 255., g as f32 / 255., b as f32 / 255., 1.],
        )
}

fn side_logo(out: &mut Scene, state: &State, side: Option<&Side>, rect: Rect) {
    match side {
        Some(side) => emblem(
            out,
            state,
            &side.image,
            &side.short,
            rect,
            Some(name_tint(&side.short)),
        ),
        None => emblem(out, state, &None, "?", rect, None),
    }
}

/// The card width a template uses.
fn card_width(kind: CardKind) -> f32 {
    match kind {
        CardKind::Scoreboard | CardKind::Media => design::WIDE_W,
        CardKind::Ticker => design::TILE_W,
    }
}

/// A message for a row without items: the error, loading, or the document's own.
fn row_message(state: &State, manifest: &Manifest, slot: u8) -> Option<String> {
    let language = state.preferences.language;
    let code = language.code();
    let secret = |id: &str| {
        manifest
            .setting(id)
            .map_or_else(String::new, |s| s.label.get(code).to_owned())
    };
    let shown = state.widgets.slot(slot);
    let document = shown.and_then(|s| s.document.as_ref());
    if let Some(error) = shown.and_then(|s| s.error.as_ref()) {
        let message = match error {
            WidgetError::NoSource { secret: id } => {
                Some(language.format("widgetNoSource", &[("secret", &secret(id))]))
            }
            WidgetError::KeyRefused { secret: id } => {
                Some(language.format("widgetKeyRefused", &[("secret", &secret(id))]))
            }
            WidgetError::Broken(_) if document.is_none() => {
                Some(language.t("widgetBroken").to_owned())
            }
            _ if document.is_none() => Some(language.t("widgetUnavailable").to_owned()),
            _ => None,
        };
        if message.is_some() {
            return message;
        }
    }
    match document {
        None => Some(language.t("widgetLoading").to_owned()),
        Some(document) if document.items.is_empty() => Some(
            document
                .empty
                .as_ref()
                .map(|e| {
                    show(
                        e,
                        language,
                        OffsetDateTime::now_utc(),
                        OffsetDateTime::now_utc(),
                    )
                })
                .filter(|e| !e.is_empty())
                .unwrap_or_else(|| language.t("widgetEmpty").to_owned()),
        ),
        Some(_) => None,
    }
}

/// A store widget's block: the row header, then its cards (docs/design.md, Rows).
#[allow(clippy::too_many_arguments)]
pub(super) fn row(
    out: &mut Scene,
    state: &State,
    assets: &Assets,
    theme: &crate::themes::Theme,
    slot: u8,
    block: &crate::home_grid::Block,
    rect: Rect,
    now: OffsetDateTime,
) {
    let language = state.preferences.language;
    let Some(manifest) = state.slot_manifest(slot) else {
        let track = design::track(block, rect, design::WIDE_W);
        if track.header {
            row_header(out, assets, rect, &state.block_name(block.kind), &[]);
        }
        ghost(
            out,
            assets,
            Rect::new(track.x, track.y, rect.w, track.height),
            language.t("widgetMissing"),
        );
        return;
    };
    let document = state.widgets.slot(slot).and_then(|s| s.document.clone());
    let count = document.as_ref().map_or(0, |d| d.items.len());
    let track = design::track(block, rect, card_width(manifest.card));
    let first = track.first(
        state.row_scroll.get(&block.kind).copied().unwrap_or(0),
        count,
    );
    if track.header {
        let mut title = manifest.name.get(language.code()).to_owned();
        if let Some(extra) = document
            .as_ref()
            .and_then(|d| d.title.as_ref())
            .map(|t| show(t, language, now, now))
            .filter(|t| !t.is_empty())
        {
            title = format!("{title} · {extra}");
        }
        let meta: Vec<String> = row_position(&track, first, count)
            .into_iter()
            .chain(
                document
                    .as_ref()
                    .and_then(|d| d.updated)
                    .map(|at| show(&Text::AsOf(at), language, now, now)),
            )
            .collect();
        row_header(out, assets, rect, &title, &meta);
    }
    if let Some(message) = row_message(state, &manifest, slot) {
        ghost(
            out,
            assets,
            Rect::new(track.x, track.y, rect.w, track.height),
            &message,
        );
        return;
    }
    let Some(document) = document else {
        return;
    };
    let data_time = data_time(&document, now);
    track_cards(out, state, track, block.kind, count, |out, index, rect| {
        let target = Target::WidgetItem(slot, index);
        let focused = state.focus == target && !state.pointer;
        match &document.items[index].card {
            Card::Scoreboard(card) => scoreboard(
                out, state, assets, theme, card, rect, focused, now, data_time,
            ),
            Card::Media(card) => media(out, state, assets, theme, card, rect, focused),
            Card::Ticker(card) => ticker(out, state, assets, theme, card, rect, focused, now),
        }
        target
    });
}

/// Two sides and their scores: event and status on top, the sides, then progress or a
/// footer (today's match card, from the document).
#[allow(clippy::too_many_arguments)]
fn scoreboard(
    out: &mut Scene,
    state: &State,
    assets: &Assets,
    theme: &crate::themes::Theme,
    found: &Scoreboard,
    rect: Rect,
    focused: bool,
    now: OffsetDateTime,
    data_time: OffsetDateTime,
) {
    let language = state.preferences.language;
    let say = |value: &Option<Text>| {
        value
            .as_ref()
            .map(|v| show(v, language, now, data_time))
            .unwrap_or_default()
    };
    let dim = design::ink;
    let compact = rect.h < 200.;
    card(
        out,
        theme,
        rect,
        focused,
        found
            .live
            .then(|| design::color(design::LIVE, if focused { 0.7 } else { 0.38 })),
    );
    let pad = design::PAD;
    let right = rect.x + rect.w - pad;

    // Top: LIVE or the status, then the logo and caption in what is left.
    let top = rect.y + if compact { 13. } else { 18. };
    let mut x = rect.x + pad;
    if found.live {
        live_badge(out, language, Rect::new(x, top, 52., 22.), 13.);
        x += 52. + 12.;
    } else {
        let status = say(&found.status);
        if !status.is_empty() {
            let style = text(16., 600, 0.);
            let w = assets.width(&status, style) + 4.;
            out.text(&status, Rect::new(x, top + 1., w, 22.), style, WHITE, false);
            x += w + 12.;
        }
    }
    if let Some(image) = feed_image(state, &found.logo) {
        out.draw.push(Draw::AppImage(
            image.clone(),
            Rect::new(x, top - 1., 24., 24.),
            [1.; 4],
        ));
        x += 32.;
    }
    let caption = say(&found.caption);
    let caption_style = text(15.2, 500, 0.);
    if right - x > 24. && !caption.is_empty() {
        out.text(
            fit(assets, &caption, caption_style, right - x),
            Rect::new(x, top + 2., right - x, 20.),
            caption_style,
            dim(0.6),
            false,
        );
    }

    // The sides, the leading one in white.
    let (row_h, logo, name_size, score_size) = if compact {
        (38., 32., 21., 26.)
    } else {
        (66., 54., 26., 40.)
    };
    let sides_y = if compact {
        rect.y + 40.
    } else {
        rect.y + 56. + ((rect.h - 56. - 50.) - 2. * row_h) / 2.
    };
    let scored = found.sides.iter().flatten().any(|s| s.score.is_some());
    let score_w = if scored { score_size * 1.5 } else { 0. };
    for (index, side) in found.sides.iter().enumerate() {
        let y = sides_y + index as f32 * row_h;
        side_logo(
            out,
            state,
            side.as_ref(),
            Rect::new(rect.x + pad, y + (row_h - logo) / 2., logo, logo),
        );
        let name_x = rect.x + pad + logo + 14.;
        let name_w = right - score_w - 10. - name_x;
        let style = text(name_size, 600, -0.01);
        let name = match side {
            Some(side) if assets.width(&side.name, style) <= name_w => side.name.as_str(),
            Some(side) => side.short.as_str(),
            None => "TBD",
        };
        out.text(
            fit(assets, name, style, name_w),
            Rect::new(
                name_x,
                y + (row_h - name_size * 1.3) / 2.,
                name_w,
                name_size * 1.3,
            ),
            style,
            if side.is_some() { WHITE } else { dim(0.5) },
            false,
        );
        if let Some(score) = side.as_ref().and_then(|s| s.score.as_ref()) {
            out.text(
                score,
                Rect::new(
                    right - score_w,
                    y + (row_h - score_size * 1.3) / 2.,
                    score_w,
                    score_size * 1.3,
                ),
                design::figure(score_size).right(),
                if side.as_ref().is_some_and(|s| s.lead) {
                    WHITE
                } else {
                    dim(0.42)
                },
                false,
            );
        }
    }

    // Bottom: progress with a dot per step, or the footer.
    let bottom = rect.y + rect.h - if compact { 29. } else { 38. };
    if !compact {
        out.shape(
            Rect::new(rect.x + pad, bottom - 12., rect.w - 2. * pad, 1.),
            0.,
            c(0xffffff, 0.08),
        );
    }
    let style = text(15.2, 500, 0.);
    let Some(progress) = &found.progress else {
        let footer = say(&found.footer);
        out.text(
            fit(assets, &footer, style, rect.w - 2. * pad),
            Rect::new(rect.x + pad, bottom, rect.w - 2. * pad, 20.),
            style,
            dim(0.55),
            false,
        );
        return;
    };
    let count = progress.steps.len().min(7);
    let dots_x = right - (count as f32 * 14. - 6.).max(0.);
    for (index, step) in progress.steps.iter().take(7).enumerate() {
        let dot = Rect::new(dots_x + index as f32 * 14., bottom + 6., 8., 8.);
        match step {
            StepState::Live => out.shape(dot, 4., c(0xff5a5f, 1.)),
            StepState::Done => out.shape(dot, 4., dim(0.75)),
            StepState::Next => out.border(dot, 4., 1.5, dim(0.3)),
        }
    }
    let mut label_w = dots_x - 14. - (rect.x + pad);
    let value = say(&progress.value);
    if !value.is_empty() {
        let value_style = text(15.2, 650, 0.);
        let w = assets.width(&value, value_style) + 4.;
        out.text(
            &value,
            Rect::new(dots_x - 14. - w, bottom, w, 20.),
            value_style,
            WHITE,
            false,
        );
        label_w -= w + 10.;
    }
    out.text(
        fit(assets, &say(&progress.label), style, label_w),
        Rect::new(rect.x + pad, bottom, label_w, 20.),
        style,
        dim(0.8),
        false,
    );
}

/// A poster flush with the card's left edge, the title beside it and a line under it.
fn media(
    out: &mut Scene,
    state: &State,
    assets: &Assets,
    theme: &crate::themes::Theme,
    found: &Media,
    rect: Rect,
    focused: bool,
) {
    let language = state.preferences.language;
    let now = OffsetDateTime::now_utc();
    card(
        out,
        theme,
        rect,
        focused,
        found
            .live
            .then(|| design::color(design::LIVE, if focused { 0.7 } else { 0.38 })),
    );
    let title = show(&found.title, language, now, now);
    let poster = Rect::new(rect.x, rect.y, (rect.h * 2. / 3.).round(), rect.h);
    match feed_image(state, &found.image) {
        Some(image) => out.draw.push(Draw::Picture(
            image.clone(),
            poster,
            design::RADIUS,
            [1.; 4],
        )),
        None => {
            out.draw.push(Draw::RoundedGradient(
                poster,
                design::RADIUS,
                alpha(name_tint(&title), 0.45),
                alpha(name_tint(&title), 0.15),
            ));
            out.icon(assets, "tv", poster.inset(poster.w * 0.3), design::ink(0.5));
        }
    }
    if found.live {
        live_badge(
            out,
            language,
            Rect::new(poster.x + 12., poster.y + 12., 52., 22.),
            13.,
        );
    }
    let tall = rect.h > 200.;
    let x = poster.x + poster.w + design::PAD;
    let w = rect.x + rect.w - design::PAD - x;
    let style = design::card_title(tall).leading(1.18);
    let lines = assets.lines(&title, style, w).min(if tall { 3 } else { 2 }) as f32;
    out.text(
        &title,
        Rect::new(
            x,
            rect.y + design::PAD - 2.,
            w,
            lines * style.size * 1.18 + 2.,
        ),
        style,
        WHITE,
        false,
    );
    if let Some(subtitle) = &found.subtitle {
        let subtitle = show(subtitle, language, now, now);
        let style = design::body();
        out.text(
            fit(assets, &subtitle, style, w),
            Rect::new(x, rect.y + rect.h - design::PAD - 24., w, 24.),
            style,
            design::ink(0.7),
            false,
        );
    }
}

/// A figure that changes: its label, the figure, the change in its tone and a detail line.
#[allow(clippy::too_many_arguments)]
fn ticker(
    out: &mut Scene,
    state: &State,
    assets: &Assets,
    theme: &crate::themes::Theme,
    found: &Ticker,
    rect: Rect,
    focused: bool,
    now: OffsetDateTime,
) {
    let language = state.preferences.language;
    card(out, theme, rect, focused, None);
    let pad = design::PAD;
    let w = rect.w - 2. * pad;
    let compact = rect.h < 120.;
    let label = show(&found.label, language, now, now).to_uppercase();
    out.text(
        fit(assets, &label, design::caps(), w),
        Rect::new(rect.x + pad, rect.y + pad - 2., w, 20.),
        design::caps(),
        design::ink(0.6),
        false,
    );
    let size = if compact { 28. } else { 40. };
    let value = show(&found.value, language, now, now);
    let value_y = if compact {
        rect.y + rect.h - pad - size * 1.25
    } else {
        rect.y + (rect.h - size * 1.25) / 2. - 4.
    };
    let change = found.change.as_ref().map(|change: &Mark| {
        (
            show(&change.text, language, now, now),
            tone_color(change.tone).unwrap_or(design::ink(0.7)),
        )
    });
    let change_w = change
        .as_ref()
        .map_or(0., |(text, _)| assets.width(text, design::body()) + 12.);
    out.text(
        fit(assets, &value, design::figure(size), w - change_w),
        Rect::new(rect.x + pad, value_y, w - change_w, size * 1.25),
        design::figure(size),
        WHITE,
        false,
    );
    if let Some((text, color)) = change {
        out.text(
            &text,
            Rect::new(
                rect.x + rect.w - pad - change_w + 12.,
                value_y + size * 1.25 - 26.,
                change_w,
                24.,
            ),
            design::body(),
            color,
            false,
        );
    }
    if !compact && let Some(detail) = &found.detail {
        out.text(
            fit(assets, &show(detail, language, now, now), design::meta(), w),
            Rect::new(rect.x + pad, rect.y + rect.h - pad - 20., w, 20.),
            design::meta(),
            design::ink(0.55),
            false,
        );
    }
}

/// An item's dialog: header, hero, tiles, lists and footer, each when the item has it (today's
/// match dialog, from the document).
pub(super) fn dialog(
    out: &mut Scene,
    state: &State,
    assets: &Assets,
    slot: u8,
    id: &str,
    now: OffsetDateTime,
) {
    let Some(document) = state.widgets.slot(slot).and_then(|s| s.document.clone()) else {
        return;
    };
    let Some(found) = document
        .items
        .iter()
        .find(|item| item.id == id)
        .and_then(|item| item.dialog.as_ref())
    else {
        return;
    };
    let language = state.preferences.language;
    let data_time = data_time(&document, now);
    let say = |value: &Option<Text>| {
        value
            .as_ref()
            .map(|v| show(v, language, now, data_time))
            .unwrap_or_default()
    };
    let panel = Rect::new(296., 52., 1328., 976.);
    out.modal = Some(panel);
    out.shadow(panel, 28., 120., 25., 0.5);
    out.draw.push(Draw::RoundedGradient(
        panel,
        28.,
        c(0x2f3337, 0.98),
        c(0x1a1d21, 0.99),
    ));
    out.border(panel, 28., 1., c(0xffffff, 0.11));
    let close = close_button(out, state, assets, panel);
    let left = panel.x + 48.;
    let width = panel.w - 96.;
    let dim = |alpha| c(0xf5f5f5, alpha);
    let mut y = panel.y + 36.;

    if let Some(header) = &found.header {
        let mut x = left;
        if let Some(image) = feed_image(state, &header.logo) {
            out.draw.push(Draw::AppImage(
                image.clone(),
                Rect::new(left, y + 2., 58., 58.),
                [1.; 4],
            ));
            x += 76.;
        }
        let header_w = close.x - 24. - x;
        let title = text(28., 650, -0.02);
        out.text(
            fit(assets, &say(&header.title), title, header_w),
            Rect::new(x, y, header_w, 34.),
            title,
            WHITE,
            false,
        );
        out.text(
            fit(
                assets,
                &say(&header.subtitle),
                text(17.6, 450, 0.),
                header_w,
            ),
            Rect::new(x, y + 40., header_w, 22.),
            text(17.6, 450, 0.),
            dim(0.6),
            false,
        );
        let mut cx = left;
        for mark in &header.chips {
            let tone = match mark.tone {
                Tone::Gold => (design::color(design::GOLD, 0.16), c(0xf7d88a, 1.)),
                Tone::Live => (
                    design::color(design::LIVE, 0.2),
                    design::color(design::LIVE_TEXT, 1.),
                ),
                Tone::Up => (
                    design::color(design::UP, 0.14),
                    design::color(design::UP, 1.),
                ),
                Tone::Down => (
                    design::color(design::DOWN, 0.14),
                    design::color(design::DOWN, 1.),
                ),
                Tone::Neutral => (c(0xffffff, 0.07), WHITE),
            };
            cx = chip(
                out,
                assets,
                &show(&mark.text, language, now, data_time),
                cx,
                y + 80.,
                tone,
            );
        }
        y += if header.chips.is_empty() { 86. } else { 132. };
    }

    if let Some(hero) = &found.hero {
        let rect = Rect::new(left, y, width, 226.);
        out.draw.push(Draw::RoundedGradient(
            rect,
            22.,
            if hero.live {
                c(0x5c262b, 0.55)
            } else {
                c(0xffffff, 0.06)
            },
            c(0xffffff, 0.02),
        ));
        out.border(
            rect,
            22.,
            1.,
            if hero.live {
                c(0xe5484d, 0.3)
            } else {
                c(0xffffff, 0.07)
            },
        );
        let column = 420.;
        for (index, side) in hero.sides.iter().enumerate() {
            let x = if index == 0 {
                rect.x
            } else {
                rect.x + rect.w - column
            };
            let logo = Rect::new(x + (column - 112.) / 2., rect.y + 24., 112., 112.);
            if let Some(side) = side {
                out.draw.push(Draw::Shadow(
                    logo.inset(10.),
                    46.,
                    64.,
                    alpha(side_tint(state, &side.image, &side.short), 0.3),
                ));
            }
            side_logo(out, state, side.as_ref(), logo);
            let style = text(28., 650, -0.02);
            let name = side.as_ref().map_or("TBD", |s| s.name.as_str());
            out.text(
                fit(assets, name, style, column - 32.),
                Rect::new(x, rect.y + 150., column, 34.),
                style,
                WHITE,
                true,
            );
            if let Some(caption) = side
                .as_ref()
                .map(|s| say(&s.caption))
                .filter(|c| !c.is_empty())
            {
                out.text(
                    &caption,
                    Rect::new(x, rect.y + 188., column, 20.),
                    text(15.2, 600, 0.1),
                    dim(0.5),
                    true,
                );
            }
        }
        let middle = Rect::new(rect.x + column, rect.y, rect.w - 2. * column, rect.h);
        let centre = middle.x + middle.w / 2.;
        let scores: Vec<(&str, bool)> = hero
            .sides
            .iter()
            .map(|s| {
                s.as_ref()
                    .and_then(|s| s.score.as_deref().map(|score| (score, s.lead)))
                    .unwrap_or(("0", false))
            })
            .collect();
        if hero.sides.iter().flatten().any(|s| s.score.is_some()) {
            let style = text(84., 800, -0.03);
            let tone = |lead: bool| if lead { WHITE } else { dim(0.42) };
            out.text(
                scores[0].0,
                Rect::new(middle.x, rect.y + 24., centre - 34. - middle.x, 104.),
                style.right(),
                tone(scores[0].1),
                false,
            );
            out.text(
                ":",
                Rect::new(centre - 34., rect.y + 18., 68., 104.),
                style,
                dim(0.35),
                true,
            );
            out.text(
                scores[1].0,
                Rect::new(centre + 34., rect.y + 24., middle.w / 2. - 34., 104.),
                style,
                tone(scores[1].1),
                false,
            );
        } else {
            out.text(
                "VS",
                Rect::new(middle.x, rect.y + 44., middle.w, 56.),
                text(44., 650, 0.08),
                dim(0.3),
                true,
            );
        }
        let lines: Vec<String> = hero
            .lines
            .iter()
            .map(|line| show(line, language, now, data_time))
            .collect();
        if hero.live {
            live_badge(
                out,
                language,
                Rect::new(middle.x, rect.y + 134., middle.w, 30.),
                15.2,
            );
            if let Some(line) = lines.first() {
                let style = text(16., 600, 0.);
                out.text(
                    fit(assets, line, style, middle.w),
                    Rect::new(middle.x, rect.y + 172., middle.w, 22.),
                    style,
                    dim(0.85),
                    true,
                );
            }
            if let Some(line) = lines.get(1) {
                out.text(
                    fit(assets, line, text(14.4, 500, 0.), middle.w),
                    Rect::new(middle.x, rect.y + 198., middle.w, 20.),
                    text(14.4, 500, 0.),
                    dim(0.48),
                    true,
                );
            }
        } else {
            if let Some(line) = lines.first() {
                out.text(
                    fit(assets, line, text(24., 650, 0.), middle.w),
                    Rect::new(middle.x, rect.y + 142., middle.w, 30.),
                    text(24., 650, 0.),
                    dim(0.85),
                    true,
                );
            }
            if let Some(line) = lines.get(1) {
                out.text(
                    fit(assets, line, text(14.4, 500, 0.), middle.w),
                    Rect::new(middle.x, rect.y + 180., middle.w, 20.),
                    text(14.4, 500, 0.),
                    dim(0.48),
                    true,
                );
            }
        }
        y += 226. + 26.;
    }

    if let Some(tiles) = &found.tiles {
        tiles_section(
            out, state, assets, found, tiles, left, width, y, now, data_time,
        );
        y += 30. + 136. + 24.;
    }

    let footer_y = panel.y + panel.h - 86.;
    let lists_bottom = if found.footer.is_some() {
        footer_y - 16.
    } else {
        panel.y + panel.h - 40.
    };
    lists_section(
        out,
        state,
        assets,
        found,
        left,
        width,
        y,
        lists_bottom,
        now,
        data_time,
    );

    if let Some(footer) = &found.footer {
        let mut text_w = width;
        let credit = say(&footer.credit);
        if !credit.is_empty() {
            let style = text(14.4, 500, 0.);
            let w = assets.width(&credit, style) + 4.;
            out.text(
                &credit,
                Rect::new(left + width - w, footer_y + 8., w, 20.),
                style,
                dim(0.5),
                false,
            );
            text_w -= w + 24.;
        }
        let body = say(&footer.text);
        if !body.is_empty() {
            let label = say(&footer.label);
            let mut x = left;
            if !label.is_empty() {
                let label = format!("{label}:");
                let label_style = text(16., 650, 0.);
                let label_w = assets.width(&label, label_style);
                out.text(
                    &label,
                    Rect::new(left, footer_y + 6., label_w + 4., 22.),
                    label_style,
                    dim(0.8),
                    false,
                );
                x += label_w + 10.;
            }
            let style = text(16., 450, 0.);
            out.text(
                fit(assets, &body, style, left + text_w - x),
                Rect::new(x, footer_y + 6., left + text_w - x, 22.),
                style,
                dim(0.7),
                false,
            );
        }
        let note = say(&footer.note);
        if !note.is_empty() {
            out.text(
                &note,
                // Up to two lines: notes are long in some languages.
                Rect::new(left, footer_y + 38., width, 44.),
                text(14.4, 400, 0.).leading(1.4),
                dim(0.42),
                false,
            );
        }
    }
}

/// The dialog's close button at its top right; returns its rect.
fn close_button(out: &mut Scene, state: &State, assets: &Assets, panel: Rect) -> Rect {
    let close = Rect::new(panel.x + panel.w - 82., panel.y + 40., 33.6, 33.6);
    out.shape(close, 16.8, c(0xffffff, 0.035));
    out.border(close, 16.8, 1., c(0xffffff, 0.07));
    out.icon(assets, "close", close.inset(7.6), WHITE);
    if state.focus == Target::CloseDialog {
        out.border(close.inset(1.), 15.8, 2., WHITE);
    }
    out.control(Target::CloseDialog, close, false);
    close
}

#[allow(clippy::too_many_arguments)]
fn tiles_section(
    out: &mut Scene,
    state: &State,
    assets: &Assets,
    _dialog: &Dialog,
    tiles: &luma_widgets::document::Tiles,
    left: f32,
    width: f32,
    y: f32,
    now: OffsetDateTime,
    data_time: OffsetDateTime,
) {
    let language = state.preferences.language;
    let dim = |alpha| c(0xf5f5f5, alpha);
    let say = |value: &Option<Text>| {
        value
            .as_ref()
            .map(|v| show(v, language, now, data_time))
            .unwrap_or_default()
    };
    let title = say(&tiles.title).to_uppercase();
    out.text(
        title,
        Rect::new(left, y, 400., 20.),
        design::caps(),
        dim(0.5),
        false,
    );
    let count = tiles.slots.clamp(1, 7);
    let gap = 14.;
    let tile_w = ((width - gap * (count - 1) as f32) / count as f32).min(400.);
    for position in 0..count {
        let tile = tiles.items.get(position);
        let rect = Rect::new(
            left + position as f32 * (tile_w + gap),
            y + 30.,
            tile_w,
            136.,
        );
        let state_of = tile.map_or(StepState::Next, |t| t.state);
        match state_of {
            StepState::Live => {
                out.draw.push(Draw::RoundedGradient(
                    rect,
                    18.,
                    c(0xe5484d, 0.3),
                    c(0xe5484d, 0.08),
                ));
                out.border(rect, 18., 1.5, c(0xe5484d, 0.75));
            }
            StepState::Done => {
                out.draw.push(Draw::RoundedGradient(
                    rect,
                    18.,
                    c(0xffffff, 0.08),
                    c(0xffffff, 0.035),
                ));
                out.border(rect, 18., 1., c(0xffffff, 0.08));
            }
            StepState::Next => {
                out.shape(rect, 18., c(0xffffff, 0.03));
                out.border(rect, 18., 1., c(0xffffff, 0.06));
            }
        }
        let Some(tile) = tile else {
            continue;
        };
        let inner_x = rect.x + 20.;
        let mut inner_w = rect.w - 40.;
        // The tile's number, large and faint, where a wide tile has room for it.
        if rect.w >= 260.
            && let Some(number) = &tile.number
        {
            out.text(
                number,
                Rect::new(rect.x + rect.w - 104., rect.y + 6., 88., 70.),
                text(56., 800, -0.02).right(),
                c(
                    0xffffff,
                    if state_of == StepState::Live {
                        0.1
                    } else {
                        0.06
                    },
                ),
                false,
            );
            inner_w -= 64.;
        }
        let title = say(&tile.title);
        if !title.is_empty() {
            let style = text(23., 650, -0.01);
            out.text(
                fit(assets, &title, style, inner_w),
                Rect::new(inner_x, rect.y + 16., inner_w, 30.),
                style,
                WHITE,
                false,
            );
        } else {
            out.text(
                fit(
                    assets,
                    &say(&tile.placeholder),
                    text(19.2, 600, 0.),
                    inner_w,
                ),
                Rect::new(inner_x, rect.y + 18., inner_w, 26.),
                text(19.2, 600, 0.),
                dim(0.62),
                false,
            );
        }
        let note = say(&tile.note);
        if !note.is_empty() {
            let style = text(14.4, 550, 0.02);
            out.text(
                fit(assets, &note, style, inner_w),
                Rect::new(inner_x, rect.y + 50., inner_w, 20.),
                style,
                dim(0.55),
                false,
            );
        }
        // The result row: the value on the right, LIVE or the result on the left.
        let row_y = rect.y + rect.h - 48.;
        let inner_w = rect.w - 40.;
        let mut result_w = inner_w;
        let value = say(&tile.value);
        if !value.is_empty() {
            let style = text(23., 650, 0.);
            let w = assets.width(&value, style) + 4.;
            out.text(
                &value,
                Rect::new(inner_x + inner_w - w, row_y - 2., w, 30.),
                style,
                WHITE,
                false,
            );
            result_w -= w + 10.;
        }
        let result_text = tile
            .result
            .as_ref()
            .map(|r| say(&r.text))
            .unwrap_or_default();
        if state_of == StepState::Live {
            let style = text(17.6, 650, 0.06);
            let live = language.t("live");
            out.text(
                live,
                Rect::new(inner_x, row_y + 2., result_w, 24.),
                style,
                design::color(design::LIVE_TEXT, 1.),
                false,
            );
            if !result_text.is_empty() {
                let x = inner_x + assets.width(live, style) + 10.;
                out.text(
                    fit(
                        assets,
                        &result_text,
                        text(15.2, 500, 0.),
                        inner_x + result_w - x,
                    ),
                    Rect::new(x, row_y + 4., inner_x + result_w - x, 20.),
                    text(15.2, 500, 0.),
                    dim(0.65),
                    false,
                );
            }
        } else if let Some(result) = &tile.result {
            let name = result.name.clone().unwrap_or_default();
            let mut x = inner_x;
            if result.image.is_some() || !name.is_empty() {
                emblem(
                    out,
                    state,
                    &result.image,
                    &name,
                    Rect::new(inner_x, row_y, 26., 26.),
                    Some(side_tint(state, &result.image, &name)),
                );
                x += 36.;
            }
            let style = text(17.6, 650, 0.);
            out.text(
                fit(assets, &result_text, style, inner_x + result_w - x),
                Rect::new(x, row_y + 2., inner_x + result_w - x, 24.),
                style,
                WHITE,
                false,
            );
        } else {
            out.text(
                "—",
                Rect::new(inner_x, row_y, result_w, 26.),
                text(19.2, 500, 0.),
                dim(0.3),
                false,
            );
        }
    }
}

/// One or two lists side by side, each a card: a logo and title, rows of people, then the
/// extra lines on one dimmer line.
#[allow(clippy::too_many_arguments)]
fn lists_section(
    out: &mut Scene,
    state: &State,
    assets: &Assets,
    found: &Dialog,
    left: f32,
    width: f32,
    top: f32,
    bottom: f32,
    now: OffsetDateTime,
    data_time: OffsetDateTime,
) {
    if found.lists.is_empty() || bottom - top < 80. {
        return;
    }
    let language = state.preferences.language;
    let dim = |alpha| c(0xf5f5f5, alpha);
    let say = |value: &Option<Text>| {
        value
            .as_ref()
            .map(|v| show(v, language, now, data_time))
            .unwrap_or_default()
    };
    let columns = found.lists.len().min(2);
    let column_w = (width - 24. * (columns - 1) as f32) / columns as f32;
    for (index, list) in found.lists.iter().take(2).enumerate() {
        let rect = Rect::new(
            left + index as f32 * (column_w + 24.),
            top,
            column_w,
            bottom - top,
        );
        out.draw.push(Draw::RoundedGradient(
            rect,
            18.,
            c(0xffffff, 0.05),
            c(0xffffff, 0.02),
        ));
        out.border(rect, 18., 1., c(0xffffff, 0.06));
        let x = rect.x + 20.;
        let inner_w = rect.w - 40.;
        let name = list.name.clone().unwrap_or_default();
        let tint = (list.image.is_some() || !name.is_empty())
            .then(|| side_tint(state, &list.image, &name));
        let mut title_x = x;
        if list.image.is_some() || !name.is_empty() {
            emblem(
                out,
                state,
                &list.image,
                if name.is_empty() { "?" } else { &name },
                Rect::new(x, rect.y + 16., 28., 28.),
                Some(name_tint(&name)),
            );
            title_x += 40.;
        }
        out.text(
            fit(
                assets,
                &say(&list.title),
                text(17.6, 650, 0.),
                x + inner_w - title_x,
            ),
            Rect::new(title_x, rect.y + 19., x + inner_w - title_x, 24.),
            text(17.6, 650, 0.),
            dim(0.88),
            false,
        );
        let rows_y = rect.y + 58.;
        if list.rows.is_empty() {
            out.text(
                fit(assets, &say(&list.empty), text(17.6, 450, 0.), inner_w),
                Rect::new(x, rows_y + 4., inner_w, 26.),
                text(17.6, 450, 0.),
                dim(0.5),
                false,
            );
            continue;
        }
        let extra_y = rect.y + rect.h - 10. - if list.extra.is_empty() { 0. } else { 40. };
        let row_h = ((extra_y - rows_y) / list.rows.len().max(1) as f32).min(40.);
        let avatar = (row_h - 6.).min(32.);
        let tint = tint.map(|t| alpha(t, 0.8));
        for (number, row) in list.rows.iter().enumerate() {
            let y = rows_y + number as f32 * row_h;
            let title = show(&row.title, language, now, data_time);
            emblem(
                out,
                state,
                &row.image,
                &title,
                Rect::new(x, y, avatar, avatar),
                tint,
            );
            let meta = say(&row.meta);
            let meta_style = text(14.4, 600, 0.08);
            let meta_w = if meta.is_empty() {
                0.
            } else {
                assets.width(&meta, meta_style)
            };
            if !meta.is_empty() {
                out.text(
                    &meta,
                    Rect::new(
                        x + inner_w - meta_w - 4.,
                        y + (avatar - 18.) / 2.,
                        meta_w + 4.,
                        18.,
                    ),
                    meta_style,
                    dim(0.5),
                    false,
                );
            }
            let nick = text(19.2, 650, -0.01);
            let text_y = y + (avatar - 24.) / 2.;
            let nick_x = x + avatar + 14.;
            let nick_w = (x + inner_w - meta_w - 20. - nick_x).max(0.);
            let title = fit(assets, &title, nick, nick_w);
            out.text(
                &title,
                Rect::new(nick_x, text_y, nick_w, 24.),
                nick,
                WHITE,
                false,
            );
            let subtitle = say(&row.subtitle);
            let sub_x = nick_x + assets.width(&title, nick) + 12.;
            let sub_w = x + inner_w - meta_w - 20. - sub_x;
            if !subtitle.is_empty() && sub_w > 40. {
                let style = text(16., 450, 0.);
                out.text(
                    fit(assets, &subtitle, style, sub_w),
                    Rect::new(sub_x, text_y + 3., sub_w, 20.),
                    style,
                    dim(0.5),
                    false,
                );
            }
        }
        if !list.extra.is_empty() {
            let line_y = extra_y + 4.;
            out.shape(Rect::new(x, line_y, inner_w, 1.), 0., c(0xffffff, 0.08));
            let mut part_x = x;
            let end = x + inner_w;
            for extra in &list.extra {
                if part_x > end - 80. {
                    break;
                }
                let label = show(&extra.label, language, now, data_time).to_uppercase();
                let label_style = text(13.6, 650, 0.1);
                let label_w = assets.width(&label, label_style);
                out.text(
                    &label,
                    Rect::new(part_x, line_y + 14.5, label_w + 4., 17.),
                    label_style,
                    dim(0.42),
                    false,
                );
                let style = text(16., 550, 0.);
                let names_x = part_x + label_w + 10.;
                let names = fit(
                    assets,
                    &show(&extra.text, language, now, data_time),
                    style,
                    end - names_x,
                );
                let names_w = assets.width(&names, style);
                out.text(
                    &names,
                    Rect::new(names_x, line_y + 12., names_w + 4., 20.),
                    style,
                    dim(0.6),
                    false,
                );
                part_x = names_x + names_w + 28.;
            }
        }
    }
}

/// The store's panel: the same surface as the item dialog, as tall as its content.
fn store_panel(out: &mut Scene, state: &State, assets: &Assets, height: f32) -> Rect {
    let height = height.clamp(320., 860.);
    let panel = Rect::new(400., (540. - height / 2.).max(110.), 1120., height);
    out.modal = Some(panel);
    out.shadow(panel, 28., 120., 25., 0.5);
    out.draw.push(Draw::RoundedGradient(
        panel,
        28.,
        c(0x2f3337, 0.98),
        c(0x1a1d21, 0.99),
    ));
    out.border(panel, 28., 1., c(0xffffff, 0.11));
    close_button(out, state, assets, panel);
    panel
}

/// A widget's icon on a square in its own colour.
fn store_icon(out: &mut Scene, assets: &Assets, id: &str, icon: &str, rect: Rect) {
    let tint = name_tint(id);
    out.draw.push(Draw::RoundedGradient(
        rect,
        rect.w * 0.26,
        alpha(tint, 0.55),
        alpha(tint, 0.25),
    ));
    out.border(rect, rect.w * 0.26, 1., alpha(tint, 0.6));
    out.icon(assets, icon, rect.inset(rect.w * 0.24), WHITE);
}

/// The host of an origin, as people know it.
fn host(origin: &str) -> &str {
    origin.strip_prefix("https://").unwrap_or(origin)
}

/// The store's list of widgets.
pub(super) fn store(out: &mut Scene, state: &State, assets: &Assets) {
    let language = state.preferences.language;
    let code = language.code();
    let entries = state.store_entries();
    let row_h = 112.;
    let rows = entries.len().clamp(1, 5) as f32;
    let panel = store_panel(out, state, assets, 140. + rows * (row_h + 14.) - 14. + 48.);
    let left = panel.x + 48.;
    let width = panel.w - 96.;
    out.text(
        language.t("widgetStoreTitle"),
        Rect::new(left, panel.y + 40., width - 80., 40.),
        text(32., 500, -0.04),
        WHITE,
        false,
    );
    out.text(
        fit(
            assets,
            language.t("widgetStoreHint"),
            design::body(),
            width - 60.,
        ),
        Rect::new(left, panel.y + 88., width - 60., 24.),
        design::body(),
        design::ink(0.6),
        false,
    );
    let list = Rect::new(left, panel.y + 140., width, panel.h - 188.);
    let message = match &state.widgets.catalog {
        CatalogState::Idle | CatalogState::Loading => Some(language.t("widgetStoreLoading")),
        CatalogState::Unavailable => Some(language.t("widgetStoreUnavailable")),
        CatalogState::Unverified => Some(language.t("widgetStoreUnverified")),
        CatalogState::Ready(index) if index.widgets.is_empty() => {
            Some(language.t("widgetStoreEmpty"))
        }
        CatalogState::Ready(_) => None,
    };
    if let Some(message) = message {
        ghost(
            out,
            assets,
            Rect::new(list.x, list.y, list.w, row_h),
            message,
        );
        return;
    }
    let visible = ((list.h + 14.) / (row_h + 14.)).floor().max(1.) as usize;
    let focused = match state.focus {
        Target::StoreEntry(index) => index,
        _ => 0,
    };
    let first = focused.saturating_sub(visible - 1);
    for (index, entry) in entries.iter().enumerate().skip(first).take(visible) {
        let rect = Rect::new(
            list.x,
            list.y + (index - first) as f32 * (row_h + 14.),
            list.w,
            row_h,
        );
        let target = Target::StoreEntry(index);
        let focused = state.focus == target && !state.pointer;
        out.shape(rect, 20., c(0xffffff, if focused { 0.09 } else { 0.035 }));
        out.border(
            rect,
            20.,
            if focused { 2. } else { 1. },
            c(0xffffff, if focused { 0.92 } else { 0.07 }),
        );
        store_icon(
            out,
            assets,
            &entry.id,
            &entry.icon,
            Rect::new(rect.x + 22., rect.y + 22., 68., 68.),
        );
        // The status chip at the right, then the name and description in what is left.
        let installed = state.preferences.widgets.installed.get(&entry.id);
        let status = if entry.luma > crate::widget_state::luma_version() {
            Some((
                language.format("widgetNeedsLuma", &[("version", &entry.luma.to_string())]),
                (c(0xffffff, 0.05), design::ink(0.55)),
            ))
        } else if state.widgets.installing.contains(&entry.id) {
            Some((
                language.t("widgetInstalling").to_owned(),
                (c(0xffffff, 0.07), WHITE),
            ))
        } else {
            installed.map(|record| {
                let newer = luma_widgets::manifest::Version::parse(&record.version)
                    .is_some_and(|v| entry.version > v);
                if newer {
                    (
                        language.t("widgetUpdateAvailable").to_owned(),
                        (design::color(design::GOLD, 0.16), c(0xf7d88a, 1.)),
                    )
                } else {
                    (
                        language.t("widgetInstalled").to_owned(),
                        (
                            design::color(design::UP, 0.14),
                            design::color(design::UP, 1.),
                        ),
                    )
                }
            })
        };
        let mut text_right = rect.x + rect.w - 24.;
        if let Some((label, tone)) = status {
            let w = assets.width(&label, text(15.2, 550, 0.01)) + 28.;
            chip(
                out,
                assets,
                &label,
                text_right - w,
                rect.y + (row_h - 30.) / 2.,
                tone,
            );
            text_right -= w + 20.;
        }
        let x = rect.x + 22. + 68. + 22.;
        let w = text_right - x;
        let title = design::card_title(false);
        out.text(
            fit(assets, entry.name.get(code), title, w),
            Rect::new(x, rect.y + 24., w, 30.),
            title,
            WHITE,
            false,
        );
        out.text(
            fit(assets, entry.description.get(code), design::body(), w),
            Rect::new(x, rect.y + 62., w, 24.),
            design::body(),
            design::ink(0.62),
            false,
        );
        out.control(target, rect, false);
    }
}

/// One widget's page in the store: what it is, what it connects to, and what can be done.
pub(super) fn store_entry(out: &mut Scene, state: &State, assets: &Assets, id: &str) {
    let language = state.preferences.language;
    let code = language.code();
    let entry = state.store_entries().into_iter().find(|e| e.id == id);
    // The content's height first, so the panel fits it.
    let width = 1120. - 96.;
    let style = text(20., 450, -0.01).leading(1.45);
    let height = entry.map_or(320., |entry| {
        let lines = assets
            .lines(entry.description.get(code), style, width)
            .min(3) as f32;
        let mut rows = 1.;
        let mut x = 0.;
        for origin in &entry.origins {
            let w = assets.width(host(origin.as_str()), text(15.2, 550, 0.01)) + 38.;
            if x + w > width + 10. {
                rows += 1.;
                x = 0.;
            }
            x += w;
        }
        168. + lines * 20. * 1.45
            + 36.
            + 32.
            + rows * 40.
            + 12.
            + entry.secrets.len() as f32 * 34.
            + 34.
            + 60.
            + 48.
    });
    let panel = store_panel(out, state, assets, height);
    let left = panel.x + 48.;
    let Some(entry) = entry else {
        out.text(
            language.t("widgetNoLonger"),
            Rect::new(left, panel.y + 48., width - 80., 30.),
            design::body(),
            design::ink(0.7),
            false,
        );
        return;
    };
    store_icon(
        out,
        assets,
        &entry.id,
        &entry.icon,
        Rect::new(left, panel.y + 40., 96., 96.),
    );
    let x = left + 96. + 28.;
    let w = panel.x + panel.w - 130. - x;
    out.text(
        fit(assets, entry.name.get(code), text(34., 650, -0.03), w),
        Rect::new(x, panel.y + 44., w, 42.),
        text(34., 650, -0.03),
        WHITE,
        false,
    );
    let by = language.format(
        "widgetBy",
        &[
            ("authors", &entry.authors.join(", ")),
            ("version", &entry.version.to_string()),
        ],
    );
    out.text(
        fit(assets, &by, design::body(), w),
        Rect::new(x, panel.y + 96., w, 24.),
        design::body(),
        design::ink(0.6),
        false,
    );
    let mut y = panel.y + 168.;
    let description = entry.description.get(code);
    let lines = assets.lines(description, style, width).min(3) as f32;
    out.text(
        description,
        Rect::new(left, y, width, lines * 20. * 1.45 + 4.),
        style,
        design::ink(0.88),
        false,
    );
    y += lines * 20. * 1.45 + 36.;
    out.text(
        language.t("widgetConnectsTo").to_uppercase(),
        Rect::new(left, y, width, 20.),
        design::caps(),
        design::ink(0.5),
        false,
    );
    y += 32.;
    let mut cx = left;
    for origin in &entry.origins {
        let label = host(origin.as_str());
        let w = assets.width(label, text(15.2, 550, 0.01)) + 28.;
        if cx + w > left + width {
            cx = left;
            y += 40.;
        }
        cx = chip(out, assets, label, cx, y, (c(0xffffff, 0.07), WHITE));
    }
    y += 52.;
    for secret in &entry.secrets {
        let to: Vec<&str> = secret.send_to.iter().map(|o| host(o.as_str())).collect();
        let line = language.format(
            "widgetSendsSecret",
            &[
                ("secret", secret.label.get(code)),
                ("origins", &to.join(", ")),
            ],
        );
        out.text(
            fit(assets, &line, design::body(), width),
            Rect::new(left, y, width, 24.),
            design::body(),
            design::ink(0.7),
            false,
        );
        y += 34.;
    }
    // Why something cannot be done, or what an update asks for.
    let note = if entry.luma > crate::widget_state::luma_version() {
        Some(language.format("widgetNeedsLuma", &[("version", &entry.luma.to_string())]))
    } else if state.widgets.installing.contains(&entry.id) {
        Some(language.t("widgetInstalling").to_owned())
    } else if state.widget_needs_consent(&entry.id) {
        let installed = state.widgets.store.manifest(&entry.id);
        let new: Vec<&str> = entry
            .origins
            .iter()
            .filter(|o| installed.as_ref().is_none_or(|m| !m.origins.contains(o)))
            .map(|o| host(o.as_str()))
            .collect();
        Some(language.format("widgetConsent", &[("origins", &new.join(", "))]))
    } else {
        None
    };
    if let Some(note) = note {
        out.text(
            fit(assets, &note, design::body(), width),
            Rect::new(left, y + 6., width, 24.),
            design::body(),
            c(0xf7d88a, 1.),
            false,
        );
    }
    // The buttons along the bottom.
    let mut bx = left;
    let by = panel.y + panel.h - 48. - 60.;
    for (index, action) in state.store_actions(&entry.id).into_iter().enumerate() {
        let label = language.t(action.key());
        let style = text(19.2, 600, -0.01);
        let rect = Rect::new(bx, by, assets.width(label, style) + 64., 60.);
        let target = Target::StoreAction(index);
        let focused = state.focus == target && !state.pointer;
        if focused {
            out.draw.push(Draw::RoundedGradient(
                rect,
                30.,
                c(0xfafafa, 1.),
                c(0xe5e5e5, 1.),
            ));
        } else {
            out.shape(rect, 30., c(0xffffff, 0.08));
            out.border(rect, 30., 1., c(0xffffff, 0.1));
        }
        out.text(
            label,
            Rect::new(rect.x, rect.y + 17., rect.w, 26.),
            style,
            if focused {
                c(0x2c2c2c, 1.)
            } else if action == StoreAction::Remove {
                design::color(design::DOWN, 1.)
            } else {
                WHITE
            },
            true,
        );
        out.control(target, rect, false);
        bx += rect.w + 16.;
    }
}

/// A store widget's settings page: a row per setting, three per secret, then the secret's hint.
pub(super) fn settings(out: &mut Scene, state: &State, assets: &Assets, slot: u8) {
    let language = state.preferences.language;
    let code = language.code();
    let s = state.scroll;
    let Some(manifest) = state.slot_manifest(slot) else {
        return;
    };
    let values = state.slot_settings(slot);
    let rows = state.widget_setting_rows(slot);
    let mut hint = None;
    for (position, row_kind) in rows.iter().enumerate() {
        let (title, icon, value, chevron) = match *row_kind {
            WidgetRow::Setting(index) => {
                let setting = &manifest.settings[index];
                let title = setting.label.get(code).to_owned();
                match &setting.kind {
                    SettingKind::Toggle { .. } => (
                        title,
                        "sparkles",
                        language
                            .t(
                                if values.get(&setting.id).and_then(|v| v.as_bool()) == Some(true) {
                                    "stateOn"
                                } else {
                                    "stateOff"
                                },
                            )
                            .to_owned(),
                        false,
                    ),
                    SettingKind::Choice { .. } | SettingKind::Choices { .. } => {
                        let chosen: Vec<String> = state
                            .widget_options(slot, index)
                            .into_iter()
                            .filter(|(_, chosen)| *chosen)
                            .map(|(label, _)| label)
                            .collect();
                        (title, "grid", chosen.join(", "), true)
                    }
                    SettingKind::Text { .. } => (
                        title,
                        "type",
                        values
                            .get(&setting.id)
                            .and_then(|v| v.as_str())
                            .filter(|v| !v.is_empty())
                            .map_or_else(|| language.t("widgetNotSet").to_owned(), str::to_owned),
                        true,
                    ),
                    SettingKind::Secret { .. } => (title, "type", String::new(), true),
                }
            }
            WidgetRow::SecretType(index) => {
                let setting = &manifest.settings[index];
                if let SettingKind::Secret {
                    hint: Some(text), ..
                } = &setting.kind
                {
                    hint.get_or_insert_with(|| text.get(code).to_owned());
                }
                let masked = state
                    .widgets
                    .store
                    .secret(&manifest.id, &setting.id)
                    .map_or_else(
                        || language.t("widgetNotSet").to_owned(),
                        |secret| {
                            let tail: String = secret
                                .chars()
                                .rev()
                                .take(4)
                                .collect::<Vec<_>>()
                                .into_iter()
                                .rev()
                                .collect();
                            format!("•••• {tail}")
                        },
                    );
                (setting.label.get(code).to_owned(), "type", masked, true)
            }
            WidgetRow::SecretPhone(_) => (
                language.t("widgetSecretPhone").to_owned(),
                "sparkles",
                String::new(),
                true,
            ),
            WidgetRow::SecretForget(index) => (
                language.format(
                    "widgetSecretForget",
                    &[("secret", manifest.settings[index].label.get(code))],
                ),
                "close",
                String::new(),
                false,
            ),
        };
        super::row(
            out,
            state,
            assets,
            Row {
                target: Target::WidgetSetting(position),
                rect: Rect::new(640., 164.8 + position as f32 * 84. - s, 1120., 72.),
                title: &title,
                icon: Some(icon),
                value: (!value.is_empty()).then_some(value.as_str()),
                description: None,
                selected: false,
                chevron,
            },
        );
    }
    if let Some(hint) = hint {
        out.text(
            &hint,
            Rect::new(660., 164.8 + rows.len() as f32 * 84. + 8. - s, 1080., 84.),
            text(16., 400, 0.).leading(1.5),
            c(0xf5f5f5, 0.55),
            false,
        );
    }
}

/// A loaded image from the feeds' cache, by its URL.
fn feed_image<'a>(
    state: &'a State,
    url: &Option<String>,
) -> Option<&'a std::sync::Arc<crate::app_artwork::Artwork>> {
    url.as_ref().and_then(|url| state.feeds.images.get(url))
}

/// An image, or the initials of `name` on a disc until it arrives (or when there is none),
/// in `tint` when given.
fn emblem(
    out: &mut Scene,
    state: &State,
    image: &Option<String>,
    name: &str,
    rect: Rect,
    tint: Option<Color>,
) {
    if let Some(image) = feed_image(state, image) {
        out.draw.push(Draw::AppImage(image.clone(), rect, [1.; 4]));
        return;
    }
    match tint {
        Some(tint) => {
            out.draw.push(Draw::RoundedGradient(
                rect,
                rect.w / 2.,
                alpha(tint, 0.42),
                alpha(tint, 0.2),
            ));
            out.border(rect, rect.w / 2., 1., alpha(tint, 0.6));
        }
        None => {
            out.shape(rect, rect.w / 2., c(0xffffff, 0.08));
            out.border(rect, rect.w / 2., 1., c(0xffffff, 0.10));
        }
    }
    let initials: String = name
        .chars()
        .filter(|c| c.is_alphanumeric())
        .take(2)
        .collect::<String>()
        .to_uppercase();
    let size = (rect.h * 0.36).max(10.);
    out.text(
        if initials.is_empty() {
            "?".into()
        } else {
            initials
        },
        Rect::new(
            rect.x,
            rect.y + (rect.h - size * 1.25) / 2.,
            rect.w,
            size * 1.25,
        ),
        text(size, 650, 0.02),
        c(0xf5f5f5, 0.78),
        true,
    );
}

/// A small label on a rounded fill; returns where the next one starts.
fn chip(
    out: &mut Scene,
    assets: &Assets,
    label: &str,
    x: f32,
    y: f32,
    tone: (Color, Color),
) -> f32 {
    let style = text(15.2, 550, 0.01);
    let w = assets.width(label, style) + 28.;
    let rect = Rect::new(x, y, w, 30.);
    out.shape(rect, 15., tone.0);
    out.text(
        label,
        Rect::new(rect.x, rect.y + 6., w, 19.),
        style,
        tone.1,
        true,
    );
    x + w + 10.
}

/// The red LIVE badge centred in `rect`'s width at its top.
fn live_badge(out: &mut Scene, language: Language, rect: Rect, size: f32) {
    let w = (size * 4.).max(52.);
    let h = size * 1.7;
    let badge = Rect::new(rect.x + (rect.w - w) / 2., rect.y, w, h);
    out.shape(badge, h * 0.28, c(0xe5484d, 1.));
    out.text(
        language.t("live"),
        Rect::new(badge.x, badge.y + (h - size * 1.25) / 2., w, size * 1.25),
        text(size, 650, 0.06),
        WHITE,
        true,
    );
}

/// The local time of an RFC 3339 instant, with the day when it is not today.
fn match_time(begin_at: &str, now: OffsetDateTime, language: Language) -> String {
    OffsetDateTime::parse(begin_at, &time::format_description::well_known::Rfc3339)
        .map_or(String::new(), |start| local_time(start, now, language))
}

/// An instant in the local time of `now`, with the day when it is not today.
fn local_time(instant: OffsetDateTime, now: OffsetDateTime, language: Language) -> String {
    let local = instant.to_offset(now.offset());
    let clock = format!("{}:{:02}", local.hour(), local.minute());
    match (local.date() - now.date()).whole_days() {
        0 => clock,
        1 => format!("{} {clock}", language.t("tomorrow")),
        _ => match language {
            Language::En => format!("{}/{} {clock}", local.month() as u8, local.day()),
            Language::Zh => format!("{}月{}日 {clock}", local.month() as u8, local.day()),
            _ => format!("{:02}.{:02} {clock}", local.day(), local.month() as u8),
        },
    }
}

/// Whole minutes since an RFC 3339 instant.
fn minutes_since(begin_at: &str, now: OffsetDateTime) -> Option<i64> {
    let start =
        OffsetDateTime::parse(begin_at, &time::format_description::well_known::Rfc3339).ok()?;
    Some((now - start).whole_minutes()).filter(|minutes| *minutes >= 0)
}

/// Sending the key from a phone: the QR code, numbered steps and the address to type instead.
pub(super) fn phone_key_dialog(out: &mut Scene, state: &State, assets: &Assets) {
    let language = state.preferences.language;
    let panel = Rect::new(360., 230., 1200., 620.);
    out.modal = Some(panel);
    out.shadow(panel, 27.2, 120., 25., 0.47);
    out.draw.push(Draw::RoundedGradient(
        panel,
        27.2,
        c(0x34383b, 0.97),
        c(0x24282d, 0.99),
    ));
    out.border(panel, 27.2, 1., c(0xffffff, 0.12));
    let close = Rect::new(panel.x + panel.w - 72., panel.y + 36., 33.6, 33.6);
    out.shape(close, 16.8, c(0xffffff, 0.035));
    out.border(close, 16.8, 1., c(0xffffff, 0.07));
    out.icon(assets, "close", close.inset(7.6), WHITE);
    if state.focus == Target::CloseDialog {
        out.border(close.inset(1.), 15.8, 2., WHITE);
    }
    out.control(Target::CloseDialog, close, false);
    out.text(
        language.t("phoneKeyTitle"),
        Rect::new(panel.x + 48., panel.y + 40., panel.w - 160., 44.),
        text(32., 500, -0.04),
        WHITE,
        false,
    );
    let code = Rect::new(panel.x + 48., panel.y + 120., 400., 400.);
    out.shape(code, 18., c(0xffffff, 1.));
    let x = code.x + code.w + 48.;
    let Some((url, qr)) = &state.phone_key else {
        out.text(
            language.t("phoneKeyStarting"),
            Rect::new(x, code.y + 10., panel.x + panel.w - x - 48., 28.),
            text(20., 450, 0.),
            c(0xf5f5f5, 0.7),
            false,
        );
        return;
    };
    out.draw
        .push(Draw::AppImage(qr.clone(), code.inset(14.), [1.; 4]));
    let width = panel.x + panel.w - x - 48.;
    let step = text(20., 450, -0.01).leading(1.35);
    let mut y = code.y;
    let (label, hint) = state
        .phone_widget_secret()
        .map(|(label, hint, _)| (label, hint))
        .unwrap_or_default();
    let third = hint.unwrap_or_else(|| language.format("widgetPhoneStep3", &[("secret", &label)]));
    let steps = [
        language.t("phoneKeyStep1").to_owned(),
        language.t("phoneKeyStep2").to_owned(),
        third,
        language.t("phoneKeyStep4").to_owned(),
    ];
    for (number, item) in steps.iter().enumerate() {
        let line = format!("{}. {item}", number + 1);
        let height = assets.lines(&line, step, width) as f32 * step.size * step.line_height;
        out.text(
            &line,
            Rect::new(x, y, width, height + 4.),
            step,
            WHITE,
            false,
        );
        y += height + 18.;
    }
    out.text(
        language.t("phoneKeyAddress"),
        Rect::new(x, y + 12., width, 22.),
        text(16., 450, 0.),
        c(0xf5f5f5, 0.6),
        false,
    );
    out.text(
        url,
        Rect::new(x, y + 40., width, 28.),
        text(22., 600, 0.),
        WHITE,
        false,
    );
}
