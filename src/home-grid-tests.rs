use super::*;

fn block(kind: BlockKind, x: u8, y: u8, w: u8, h: u8) -> Block {
    Block { kind, x, y, w, h }
}

fn overlapping(blocks: &[Block]) -> bool {
    blocks
        .iter()
        .enumerate()
        .any(|(i, a)| blocks[i + 1..].iter().any(|b| a.overlaps(b)))
}

#[test]
fn the_classic_layout_fills_the_grid_without_overlap() {
    let layout = classic_layout();
    assert!(!overlapping(&layout));
    assert!(layout.iter().all(|b| b.x + b.w <= COLUMNS));
    let kinds: Vec<_> = layout.iter().map(|b| b.kind).collect();
    assert_eq!(
        kinds,
        [
            BlockKind::Greeting,
            BlockKind::Weather,
            BlockKind::Favorites,
            BlockKind::Inputs
        ]
    );
}

#[test]
fn cells_map_to_the_logical_layout() {
    let r = rect(&block(BlockKind::Favorites, 0, 0, 12, 2));
    assert_eq!((r.x, r.y, r.w, r.h), (84., TOP, 1752., 224.));
    let r = rect(&block(BlockKind::Crypto, 8, 1, 4, 1));
    assert_eq!(
        (r.x, r.y, r.w, r.h),
        (84. + 8. * 148., TOP + 124., 568., 100.)
    );
}

#[test]
fn a_pushed_block_returns_once_the_edited_block_moves_on() {
    let start = vec![
        block(BlockKind::Weather, 8, 0, 4, 1),
        block(BlockKind::Crypto, 8, 1, 4, 1),
        block(BlockKind::Favorites, 0, 2, 12, 2),
    ];
    let mut weather = start[0];
    weather.y = 1;
    let pushed = reflow(&start, weather);
    let find = |blocks: &[Block], kind| *blocks.iter().find(|b| b.kind == kind).unwrap();
    assert_eq!(find(&pushed, BlockKind::Crypto).y, 2);
    assert_eq!(find(&pushed, BlockKind::Favorites).y, 3);
    assert!(!overlapping(&pushed));
    weather.y = 4;
    let moved_on = reflow(&start, weather);
    assert_eq!(find(&moved_on, BlockKind::Crypto).y, 1, "back in place");
    assert_eq!(find(&moved_on, BlockKind::Favorites).y, 2);
    assert!(!overlapping(&moved_on));
}

#[test]
fn the_first_free_place_fills_gaps_before_the_bottom() {
    let blocks = vec![
        block(BlockKind::Greeting, 0, 0, 8, 2),
        block(BlockKind::Favorites, 0, 2, 12, 2),
    ];
    assert_eq!(first_free(&blocks, 4, 1), (8, 0));
    assert_eq!(first_free(&blocks, 6, 2), (0, 4));
    assert_eq!(bottom(&blocks), 4);
}

#[test]
fn normalising_repairs_sizes_overlap_and_missing_blocks() {
    let mut blocks = vec![
        block(BlockKind::Weather, 10, 0, 9, 7),
        block(BlockKind::Weather, 0, 9, 4, 1),
        block(BlockKind::Favorites, 0, 0, 2, 1),
        block(BlockKind::Crypto, 3, 0, 1, 1),
    ];
    normalize(&mut blocks, false, 0);
    assert!(!overlapping(&blocks));
    assert!(blocks.iter().all(|b| b.x + b.w <= COLUMNS));
    assert_eq!(
        blocks
            .iter()
            .filter(|b| b.kind == BlockKind::Weather)
            .count(),
        1
    );
    let weather = blocks
        .iter()
        .find(|b| b.kind == BlockKind::Weather)
        .unwrap();
    assert_eq!(
        (weather.w, weather.h),
        (9, 2),
        "clamped to the weather maximum"
    );
    let favorites = blocks
        .iter()
        .find(|b| b.kind == BlockKind::Favorites)
        .unwrap();
    assert_eq!((favorites.w, favorites.h), (6, 2), "grown to the minimum");
    let crypto = blocks.iter().find(|b| b.kind == BlockKind::Crypto).unwrap();
    assert_eq!(crypto.w, 3);
    assert!(
        blocks.iter().any(|b| b.kind == BlockKind::Greeting),
        "a missing greeting comes back"
    );
}

#[test]
fn content_rules_grow_blocks_that_need_more_rows() {
    let mut blocks = classic_layout();
    normalize(&mut blocks, true, 0);
    let favorites = blocks
        .iter()
        .find(|b| b.kind == BlockKind::Favorites)
        .unwrap();
    assert_eq!(favorites.h, 3, "names under the tiles need a third row");
    let mut blocks = vec![
        block(BlockKind::Greeting, 0, 0, 8, 2),
        block(BlockKind::Favorites, 0, 2, 12, 2),
        block(BlockKind::Inputs, 0, 4, 4, 1),
    ];
    normalize(&mut blocks, false, 3);
    let inputs = blocks.iter().find(|b| b.kind == BlockKind::Inputs).unwrap();
    assert_eq!(
        inputs.h, 1,
        "inputs scroll in one line, however many there are"
    );
}

#[test]
fn saved_layouts_ignore_unknown_blocks_and_fall_back_when_broken() {
    use crate::model::Preferences;
    let preferences: Preferences = serde_json::from_value(serde_json::json!({
        "home_layout": [
            {"kind": "greeting", "x": 0, "y": 0, "w": 8, "h": 2},
            {"kind": "future_widget", "x": 0, "y": 2, "w": 4, "h": 1},
            {"kind": "favorites", "x": 0, "y": 2, "w": 12, "h": 2}
        ],
        "show_home_names": true
    }))
    .unwrap();
    assert_eq!(preferences.home_layout.len(), 2);
    assert!(preferences.show_home_names);
    for broken in [
        serde_json::json!("grid"),
        serde_json::json!([]),
        serde_json::json!(4),
    ] {
        let preferences: Preferences =
            serde_json::from_value(serde_json::json!({ "home_layout": broken })).unwrap();
        assert_eq!(preferences.home_layout, classic_layout());
    }
    assert_eq!(Preferences::default().home_layout, classic_layout());
}

#[test]
fn every_theme_is_a_valid_layout_with_a_known_wallpaper() {
    for theme in crate::themes::THEMES.iter() {
        let layout = (theme.layout)();
        let mut normalized = layout.clone();
        normalize(&mut normalized, false, 0);
        assert_eq!(normalized, layout, "{} normalises unchanged", theme.id);
        assert!(
            crate::model::BACKGROUNDS
                .iter()
                .any(|(id, _)| *id == theme.wallpaper),
            "{}",
            theme.id
        );
    }
    assert_eq!(crate::themes::theme("unknown").id, "classic");
}

#[test]
fn applying_a_theme_sets_its_layout_wallpaper_and_id() {
    let mut preferences = crate::model::Preferences::default();
    crate::themes::apply(&mut preferences, "cinema");
    assert_eq!(preferences.home_theme, "cinema");
    assert_eq!(preferences.background, "midnight");
    assert_eq!(
        preferences.home_layout,
        (crate::themes::theme("cinema").layout)()
    );
    assert_eq!(crate::themes::THEMES[0].id, "classic");
    assert_eq!((crate::themes::THEMES[0].layout)(), classic_layout());
}
