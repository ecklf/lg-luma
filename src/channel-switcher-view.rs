//! Recents-style landscape cards for Home channel favorites, with now/next EPG.
use super::*;
use time::{OffsetDateTime, UtcOffset};

pub fn render(out: &mut Scene, state: &State, assets: &Assets) {
    let language = state.preferences.language;
    let switcher = &state.channel_switcher;
    let channels = state.home_channels();
    let focus = switcher_focus(state);
    out.modal = Some(Rect::new(0., 0., 1920., 1080.));
    out.draw.push(Draw::RoundedGradient(
        Rect::new(0., 0., 1920., 1080.),
        0.,
        c(0x252c3a, 0.38),
        c(0x0b0e15, 0.50),
    ));
    if channels.is_empty() {
        out.icon(
            assets,
            "alert",
            Rect::new(924., 410., 72., 72.),
            c(0xffffff, 0.40),
        );
        out.text(
            language.t("noChannels"),
            Rect::new(360., 526., 1200., 60.),
            text(32., 500, -0.025),
            WHITE,
            true,
        );
        return;
    }

    let mut cards: Vec<_> = channels
        .iter()
        .enumerate()
        .map(|(index, channel)| (*channel, switcher.offset(index)))
        .collect();
    cards.sort_by(|a, b| b.1.abs().total_cmp(&a.1.abs()));
    for (channel, offset) in cards {
        let depth = offset.abs();
        if depth > 4. {
            continue;
        }
        let rect = recents_view::card_rect(offset);
        if rect.y + rect.h + 80. < 0. || rect.x + rect.w + 64. < 0. || rect.x - 64. > 1920. {
            continue;
        }
        let start = out.draw.len();
        let target = Target::ChannelSwitch(channel.id.clone());
        let focused = *focus == target;
        let radius = 26. * rect.w / 900.;
        let prominence = (1. - depth).clamp(0., 1.);
        out.shadow(rect, radius, 48., 22., 0.28 + 0.27 * prominence);
        out.glass(rect.inset(-6.), radius + 6.);
        out.shape(rect, radius, c(0x141820, 0.92));
        channel_face(out, assets, channel, rect, radius);
        if focused {
            channel_epg(out, state, assets, channel, rect);
        } else {
            channel_caption(out, channel, rect, false);
        }
        out.border(
            rect,
            radius,
            if focused { 3. } else { 1. },
            c(0xffffff, if focused { 0.85 } else { 0.16 }),
        );
        if focused {
            out.border(rect.inset(-6.), radius + 6., 2., c(0xffffff, 0.10));
        }
        out.control(target.clone(), rect, false);
        if state.pressed.as_ref() == Some(&target) {
            out.fade(start, 1., 0.83);
        }
        out.fade(start, 1., 1. - 0.17 * depth.min(3.));
    }
}

fn switcher_focus(state: &State) -> &Target {
    if state.channel_switcher_open() {
        &state.focus
    } else {
        state
            .channel_switcher
            .closing_focus
            .as_ref()
            .unwrap_or(&state.focus)
    }
}

fn channel_face(
    out: &mut Scene,
    assets: &Assets,
    channel: &crate::model::Channel,
    rect: Rect,
    radius: f32,
) {
    let logo = Rect::new(
        rect.x + rect.w * 0.08,
        rect.y + rect.h * 0.12,
        rect.w * 0.84,
        rect.h * 0.42,
    );
    if let Some(source) = &channel.icon
        && let Some(art) = assets.artwork.get(source)
    {
        out.draw
            .push(Draw::AppImage(art, logo, radius * 0.35, [1., 1., 1., 1.]));
    } else {
        let number = if channel.number.is_empty() {
            "·"
        } else {
            &channel.number
        };
        out.text(
            number,
            Rect::new(logo.x, logo.y + logo.h * 0.18, logo.w, logo.h * 0.45),
            text(56. * rect.w / 900., 650, -0.03),
            WHITE,
            true,
        );
    }
}

fn channel_caption(out: &mut Scene, channel: &crate::model::Channel, rect: Rect, focused: bool) {
    let heading = channel_heading(channel);
    out.text(
        &heading,
        Rect::new(
            rect.x + rect.w * 0.08,
            rect.y + rect.h * (if focused { 0.58 } else { 0.62 }),
            rect.w * 0.84,
            40. * rect.w / 900.,
        ),
        text(if focused { 28. } else { 24. } * rect.w / 900., 600, -0.02),
        c(0xffffff, if focused { 0.95 } else { 0.78 }),
        true,
    );
}

fn channel_epg(
    out: &mut Scene,
    state: &State,
    assets: &Assets,
    channel: &crate::model::Channel,
    rect: Rect,
) {
    let language = state.preferences.language;
    channel_caption(out, channel, rect, true);
    let now_title = channel
        .program
        .as_ref()
        .map(|program| program.title.as_str())
        .filter(|title| !title.is_empty());
    let now_line = match now_title {
        Some(title) => {
            if let Some(range) = program_range(state, channel.program.as_ref()) {
                format!("{title}  ·  {range}")
            } else {
                title.to_string()
            }
        }
        None => channel.name.clone(),
    };
    let text_x = rect.x + rect.w * 0.08;
    let text_w = rect.w * 0.84;
    let scale = rect.w / 900.;
    out.text(
        &now_line,
        Rect::new(text_x, rect.y + rect.h * 0.70, text_w, 36. * scale),
        text(22. * scale, 500, -0.015).leading(1.15),
        c(0xffffff, 0.90),
        true,
    );
    if let Some(next) = channel
        .next
        .as_ref()
        .map(|program| program.title.as_str())
        .filter(|title| !title.is_empty())
    {
        let label = language.t("upNext");
        let width = assets.width(label, text(18. * scale, 550, -0.01));
        out.text(
            label,
            Rect::new(
                text_x,
                rect.y + rect.h * 0.82,
                width.min(text_w),
                28. * scale,
            ),
            text(18. * scale, 550, -0.01),
            c(0xffffff, 0.55),
            false,
        );
        out.text(
            next,
            Rect::new(
                text_x + width.min(text_w) + 14. * scale,
                rect.y + rect.h * 0.82,
                (text_w - width.min(text_w) - 14. * scale).max(0.),
                28. * scale,
            ),
            text(18. * scale, 500, -0.01),
            c(0xffffff, 0.72),
            false,
        );
    }
}

fn channel_heading(channel: &crate::model::Channel) -> String {
    if channel.number.is_empty() {
        channel.name.clone()
    } else {
        format!("{} · {}", channel.number, channel.name)
    }
}

fn program_range(state: &State, program: Option<&crate::model::ChannelProgram>) -> Option<String> {
    let program = program?;
    let start = program.start?;
    let end = program.end?;
    let offset = UtcOffset::current_local_offset().unwrap_or(UtcOffset::UTC);
    let start = OffsetDateTime::from_unix_timestamp(start as i64)
        .ok()?
        .to_offset(offset);
    let end = OffsetDateTime::from_unix_timestamp(end as i64)
        .ok()?
        .to_offset(offset);
    let language = state.preferences.language;
    let hour24 = state.preferences.hour_24;
    Some(format!(
        "{}–{}",
        language.clock(start, hour24, false),
        language.clock(end, hour24, false)
    ))
}
