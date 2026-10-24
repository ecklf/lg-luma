//! A quiet, television-distance carousel: no parallax, bounce or moving hit targets.
use super::*;
use crate::recents::Command;

pub fn render(out: &mut Scene, state: &State, assets: &Assets) {
    let language = state.preferences.language;
    let recent = &state.recents;
    let focus = recent_focus(state);
    out.modal = Some(Rect::new(0., 0., 1920., 1080.));
    out.draw.push(Draw::RoundedGradient(
        Rect::new(0., 0., 1920., 1080.),
        0.,
        c(0x252c3a, 0.38),
        c(0x0b0e15, 0.50),
    ));
    if recent.entries.is_empty() {
        let loading = recent.pending.is_some();
        out.icon(
            assets,
            if recent.error.is_some() {
                "alert"
            } else {
                "recent"
            },
            Rect::new(924., 410., 72., 72.),
            c(0xffffff, 0.40),
        );
        out.text(
            language.t(if recent.error.is_some() {
                "recentError"
            } else if loading {
                "recentLoading"
            } else {
                "recentEmpty"
            }),
            Rect::new(360., 526., 1200., 60.),
            text(32., 500, -0.025),
            WHITE,
            true,
        );
    } else {
        out.draw
            .push(Draw::Clip(Some(Rect::new(0., 250., 1920., 560.))));
        for (index, entry) in recent.entries.iter().enumerate() {
            let x = 620. + (index as f32 - recent.position) * 744.;
            if x + 712. < 0. || x - 32. > 1920. {
                continue;
            }
            let start = out.draw.len();
            let rect = Rect::new(x, 336., 680., 382.5);
            let target = Target::Recent(entry.app.id.clone());
            let focused = *focus == target;
            let selected = index == recent.selected;
            out.shadow(rect, 24., 40., 18., if selected { 0.55 } else { 0.30 });
            let app = state
                .inventory
                .apps
                .iter()
                .find(|app| app.id == entry.app.id)
                .unwrap_or(&entry.app);
            app_face(out, assets, app, rect, language, 24.);
            out.border(
                rect,
                24.,
                if focused { 3. } else { 1. },
                c(0xffffff, if focused { 0.95 } else { 0.13 }),
            );
            if focused {
                out.border(rect.inset(-6.), 30., 2., c(0xffffff, 0.12));
            }
            // Geometry depends on carousel position, never pointer hover/down.
            out.control(target.clone(), rect, false);
            if state.pressed.as_ref() == Some(&target) {
                out.fade(start, 1., 0.83);
            }
            if !selected && !focused {
                out.fade(start, 0.55, 0.85);
            }
        }
        out.draw.push(Draw::Clip(None));
    }

    if recent.error.is_some() {
        if !recent.entries.is_empty() {
            out.text(
                language.t("recentError"),
                Rect::new(420., 945., 1080., 34.),
                text(19., 450, 0.),
                c(0xf3ddbd, 1.),
                true,
            );
        }
        let label = language.t("retry");
        let width = assets.width(label, text(21., 550, -0.01)) + 74.;
        button(
            out,
            state,
            assets,
            Target::RecentRetry,
            Rect::new(84., 858., width, 60.),
            "refresh",
            label,
        );
    } else if let Some(command) = &recent.pending
        && !recent.entries.is_empty()
    {
        out.text(
            language.t(if matches!(command, Command::Launch(_)) {
                "opening"
            } else {
                "recentLoading"
            }),
            Rect::new(660., 945., 600., 30.),
            text(18., 450, 0.),
            c(0xffffff, 0.6),
            true,
        );
    }
}

fn recent_focus(state: &State) -> &Target {
    if state.recents_open() {
        &state.focus
    } else {
        state.recents.closing_focus.as_ref().unwrap_or(&state.focus)
    }
}

fn button(
    out: &mut Scene,
    state: &State,
    assets: &Assets,
    target: Target,
    rect: Rect,
    icon: &str,
    label: &str,
) {
    let focused = *recent_focus(state) == target;
    let busy = state.recents.pending.is_some();
    out.pill(rect, focused && !busy, false);
    let color = if focused && !busy {
        c(0x20232a, 1.)
    } else {
        c(0xffffff, if busy { 0.30 } else { 0.8 })
    };
    out.icon(
        assets,
        icon,
        Rect::new(rect.x + 20., rect.y + (rect.h - 22.) / 2., 22., 22.),
        color,
    );
    out.text(
        label,
        Rect::new(
            rect.x + 54.,
            rect.y + (rect.h - 27.) / 2.,
            rect.w - 74.,
            32.,
        ),
        text(21., 550, -0.01),
        color,
        false,
    );
    if state.pressed.as_ref() == Some(&target) {
        out.border(rect.inset(2.), rect.h / 2., 2., c(0xffffff, 0.4));
    }
    out.control(target, rect, false);
}
