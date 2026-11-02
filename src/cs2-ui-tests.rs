use super::*;

fn with_feed(state: &mut State) {
    let feed = crate::cs2::parse_feed(&serde_json::json!({
        "updated": 0,
        "staff": {"Team_Vitality": [{"name": "XTQZZZ", "role": "Head Coach"}]},
        "matches": [
            {"id": "a", "begin_at": "1970-01-01T00:00:00Z", "state": "live", "best_of": 3,
             "tournament": "IEM Chengdu", "tier": 1, "score": [1, 0],
             "round": {"key": "final", "bracket": "upper"},
             "teams": [{"name": "Team Vitality", "short": "Vitality", "page": "Team_Vitality"}, {"name": "FaZe Clan", "short": "FaZe"}],
             "maps": [{"map": "Mirage", "score": [13, 9], "winner": 0},
                      {"map": "Inferno", "winner": null}],
             "lineups": [[{"name": "ZywOo", "flag": "France"}], []]},
            {"id": "b", "begin_at": "1970-01-02T10:00:00Z", "state": "upcoming", "best_of": 3,
             "tournament": "PGL Masters", "stage": "Playoffs", "tier": 1,
             "teams": [{"name": "MOUZ"}, null], "maps": []}
        ]
    }))
    .unwrap();
    let (Some(command), _) = state.cs2.sync(true, true, 0) else {
        panic!();
    };
    state.cs2.accept(command.id, Ok(feed), 0);
}

#[test]
fn cs2_score_is_switched_in_widgets() {
    let mut state = ready();
    state.open(2);
    state.focus = Target::Widget(6);
    state.activate();
    assert_eq!(state.subpage, Some(crate::state::Subpage::Cs2));
    assert_eq!(state.focus, Target::Cs2Toggle);
    assert_eq!(state.activate(), vec![Effect::Save]);
    assert!(state.preferences.show_cs2);
    state.direction(Direction::Down, &controls(&state));
    assert_eq!(state.focus, Target::Cs2Major);
    state.activate();
    assert!(!state.preferences.cs2_major_only);
    assert!(
        state
            .direction(Direction::Right, &controls(&state))
            .is_empty()
    );
    state.direction(Direction::Left, &controls(&state));
    assert_eq!(state.focus, Target::Widget(6));
}

#[test]
fn cs2_cards_follow_inputs_and_open_their_match() {
    let mut state = ready();
    let positions = |scene: &view::Scene| -> Vec<_> {
        scene
            .controls
            .iter()
            .filter(|c| matches!(c.target, Target::App(_) | Target::Input(_)))
            .map(|c| (c.target.clone(), c.rect.x, c.rect.y))
            .collect()
    };
    let before = scene(&state);
    state.preferences.show_cs2 = true;
    with_feed(&mut state);
    let after = scene(&state);
    assert_eq!(positions(&before), positions(&after));
    let text: Vec<String> = after
        .draw
        .iter()
        .filter_map(|draw| match draw {
            crate::renderer::Draw::Text(text, ..) => Some(text.clone()),
            _ => None,
        })
        .collect();
    for expected in [
        "CS2 Score",
        "Powered by Liquipedia",
        "LIVE",
        "Team Vitality",
        "Map 2 · Inferno",
        "Tomorrow 10:00 AM",
        "TBD",
        "BO3 · Playoffs",
    ] {
        assert!(text.iter().any(|t| t == expected), "{expected} in {text:?}");
    }
    let input_bottom = after
        .controls
        .iter()
        .filter(|c| matches!(c.target, Target::Input(_)))
        .map(|c| c.rect.y + c.rect.h)
        .fold(0., f32::max);
    let cards: Vec<_> = after
        .controls
        .iter()
        .filter(|c| matches!(c.target, Target::Cs2Match(_)))
        .collect();
    assert_eq!(cards.len(), 2);
    assert!(cards.iter().all(|c| c.rect.y > input_bottom));

    state.focus = Target::Input("com.webos.app.hdmi1".into());
    state.direction(Direction::Down, &controls(&state));
    assert_eq!(state.focus, Target::Cs2Match(0));
    let card = controls(&state)
        .into_iter()
        .find(|c| c.target == Target::Cs2Match(0))
        .unwrap();
    assert!(card.rect.y + card.rect.h <= 1080.);
    // OK opens the match: its series, maps and lineups, with Close focused.
    state.activate();
    assert!(matches!(
        &state.overlay,
        Some(crate::state::Overlay::Cs2Match { id, .. }) if id == "a"
    ));
    assert_eq!(state.focus, Target::Cs2Close);
    let dialog = scene(&state);
    let text: Vec<String> = dialog
        .draw
        .iter()
        .filter_map(|draw| match draw {
            crate::renderer::Draw::Text(text, ..) => Some(text.clone()),
            _ => None,
        })
        .collect();
    for expected in [
        "IEM Chengdu",
        "BO3",
        "Tier 1",
        "Mirage",
        "13:9",
        "Vitality lineup",
        "ZywOo",
        "FRANCE",
        "No lineup yet",
        "Upper bracket · Final",
        "COACH",
        "XTQZZZ",
        "Liquipedia has no rounds for this map yet",
    ] {
        assert!(text.iter().any(|t| t == expected), "{expected} in {text:?}");
    }
    assert!(dialog.controls.iter().any(|c| c.target == Target::Cs2Close));
    state.back();
    assert!(state.overlay.is_none());
    assert_eq!(state.focus, Target::Cs2Match(0));
}

