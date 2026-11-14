use crate::{
    model::{ChannelProgram, Preferences, demo_inventory},
    state::{Direction, Effect, Overlay, State, Target},
};

fn with_favorites() -> State {
    let mut state = State::new(Preferences::default());
    state.set_inventory(demo_inventory(), None);
    for channel in state.inventory.channels.clone().into_iter().take(3) {
        assert!(state.preferences.toggle_channel_favorite(&channel.id));
    }
    state.open(0);
    let id = state.home_channels()[1].id.clone();
    state.focus = Target::Channel(id);
    state
}

#[test]
fn back_on_home_channel_opens_switcher_not_recents() {
    let mut state = with_favorites();
    let id = match &state.focus {
        Target::Channel(id) => id.clone(),
        other => panic!("expected channel focus, got {other:?}"),
    };
    assert_eq!(
        state.back(),
        vec![Effect::RefreshChannelGuide],
        "opening the switcher refreshes guide titles"
    );
    assert!(state.channel_switcher_open());
    assert!(!state.recents_open());
    assert!(matches!(
        state.overlay,
        Some(Overlay::ChannelSwitcher { .. })
    ));
    assert_eq!(state.focus, Target::ChannelSwitch(id));
}

#[test]
fn hold_back_on_home_channel_opens_switcher() {
    let mut state = with_favorites();
    assert_eq!(state.open_back_overlay(), vec![Effect::RefreshChannelGuide]);
    assert!(state.channel_switcher_open());
    assert!(!state.recents_open());
}

#[test]
fn switcher_ok_tunes_selected_channel() {
    let mut state = with_favorites();
    state.preferences.reduced_motion = true;
    state.open_channel_switcher();
    let right = state.home_channels()[2].id.clone();
    state.channel_switcher_direction(Direction::Right);
    assert_eq!(state.focus, Target::ChannelSwitch(right.clone()));
    assert_eq!(state.activate(), vec![Effect::OpenChannel(right)]);
    assert!(!state.channel_switcher_open());
}

#[test]
fn switcher_back_and_up_restore_prior_focus() {
    let mut state = with_favorites();
    let prior = state.focus.clone();
    state.preferences.reduced_motion = true;
    state.open_channel_switcher();
    assert!(state.channel_switcher_open());
    assert!(state.back().is_empty());
    assert!(!state.channel_switcher_open());
    assert_eq!(state.focus, prior);

    state.focus = prior.clone();
    state.open_channel_switcher();
    assert_eq!(
        state.channel_switcher_direction(Direction::Up),
        Vec::<Effect>::new()
    );
    assert!(!state.channel_switcher_open());
    assert_eq!(state.focus, prior);
}

#[test]
fn channels_widget_toggle_hides_home_strip_and_left_closes_to_widgets_row() {
    use crate::state::Subpage;
    let mut state = with_favorites();
    assert!(!state.home_channels().is_empty());
    state.open(2);
    state.section = crate::state::WIDGETS_SECTION;
    state.focus = Target::Widget(5);
    state.activate();
    assert_eq!(state.subpage, Some(Subpage::Channels));
    assert_eq!(state.focus, Target::ChannelsToggle);
    assert_eq!(state.activate(), vec![Effect::Save]);
    assert!(!state.preferences.show_channels);
    assert!(state.home_channels().is_empty());
    assert!(
        !state.channels_settings_list().is_empty(),
        "settings list stays editable while the strip is hidden"
    );
    state.focus = Target::ChannelFavorite(state.channels_settings_list()[0].id.clone());
    state.direction(Direction::Left, &[]);
    assert!(state.subpage.is_none());
    assert_eq!(state.focus, Target::Widget(5));
    state.activate();
    state.focus = Target::SettingsMenu;
    state.direction(Direction::Left, &[]);
    assert!(state.subpage.is_none());
    assert_eq!(state.focus, Target::Widget(5));
}

#[test]
fn guide_entry_applies_now_and_next_titles() {
    let mut state = with_favorites();
    let id = state.home_channels()[0].id.clone();
    let mut programs = std::collections::HashMap::new();
    programs.insert(
        id.clone(),
        crate::model::ChannelGuideEntry {
            program: Some(ChannelProgram {
                title: "Now show".into(),
                start: None,
                end: None,
            }),
            next: Some(ChannelProgram {
                title: "Next show".into(),
                start: None,
                end: None,
            }),
        },
    );
    state.set_channel_guide(programs);
    let channel = state
        .inventory
        .channels
        .iter()
        .find(|channel| channel.id == id)
        .unwrap();
    assert_eq!(
        channel.program.as_ref().map(|p| p.title.as_str()),
        Some("Now show")
    );
    assert_eq!(
        channel.next.as_ref().map(|p| p.title.as_str()),
        Some("Next show")
    );
}
