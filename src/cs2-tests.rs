use super::*;
use serde_json::json;

fn feed() -> Value {
    json!({
        "updated": 1_000_000,
        "matches": [
            {"id": "live", "begin_at": "2026-10-30T18:00:00Z", "state": "live", "best_of": 3,
             "tournament": "IEM Chengdu", "stage": "Playoffs", "tier": 1,
             "teams": [{"name": "Team Vitality", "short": "Vitality",
                        "image": "https://liquipedia.net/commons/images/thumb/a/ab/Vitality.png/400px-Vitality.png"},
                       {"name": "FaZe Clan", "short": "FaZe", "image": "https://example.com/faze.png"}],
             "lineups": [[{"name": "ZywOo", "flag": "France"}, {"name": ""}], [{"name": "ropz"}]],
             "score": [1, 0],
             "maps": [{"map": "Mirage", "score": [13, 9], "winner": 0,
                       "image": "https://liquipedia.net/commons/images/Mirage.jpg"},
                      {"map": "Inferno", "score": [4, 6], "winner": null}]},
            {"id": "small", "begin_at": "2026-10-31T08:00:00Z", "state": "upcoming", "tier": 4,
             "teams": [{"name": "OG"}, null], "maps": []},
            {"id": "bad", "begin_at": "soon", "state": "upcoming", "teams": [null, null]},
            {"id": "solo", "begin_at": "2026-10-31T08:00:00Z", "state": "upcoming", "teams": [null]},
            {"id": "done", "begin_at": "2026-10-30T15:00:00Z", "state": "finished", "tier": 2,
             "teams": [{"name": "NAVI", "short": "NAVI"}, {"name": "G2 Esports", "short": "G2"}],
             "score": [0, 2], "maps": []},
            {"id": "next", "begin_at": "2026-10-31T11:00:00Z", "state": "upcoming", "tier": 1,
             "best_of": 5, "teams": [{"name": "MOUZ"}, {"name": "Spirit"}], "maps": []}
        ]
    })
}

#[test]
fn the_feed_keeps_valid_matches() {
    let feed = parse_feed(&feed()).unwrap();
    let ids: Vec<_> = feed.matches.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(ids, ["live", "small", "done", "next"]);
    let live = &feed.matches[0];
    assert_eq!(live.state, State::Live);
    assert_eq!(live.score, Some([1, 0]));
    assert_eq!(live.teams[1].as_ref().unwrap().short, "FaZe");
    assert_eq!(
        live.current_map().map(|(n, m)| (n, m.name.as_str())),
        Some((2, "Inferno"))
    );
    // A side without a short name uses its name; an unknown side is None.
    assert_eq!(feed.matches[1].teams[0].as_ref().unwrap().short, "OG");
    assert!(feed.matches[1].teams[1].is_none());
    assert!(parse_feed(&json!({"matches": []})).is_err());
}

#[test]
fn cards_put_live_first_and_follow_the_settings() {
    let feed = parse_feed(&feed()).unwrap();
    let ids = |major, results| -> Vec<_> {
        shown(&feed, major, results)
            .iter()
            .map(|m| m.id.clone())
            .collect()
    };
    assert_eq!(ids(true, true), ["live", "next", "done"]);
    assert_eq!(ids(true, false), ["live", "next"]);
    assert_eq!(ids(false, true), ["live", "small", "next", "done"]);
}

#[test]
fn scores_are_fetched_only_while_home_shows_them() {
    let mut scores = Scores::default();
    assert!(scores.sync(false, true, 1000).0.is_none());
    assert!(scores.sync(true, false, 1000).0.is_none());
    let (Some(command), _) = scores.sync(true, true, 1000) else {
        panic!("Home with the widget should fetch");
    };
    assert!(scores.loading);
    assert!(
        scores.sync(true, true, 1001).0.is_none(),
        "one request at a time"
    );
    let mut feed = parse_feed(&feed()).unwrap();
    feed.updated = 1000;
    assert!(scores.accept(command.id, Ok(feed), 1002));
    assert!(scores.current(1002).is_some());
    assert!(
        scores
            .sync(true, true, 1002 + REFRESH_SECONDS - 1)
            .0
            .is_none()
    );
    let (Some(next), _) = scores.sync(true, true, 1002 + REFRESH_SECONDS) else {
        panic!("due again");
    };
    scores.accept(next.id, Err("offline".into()), 1200);
    assert!(scores.failed);
    assert!(
        scores.sync(true, true, 1201).0.is_none(),
        "a failure waits for the retry"
    );
    // A feed older than half an hour is still shown, marked as behind.
    assert!(scores.current(1000 + MAX_AGE + 1).is_some());
    assert!(!scores.stale(1000 + MAX_AGE));
    assert!(scores.stale(1000 + MAX_AGE + 1));
    // Replies to cancelled requests are ignored.
    let (Some(old), _) = scores.sync(true, true, 1200 + RETRY_SECONDS) else {
        panic!();
    };
    scores.sync(false, true, 1200 + RETRY_SECONDS);
    assert!(!scores.accept(old.id, Err("late".into()), 1400));
}

