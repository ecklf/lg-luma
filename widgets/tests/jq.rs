use luma_widgets::jq::{self, Vars};
use serde_json::json;

fn vars() -> Vars {
    Vars {
        settings: json!({ "games": ["csgo"], "major_only": true, "key": true }),
        t: json!({ "map": "Map {number}", "pick": "Pick: {team}" }),
        lang: "en".into(),
        now: 1793021218,
        source: "live".into(),
        item: Some(json!("csgo")),
    }
}

#[test]
fn transforms_see_their_variables() {
    let out = jq::run(
        "{games: $settings.games, lang: $lang, now: $now, source: $source, item: $item, first: .a}",
        json!({ "a": 1 }),
        &vars(),
    )
    .unwrap();
    assert_eq!(
        out,
        json!({ "games": ["csgo"], "lang": "en", "now": 1793021218, "source": "live", "item": "csgo", "first": 1 })
    );
}

#[test]
fn fmt_fills_placeholders() {
    let out = jq::run(
        "[($t.map | fmt({number: 3})), ($t.pick | fmt({team: \"NAVI\"}))]",
        json!(null),
        &vars(),
    )
    .unwrap();
    assert_eq!(out, json!(["Map 3", "Pick: NAVI"]));
}

#[test]
fn the_standard_library_has_what_widgets_need() {
    let out = jq::run(
        r#"["2026-10-26T10:56:04Z" | fromdateiso8601, ("Dust II" | test("dust"; "i")), ("a b" | @uri), ([3,1,2] | sort | first), (1.5 | floor)]"#,
        json!(null),
        &vars(),
    )
    .unwrap();
    assert_eq!(out, json!([1793012164, true, "a%20b", 1, 1]));
}

#[test]
fn filters_that_reach_outside_are_not_there() {
    for program in [
        "now",
        "input",
        "inputs",
        "env",
        "$ENV",
        "debug",
        "stderr",
        "halt",
        "halt_error",
        "input_filename",
        "include \"x\"; .",
        "import \"x\" as x; .",
    ] {
        assert!(jq::check(program).is_err(), "{program} must be refused");
    }
    assert!(jq::check(".items | map(select(.live))").is_ok());
    assert!(jq::check("def f: .; f |").is_err());
}

#[test]
fn a_transform_gives_exactly_one_output() {
    assert!(jq::run(".[]", json!([1, 2]), &vars()).is_err());
    assert!(jq::run("empty", json!(null), &vars()).is_err());
    assert!(jq::run("error(\"no\")", json!(null), &vars()).is_err());
}
