use super::*;
use crate::{
    renderer::Draw,
    state::{SYSTEM_SECTION, Subpage},
};

fn opened() -> State {
    let mut state = ready();
    state.open(2);
    state.section = SYSTEM_SECTION;
    state.focus = Target::Setting(5);
    state.direction(Direction::Down, &controls(&state));
    assert_eq!(state.focus, Target::Attributions);
    assert!(state.activate().is_empty());
    assert_eq!(state.subpage, Some(Subpage::Attributions));
    assert_eq!(state.focus, Target::AttributionText);
    state
}

#[test]
fn attribution_action_has_a_copyright_icon_and_no_chevron() {
    let mut state = ready();
    state.open(2);
    state.section = SYSTEM_SECTION;
    let rendered = scene(&state);
    let rect = rendered
        .controls
        .iter()
        .find(|control| control.target == Target::Attributions)
        .unwrap()
        .rect;
    let icons: Vec<_> = rendered
        .draw
        .iter()
        .filter_map(|draw| match draw {
            Draw::TintedImage(path, icon, ..)
                if icon.y >= rect.y && icon.y + icon.h <= rect.y + rect.h =>
            {
                Some(path.as_str())
            }
            _ => None,
        })
        .collect();
    assert!(icons.iter().any(|path| path.ends_with("/copyright.lvec")));
    assert!(!icons.iter().any(|path| path.ends_with("/right.lvec")));
}

#[test]
fn attributions_is_read_only_and_restores_system_parent_for_remote_and_pointer() {
    let mut state = opened();
    let prefs = serde_json::to_value(&state.preferences).unwrap();
    assert!(!state.text_input_active());
    assert!(state.activate().is_empty());
    state.text("Do not edit the credits");
    assert!(!state.text_input_active());
    assert!(state.back().is_empty());
    assert_eq!(state.subpage, None);
    assert_eq!(state.section, SYSTEM_SECTION);
    assert_eq!(state.focus, Target::Attributions);
    let rendered = scene(&state);
    let row = rendered
        .controls
        .iter()
        .find(|c| c.target == Target::Attributions)
        .unwrap();
    let (x, y) = row.rect.center();
    assert_eq!(
        state.hit_test(&rendered.controls, x, y),
        Some(Target::Attributions)
    );
    state.pointer_focus(Target::Attributions);
    state.activate();
    state.direction(Direction::Left, &controls(&state));
    assert_eq!(state.focus, Target::Attributions);
    assert_eq!(serde_json::to_value(&state.preferences).unwrap(), prefs);
}

#[test]
fn credits_scroll_with_remote_and_wheel_are_clipped_and_reach_both_ends() {
    let mut state = opened();
    let field = state.attributions_viewport();
    assert!(scene(&state).draw.iter().any(|draw| matches!(draw,
        Draw::Text(label, ..) if label.starts_with("Luma — acknowledgements"))));
    for _ in 0..100 {
        state.direction(Direction::Down, &controls(&state));
    }
    let end = state.scroll;
    assert!(end > field.h);
    state.reveal_focus(&controls(&state));
    assert_eq!(state.scroll, end);
    let rendered = scene(&state);
    assert!(rendered.draw.iter().any(|draw| matches!(draw,
        Draw::Text(label, rect, ..) if label.starts_with("Thank you") && rect.y + rect.h <= field.y + field.h)));
    assert!(rendered.draw.iter().any(
        |draw| matches!(draw, Draw::Clip(Some(rect)) if rect.x > field.x && rect.w < field.w)
    ));
    assert_eq!(
        state.hit_test(&rendered.controls, field.x + 100., field.y + 100.),
        Some(Target::AttributionText)
    );
    assert_ne!(
        state.hit_test(&rendered.controls, field.x + 100., field.y - 10.),
        Some(Target::AttributionText)
    );
    state.direction(Direction::Down, &rendered.controls);
    assert_eq!(state.scroll, end);
    for _ in 0..100 {
        state.direction(Direction::Up, &controls(&state));
    }
    assert_eq!(state.scroll, 0.);
    state.wheel_scroll(&controls(&state), Some(field.center()), 0, -1000);
    assert_eq!(state.scroll, end);
    state.wheel_scroll(&controls(&state), Some((200., 200.)), 0, 1000);
    assert_eq!(
        state.scroll, end,
        "Scrolling over the sidebar must not move the text"
    );
    state.wheel_scroll(&controls(&state), Some(field.center()), 0, 1000);
    assert_eq!(state.scroll, 0.);
}

#[test]
fn system_attributions_label_is_localized_and_credits_remain_available_offline() {
    for language in crate::i18n::Language::ALL {
        let mut state = ready();
        state.preferences.language = language;
        state.open(2);
        state.section = SYSTEM_SECTION;
        assert!(scene(&state).draw.iter().any(|draw| matches!(draw,
            Draw::Text(label, ..) if label == language.t("attributions"))));
        state.focus = Target::Attributions;
        state.activate();
        assert_eq!(state.focus, Target::AttributionText);
        assert!(scene(&state).draw.iter().any(|draw| matches!(draw,
            Draw::Text(label, ..) if label.starts_with("Luma — acknowledgements"))));
    }
    let credits = include_str!("../assets/attributions.txt");
    for credit in [
        "Breezy Weather",
        "Lucide",
        "Kickstand",
        "Kuankuan",
        "MPL-2.0",
        "SIL Open Font",
        "Open-Meteo",
        "Inter",
        "Noto",
        "SDL",
        "Nayuki",
        "if-addrs",
        "Simple Icons",
        "Jellyfin",
    ] {
        assert!(credits.contains(credit), "Missing {credit}");
    }
    assert!(!credits.contains("Icons8"));
}
