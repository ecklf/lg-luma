use super::*;
use crate::home_grid::{Block, BlockKind, rect};

fn row(x: u8, w: u8, h: u8) -> (Block, Rect) {
    let block = Block {
        kind: BlockKind::Premieres,
        x,
        y: 3,
        w,
        h,
    };
    (block, rect(&block))
}

#[test]
fn rows_keep_the_favourites_rhythm_with_a_header_above_their_cards() {
    assert!((GAP - 25.6).abs() < 1e-3 && (WIDE_W - 537.6).abs() < 1e-3);
    let (block, area) = row(0, 12, 2);
    let wide = track(&block, area, WIDE_W);
    assert!(wide.header);
    assert_eq!((wide.y, wide.height), (area.y + ROW_HEADER, 158.));
    assert_eq!(wide.columns, 3, "three wide cards, the fourth cut off");
    assert_eq!(track(&block, area, TILE_W).columns, 6);
    assert_eq!(
        (wide.clip_left, wide.clip_right),
        (8., 1910.),
        "a full-width row runs to the screen's edges"
    );
    let (block, area) = row(0, 12, 3);
    assert_eq!(
        track(&block, area, WIDE_W).height,
        282.,
        "taller cards, not more of them"
    );
    let (block, area) = row(6, 6, 1);
    let compact = track(&block, area, WIDE_W);
    assert!(!compact.header, "one cell tall: a compact row");
    assert_eq!((compact.y, compact.height), (area.y, 100.));
    assert_eq!(compact.columns, 1);
    assert_eq!(
        compact.clip_left, area.x,
        "beside another block it keeps to its own"
    );
}

#[test]
fn a_row_scrolls_just_enough_to_show_a_card_whole() {
    let (block, area) = row(0, 12, 2);
    let wide = track(&block, area, WIDE_W);
    assert_eq!(wide.first(9, 8), 5, "never past the last cards");
    assert_eq!(wide.reveal(0, 2), 0);
    assert_eq!(wide.reveal(0, 3), 1);
    assert_eq!(wide.reveal(5, 6), 5, "a card in view does not scroll");
    assert_eq!(wide.reveal(5, 4), 4);
    assert_eq!(wide.card(4, 3).x, area.x + wide.pitch);
}
