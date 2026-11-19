use super::*;
use crate::{renderer::Color, renderer::Draw, state::Rect, state::Subpage};

fn luminance(color: Color) -> f32 {
    color[..3]
        .iter()
        .zip([0.2126, 0.7152, 0.0722])
        .map(|(&channel, weight)| {
            weight
                * if channel <= 0.04045 {
                    channel / 12.92
                } else {
                    ((channel + 0.055) / 1.055).powf(2.4)
                }
        })
        .sum()
}

fn contrast(foreground: Color, background: Color) -> f32 {
    let mut blended = foreground;
    for channel in 0..3 {
        blended[channel] =
            foreground[channel] * foreground[3] + background[channel] * (1. - foreground[3]);
    }
    let a = luminance(blended);
    let b = luminance(background);
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

fn same_rect(a: Rect, b: Rect) -> bool {
    (a.x, a.y, a.w, a.h) == (b.x, b.y, b.w, b.h)
}

fn assert_readable_focus(state: &State) {
    let scene = scene(state);
    let rect = scene
        .controls
        .iter()
        .find(|control| control.target == state.focus)
        .expect("focused settings control must be rendered")
        .rect;
    let fill = scene
        .draw
        .iter()
        .find_map(|draw| match draw {
            Draw::Glass(area, _, color) if same_rect(*area, rect) => Some(*color),
            Draw::Shape(area, _, 0., color)
                if state.focus == Target::UseBackground && same_rect(*area, rect) =>
            {
                Some(*color)
            }
            _ => None,
        })
        .expect("focused controls must retain their bright highlight surface");
    assert!(fill[3] >= 0.96);
    assert!(luminance(fill) > 0.5);
    let mut labels = 0;
    for draw in &scene.draw {
        let (area, color, minimum) = match draw {
            Draw::Text(label, area, _, color, _) if !label.is_empty() => (*area, *color, 4.5),
            Draw::TintedImage(_, area, _, _, color) => (*area, *color, 3.),
            _ => continue,
        };
        if rect.contains(area.center().0, area.center().1) {
            assert!(
                contrast(color, fill) >= minimum,
                "{:?}: {:?} has insufficient contrast against {:?}",
                state.focus,
                color,
                fill
            );
            labels += 1;
        }
    }
    assert!(labels > 0);
}

#[test]
fn focused_corner_settings_and_wallpaper_actions_keep_dark_labels() {
    for language in crate::i18n::Language::ALL {
        let mut state = ready();
        state.preferences.language = language;
        state.overlay = Some(crate::state::Overlay::Settings);
        for target in [Target::LumaSettings, Target::TvSettings] {
            state.focus = target;
            assert_readable_focus(&state);
        }
        state.open(2);
        state.section = 1;
        let id = crate::model::BACKGROUNDS[0].0.to_owned();
        state.overlay = Some(crate::state::Overlay::BackgroundOptions {
            id,
            restore: Target::Background(0),
        });
        state.focus = Target::UseBackground;
        assert_readable_focus(&state);
    }
}

#[test]
fn focused_settings_sidebar_and_wallpaper_options_keep_contrast_in_every_language() {
    for language in crate::i18n::Language::ALL {
        let mut state = ready();
        state.open(2);
        state.preferences.language = language;
        for section in crate::state::SETTINGS_ORDER {
            state.section = section;
            state.focus = Target::Sidebar(section);
            assert_readable_focus(&state);
        }
        state.section = 1;
        for target in [
            Target::WallpaperGradient,
            Target::WallpaperCycle,
            Target::UploadWallpaper,
        ] {
            state.focus = target;
            assert_readable_focus(&state);
        }
    }
}

#[test]
fn focused_settings_breadcrumbs_values_and_descriptions_keep_contrast() {
    let mut state = ready();
    state.open(2);
    state.preferences.language = crate::i18n::Language::It;
    state.subpage = Some(Subpage::AppDrawerStyle);
    state.focus = Target::Sidebar(state.section);
    assert_readable_focus(&state);
    state.subpage = Some(Subpage::Language);
    state.focus = Target::Choice(5);
    assert_readable_focus(&state);
    state.subpage = Some(Subpage::RecentApps);
    // Exercise every option, including descriptions and selected checkmarks.
    let targets: Vec<_> = controls(&state)
        .into_iter()
        .filter(|control| control.scrollable)
        .map(|control| control.target)
        .collect();
    assert!(!targets.is_empty());
    for target in targets {
        state.focus = target;
        assert_readable_focus(&state);
    }
}
