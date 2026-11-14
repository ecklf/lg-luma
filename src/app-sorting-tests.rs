use super::*;
use crate::{model::AppSorting, state::Subpage};

fn fixture() -> State {
    let mut state = ready();
    state.inventory.apps = vec![
        App::new("z", "Zulu"),
        App::new("b", "beta"),
        App::new("a2", "Alpha"),
        App::new("a1", "alpha"),
        App::new("f2", "Favorite two"),
        App::new("f1", "Favorite one"),
        App::new("c", "Charlie"),
        App::new("d", "Delta"),
    ];
    state.preferences.favorites = Some(vec!["f2".into(), "f1".into()]);
    state.preferences.app_launch_counts = BTreeMap::from([
        ("z".into(), 8),
        ("b".into(), 4),
        ("a2".into(), 4),
        ("a1".into(), 4),
        ("f1".into(), 100),
    ]);
    manage_apps(&mut state);
    state
}

fn ids(apps: Vec<&App>) -> Vec<&str> {
    apps.into_iter().map(|app| app.id.as_str()).collect()
}

#[test]
fn app_sorting_defaults_to_alphabetical_and_round_trips_counts() {
    let old: Preferences = serde_json::from_str("{}").unwrap();
    assert_eq!(old.app_sorting, AppSorting::Alphabetical);
    assert!(old.app_launch_counts.is_empty());
    let mut state = fixture();
    assert_eq!(
        ids(state.apps()),
        ["f2", "f1", "a1", "a2", "b", "c", "d", "z"]
    );
    state.preferences.app_sorting = AppSorting::Usage;
    let loaded: Preferences =
        serde_json::from_slice(&serde_json::to_vec(&state.preferences).unwrap()).unwrap();
    assert_eq!(loaded.app_sorting, AppSorting::Usage);
    assert_eq!(
        loaded.app_launch_counts,
        state.preferences.app_launch_counts
    );
}

#[test]
fn usage_sorting_keeps_favorites_and_breaks_ties_by_name_then_id() {
    let mut state = fixture();
    state.preferences.app_sorting = AppSorting::Usage;
    assert_eq!(
        ids(state.apps()),
        ["f2", "f1", "z", "a1", "a2", "b", "c", "d"]
    );
    assert_eq!(ids(state.favorite_apps()), ["f2", "f1"]);
    assert_eq!(ids(state.dock_apps()), ["f2", "f1", "z", "a1", "a2", "b"]);
    assert_eq!(ids(state.drawer_apps()), ["c", "d"]);
    state.preferences.hidden.push("z".into());
    state.open(0);
    assert!(!ids(state.dock_apps()).contains(&"z"));
    assert!(!ids(state.drawer_apps()).contains(&"z"));
    state.preferences.app_sorting = AppSorting::Alphabetical;
    assert_eq!(state.preferences.app_launch_counts["z"], 8);
}

#[test]
fn launch_counts_work_in_both_modes_and_saturate_without_counting_non_apps() {
    let mut state = fixture();
    assert_eq!(state.record_app_launch("z"), [Effect::Save]);
    assert_eq!(state.preferences.app_launch_counts["z"], 9);
    state.preferences.app_sorting = AppSorting::Usage;
    state
        .preferences
        .app_launch_counts
        .insert("z".into(), u64::MAX);
    assert_eq!(state.record_app_launch("z"), [Effect::Save]);
    assert_eq!(state.preferences.app_launch_counts["z"], u64::MAX);
    for id in ["unknown", "com.webos.app.hdmi1", "com.palm.app.settings"] {
        assert!(state.record_app_launch(id).is_empty());
        assert!(!state.preferences.app_launch_counts.contains_key(id));
    }
}

#[test]
fn app_sorting_picker_supports_remote_pointer_and_parent_navigation() {
    use crate::renderer::Draw;
    for pointer in [false, true] {
        let mut state = ready();
        state.open(2);
        assert!(scene(&state).draw.iter().any(|draw| matches!(draw,
            Draw::TintedImage(path, ..) if path.ends_with("arrow-up-down.lvec"))));
        assert!(
            controls(&state)
                .iter()
                .any(|c| c.target == Target::Setting(10))
        );
        state.focus = Target::Setting(10);
        state.pointer = pointer;
        state.activate();
        assert_eq!(state.subpage, Some(Subpage::AppSorting));
        let rendered = scene(&state);
        for label in ["Alphabetically", "Usage count"] {
            assert!(rendered.draw.iter().any(|draw| matches!(draw,
                Draw::Text(text, ..) if text == label)));
        }
        assert_eq!(state.focus, Target::Choice(0));
        state.direction(Direction::Down, &controls(&state));
        assert_eq!(state.focus, Target::Choice(1));
        state.direction(Direction::Down, &controls(&state));
        assert_eq!(state.focus, Target::Choice(0));
        state.direction(Direction::Down, &controls(&state));
        assert_eq!(state.activate(), [Effect::Save]);
        assert_eq!(state.preferences.app_sorting, AppSorting::Usage);
        assert_eq!(state.focus, Target::Setting(10));
        state.activate();
        assert_eq!(state.focus, Target::Choice(1));
        state.back();
        assert_eq!(state.focus, Target::Setting(10));
    }
}