#[test]
fn cs2_cards_scroll_three_at_a_time() {
    let mut state = ready();
    state.preferences.show_cs2 = true;
    state.preferences.cs2_major_only = false;
    let matches: Vec<_> = (0..5)
        .map(|i| {
            serde_json::json!({"id": format!("m{i}"), "begin_at": "1970-01-02T10:00:00Z",
                "state": "upcoming", "tournament": format!("Cup {i}"),
                "teams": [{"name": format!("Team {i}A")}, {"name": format!("Team {i}B")}]})
        })
        .collect();
    let feed =
        crate::cs2::parse_feed(&serde_json::json!({"updated": 0, "matches": matches})).unwrap();
    let (Some(command), _) = state.cs2.sync(true, true, 0) else {
        panic!();
    };
    state.cs2.accept(command.id, Ok(feed), 0);
    let cards = |state: &State| -> Vec<usize> {
        controls(state)
            .iter()
            .filter_map(|c| match c.target {
                Target::Cs2Match(i) => Some(i),
                _ => None,
            })
            .collect()
    };
    assert_eq!(cards(&state), [0, 1, 2]);
    state.focus = Target::Cs2Match(2);
    state.direction(Direction::Right, &controls(&state));
    assert_eq!(state.focus, Target::Cs2Match(3));
    assert_eq!(cards(&state), [1, 2, 3]);
    let text: Vec<String> = scene(&state)
        .draw
        .iter()
        .filter_map(|draw| match draw {
            crate::renderer::Draw::Text(text, ..) => Some(text.clone()),
            _ => None,
        })
        .collect();
    assert!(text.iter().any(|t| t == "2–4 / 5"), "{text:?}");
    state.direction(Direction::Right, &controls(&state));
    state.direction(Direction::Right, &controls(&state));
    assert_eq!(
        state.focus,
        Target::Cs2Match(4),
        "the last card keeps focus"
    );
    for _ in 0..4 {
        state.direction(Direction::Left, &controls(&state));
    }
    assert_eq!(state.focus, Target::Cs2Match(0));
    assert_eq!(cards(&state), [0, 1, 2]);
    // OK opens the card it is on, not the one in the same place before scrolling.
    state.focus = Target::Cs2Match(4);
    state.cs2_reveal(4);
    state.activate();
    assert!(matches!(
        &state.overlay,
        Some(crate::state::Overlay::Cs2Match { id, .. }) if id == "m4"
    ));
}

#[test]
fn down_from_favorites_reaches_cs2_score_without_inputs() {
    let mut state = ready();
    state.inventory.inputs.clear();
    state.preferences.show_cs2 = true;
    with_feed(&mut state);
    let favorite = state.apps()[2].id.clone();
    state.focus = Target::App(favorite);
    state.direction(Direction::Down, &controls(&state));
    assert!(
        matches!(state.focus, Target::Cs2Match(_)),
        "{:?}",
        state.focus
    );
    let card = controls(&state)
        .into_iter()
        .find(|c| c.target == state.focus)
        .unwrap();
    assert!(card.rect.y + card.rect.h <= 1080.);
}
