//! A quiet, television-distance carousel: no parallax, bounce or moving hit targets.
use super::*;
use crate::recents::Command;

pub fn render(out: &mut Scene, state: &State, assets: &Assets) {
    let language = state.preferences.language;
    let recent = &state.recents;
    out.modal = Some(Rect::new(0., 0., 1920., 1080.));
    out.draw.push(Draw::RoundedGradient(
        Rect::new(0., 0., 1920., 1080.),
        0.,
        c(0x252c3a, 0.38),
        c(0x0b0e15, 0.50),
    ));
    out.text(
        language.t("recentApps"),
        Rect::new(84., 122., 1450., 65.),
        text(52., 600, -0.045),
        WHITE,
        false,
    );
    out.text(
        language.t(if assets.artwork.enabled() {
            "recentHint"
        } else {
            "recentDesktop"
        }),
        Rect::new(86., 199., 1500., 35.),
        text(22., 400, -0.015),
        c(0xe8ebf4, 0.62),
        false,
    );
    let dismiss = Rect::new(1776., 132., 60., 60.);
    button(
        out,
        state,
        assets,
        Target::CloseDialog,
        dismiss,
        "close",
        None,
    );

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
        if !loading && recent.error.is_none() {
            out.text(
                language.t("recentEmptyHint"),
                Rect::new(360., 596., 1200., 42.),
                text(21., 400, -0.01),
                c(0xffffff, 0.55),
                true,
            );
        }
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
            let focused = state.focus == target;
            let selected = index == recent.selected;
            out.shadow(rect, 24., 40., 18., if selected { 0.55 } else { 0.30 });
            out.draw.push(Draw::RoundedGradient(
                rect,
                24.,
                c(0x303643, 1.),
                c(0x171c27, 1.),
            ));

            let preview = entry
                .preview
                .as_ref()
                .and_then(|source| assets.artwork.get(source));
            if let Some(image) = preview {
                image_fit(out, image, rect.inset(16.));
            } else {
                let app = state
                    .inventory
                    .apps
                    .iter()
                    .find(|app| app.id == entry.app.id)
                    .unwrap_or(&entry.app);
                if let Some(brand) = crate::branding::for_key(&app.brand) {
                    let (ratio, height) = brand.artwork_size();
                    out.image(
                        assets.path(format!("surfaces/{}.png", app.brand)),
                        rect,
                        24.,
                        false,
                    );
                    let width = (rect.w * ratio).min(450.);
                    let height = (height * 1.8).min(160.);
                    out.image(
                        assets.path(format!("logos/{}.lvec", app.brand)),
                        Rect::new(
                            rect.x + (rect.w - width) / 2.,
                            rect.y + (rect.h - height) / 2.,
                            width,
                            height,
                        ),
                        0.,
                        true,
                    );
                } else if let Some(image) = app
                    .icon
                    .as_ref()
                    .and_then(|source| assets.artwork.get(source))
                {
                    image_fit(
                        out,
                        image,
                        Rect::new(rect.x + 230., rect.y + 81., 220., 220.),
                    );
                } else {
                    brand_mark(out, assets, app, rect, language);
                }
            }
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
            out.text(
                &entry.app.title,
                Rect::new(x + 8., 268., 594., 44.),
                text(27., 550, -0.02),
                WHITE,
                false,
            );
            button(
                out,
                state,
                assets,
                Target::RecentClose(entry.app.id.clone()),
                Rect::new(x + 624., 264., 48., 48.),
                "close",
                None,
            );
            if !selected && !focused && state.focus != Target::RecentClose(entry.app.id.clone()) {
                out.fade(start, 0.55, 0.85);
            }
        }
        out.draw.push(Draw::Clip(None));
        out.text(
            format!("{} / {}", recent.selected + 1, recent.entries.len()),
            Rect::new(760., 768., 400., 32.),
            text(18., 500, 0.04),
            c(0xffffff, 0.45),
            true,
        );
        let label = language.t("recentCloseAll");
        let width = (assets.width(label, text(21., 550, -0.01)) + 90.).max(218.);
        button(
            out,
            state,
            assets,
            Target::RecentCloseAll,
            Rect::new(960. - width / 2., 858., width, 60.),
            "close",
            Some(label),
        );
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
        let width = assets.width(label, text(21., 550, -0.01)) + 90.;
        button(
            out,
            state,
            assets,
            Target::RecentRetry,
            Rect::new(84., 858., width, 60.),
            "refresh",
            Some(label),
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
    out.text(
        language.t("recentControls"),
        Rect::new(180., 1010., 1560., 30.),
        text(17., 450, 0.005),
        c(0xffffff, 0.48),
        true,
    );
}

fn image_fit(out: &mut Scene, image: std::sync::Arc<crate::app_artwork::Artwork>, rect: Rect) {
    let scale = (rect.w / image.pixels.width() as f32).min(rect.h / image.pixels.height() as f32);
    let w = image.pixels.width() as f32 * scale;
    let h = image.pixels.height() as f32 * scale;
    out.draw.push(Draw::AppImage(
        image,
        Rect::new(rect.x + (rect.w - w) / 2., rect.y + (rect.h - h) / 2., w, h),
        [1.; 4],
    ));
}

fn button(
    out: &mut Scene,
    state: &State,
    assets: &Assets,
    target: Target,
    rect: Rect,
    icon: &str,
    label: Option<&str>,
) {
    let focused = state.focus == target;
    let busy = state.recents.pending.is_some() && target != Target::CloseDialog;
    out.pill(rect, focused && !busy, false);
    let color = if focused && !busy {
        c(0x20232a, 1.)
    } else {
        c(0xffffff, if busy { 0.30 } else { 0.8 })
    };
    out.icon(
        assets,
        icon,
        Rect::new(
            rect.x
                + if label.is_some() {
                    21.
                } else {
                    (rect.w - 22.) / 2.
                },
            rect.y + (rect.h - 22.) / 2.,
            22.,
            22.,
        ),
        color,
    );
    if let Some(label) = label {
        out.text(
            label,
            Rect::new(
                rect.x + 56.,
                rect.y + (rect.h - 27.) / 2.,
                rect.w - 72.,
                32.,
            ),
            text(21., 550, -0.01),
            color,
            false,
        );
    }
    if state.pressed.as_ref() == Some(&target) {
        out.border(rect.inset(2.), rect.h / 2., 2., c(0xffffff, 0.4));
    }
    out.control(target, rect, false);
}