#[test]
fn malformed_switches_keep_their_defaults() {
    let preferences: crate::model::Preferences = serde_json::from_value(json!({
        "favorites": ["netflix"], "show_cs2": "yes", "cs2_major_only": 0, "cs2_results": false
    }))
    .unwrap();
    assert_eq!(preferences.favorites, Some(vec!["netflix".into()]));
    assert!(!preferences.show_cs2);
    assert!(preferences.cs2_major_only);
    assert!(!preferences.cs2_results);
}

#[test]
fn logos_come_only_from_liquipedia_and_load_once() {
    let mut scores = Scores::default();
    let (Some(command), _) = scores.sync(true, true, 0) else {
        panic!();
    };
    let mut feed = parse_feed(&feed()).unwrap();
    feed.updated = 0;
    let live = &feed.matches[0];
    assert!(live.teams[0].as_ref().unwrap().logo.is_some());
    assert!(
        live.teams[1].as_ref().unwrap().logo.is_none(),
        "another origin is refused"
    );
    assert_eq!(
        live.lineups[0].len(),
        1,
        "a player without a name is left out"
    );
    assert_eq!(live.lineups[0][0].flag, "France");
    assert_eq!(live.lineups[1][0].flag, "");
    scores.accept(command.id, Ok(feed), 0);
    let wanted = scores.wanted_images(&["live".into()], None);
    assert_eq!(wanted, [(live_logo(), false)]);
    assert!(
        scores.wanted_images(&["live".into()], None).is_empty(),
        "asked for once"
    );
    // The open match's maps come as backdrops.
    let maps = scores.wanted_images(&["live".into()], Some("live"));
    assert!(
        maps.iter()
            .all(|(url, backdrop)| *backdrop && url.ends_with("Mirage.jpg"))
    );
    assert_eq!(maps.len(), 1);
    assert!(scores.accept_logo(wanted[0].0.clone(), None));
    assert!(
        !scores.accept_logo(wanted[0].0.clone(), None),
        "only an awaited logo is taken"
    );
    let team = scores.find("live").unwrap().teams[0].clone().unwrap();
    assert!(scores.logo(&team).is_none());
}

fn live_logo() -> String {
    "https://liquipedia.net/commons/images/thumb/a/ab/Vitality.png/400px-Vitality.png".into()
}

#[test]
fn rounds_coaches_and_the_featured_map_are_read() {
    let feed = parse_feed(&serde_json::json!({
        "updated": 0,
        "staff": {"Team_Vitality": [{"name": "XTQZZZ", "role": "Head Coach"},
                                    {"name": "MaT", "role": "Assistant Coach"}, {"name": ""}]},
        "matches": [
            {"id": "a", "begin_at": "2026-10-30T18:00:00Z", "state": "live",
             "round": {"key": "semifinals", "bracket": "upper"},
             "teams": [{"name": "Team Vitality", "page": "Team_Vitality"}, {"name": "OG"}],
             "maps": [{"map": "Mirage", "winner": 0}, {"map": "Nuke",
                       "image": "https://liquipedia.net/commons/images/nuke.jpg"}]},
            {"id": "b", "begin_at": "2026-10-30T18:00:00Z", "state": "finished",
             "round": {"key": "round_of", "number": 16}, "teams": [null, null],
             "maps": [{"map": "Inferno", "winner": 1}, {"map": "Dust II", "winner": 0}]},
            {"id": "c", "begin_at": "2026-10-30T18:00:00Z", "state": "upcoming",
             "round": {"text": "Decider Match"}, "teams": [null, null]}
        ]
    }))
    .unwrap();
    let [a, b, c] = &feed.matches[..] else {
        panic!();
    };
    assert_eq!(a.round, Some((Round::Semifinals, Some(Bracket::Upper))));
    assert_eq!(b.round, Some((Round::Top(16), None)));
    assert_eq!(c.round, Some((Round::Named("Decider Match".into()), None)));
    assert_eq!(a.featured_map().map(|m| m.name.as_str()), Some("Nuke"));
    assert_eq!(b.featured_map().map(|m| m.name.as_str()), Some("Dust II"));
    assert!(c.featured_map().is_none());
    let coaches = feed.coaches(a.teams[0].as_ref().unwrap());
    assert_eq!(coaches.len(), 2);
    assert!(!coaches[0].assistant && coaches[1].assistant);
    assert!(feed.coaches(a.teams[1].as_ref().unwrap()).is_empty());
}
