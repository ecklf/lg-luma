//! Home as a grid of blocks: twelve columns on the logical 1920×1080 layout, whole cells only,
//! no overlap. Blocks pushed aside while one is moved or resized are reflowed from the layout at
//! the start of the edit, so they return once the edited block moves on.
use crate::state::Rect;
use serde::{Deserialize, Deserializer, Serialize};

pub const COLUMNS: u8 = 12;
pub const CELL_W: f32 = 124.;
pub const CELL_H: f32 = 100.;
pub const GAP: f32 = 24.;
pub const LEFT: f32 = 84.;
pub const TOP: f32 = 150.;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockKind {
    Greeting,
    Weather,
    Favorites,
    Inputs,
    Crypto,
    Premieres,
}

impl BlockKind {
    pub const ALL: [Self; 6] = [
        Self::Greeting,
        Self::Weather,
        Self::Favorites,
        Self::Inputs,
        Self::Crypto,
        Self::Premieres,
    ];

    /// Minimum and maximum (width, height) in cells.
    pub fn limits(self) -> ((u8, u8), (u8, u8)) {
        match self {
            Self::Greeting => ((4, 1), (12, 3)),
            Self::Weather => ((4, 1), (12, 2)),
            Self::Favorites => ((6, 2), (12, 3)),
            Self::Inputs => ((4, 1), (12, 3)),
            Self::Crypto => ((3, 1), (6, 2)),
            Self::Premieres => ((6, 2), (12, 3)),
        }
    }

    pub fn default_size(self) -> (u8, u8) {
        match self {
            Self::Greeting => (8, 2),
            Self::Weather => (8, 1),
            Self::Favorites => (12, 2),
            Self::Inputs => (12, 2),
            Self::Crypto => (4, 1),
            Self::Premieres => (12, 2),
        }
    }

    /// The greeting and the favourites always stay on Home.
    pub fn removable(self) -> bool {
        !matches!(self, Self::Greeting | Self::Favorites)
    }

    /// Message key of the block's name.
    pub fn name_key(self) -> &'static str {
        match self {
            Self::Greeting => "greeting",
            Self::Weather => "weather",
            Self::Favorites => "favorites",
            Self::Inputs => "inputsHeading",
            Self::Crypto => "cryptoWidget",
            Self::Premieres => "premieresWidget",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Block {
    pub kind: BlockKind,
    pub x: u8,
    pub y: u8,
    pub w: u8,
    pub h: u8,
}

impl Block {
    pub fn new(kind: BlockKind, x: u8, y: u8) -> Self {
        let (w, h) = kind.default_size();
        Self { kind, x, y, w, h }
    }

    pub fn overlaps(&self, other: &Block) -> bool {
        self.x < other.x + other.w
            && other.x < self.x + self.w
            && self.y < other.y + other.h
            && other.y < self.y + self.h
    }
}

/// Today's Home: greeting, its weather line, the favourites strip and the inputs.
pub fn classic_layout() -> Vec<Block> {
    vec![
        Block::new(BlockKind::Greeting, 0, 0),
        Block::new(BlockKind::Weather, 0, 2),
        Block::new(BlockKind::Favorites, 0, 3),
        Block::new(BlockKind::Inputs, 0, 5),
    ]
}

/// A block's area on the logical layout, before Home's scroll.
pub fn rect(block: &Block) -> Rect {
    Rect::new(
        LEFT + f32::from(block.x) * (CELL_W + GAP),
        TOP + f32::from(block.y) * (CELL_H + GAP),
        f32::from(block.w) * CELL_W + f32::from(block.w.saturating_sub(1)) * GAP,
        f32::from(block.h) * CELL_H + f32::from(block.h.saturating_sub(1)) * GAP,
    )
}

/// The first row below every block.
pub fn bottom(blocks: &[Block]) -> u8 {
    blocks.iter().map(|b| b.y + b.h).max().unwrap_or(0)
}

/// Keeps `fixed` where it is and pushes every block it or a pushed block overlaps down.
pub fn settle(blocks: &mut [Block], fixed: BlockKind) {
    blocks.sort_by_key(|b| (b.kind != fixed, b.y, b.x));
    for index in 1..blocks.len() {
        let (placed, rest) = blocks.split_at_mut(index);
        let block = &mut rest[0];
        while placed.iter().any(|other| other.overlaps(block)) {
            block.y += 1;
        }
    }
}

/// The layout with `edited` in place of its kind, others from `snapshot`, collisions pushed down.
pub fn reflow(snapshot: &[Block], edited: Block) -> Vec<Block> {
    let mut blocks: Vec<Block> = snapshot
        .iter()
        .map(|b| if b.kind == edited.kind { edited } else { *b })
        .collect();
    settle(&mut blocks, edited.kind);
    blocks
}

/// The top-left cell of the first free area of this size, scanning rows from the top.
pub fn first_free(blocks: &[Block], w: u8, h: u8) -> (u8, u8) {
    let w = w.min(COLUMNS);
    for y in 0..=bottom(blocks) {
        for x in 0..=COLUMNS - w {
            let probe = Block {
                kind: BlockKind::Greeting,
                x,
                y,
                w,
                h,
            };
            if !blocks.iter().any(|b| b.overlaps(&probe)) {
                return (x, y);
            }
        }
    }
    (0, bottom(blocks))
}

/// The smallest height this block's content needs: names under the favourites. Inputs scroll
/// like every row, so any number of them fits one line of cards.
fn min_height(block: &Block, names: bool, _inputs: usize) -> u8 {
    let ((_, min), (_, max)) = block.kind.limits();
    let needed = match block.kind {
        BlockKind::Favorites if names => 3,
        _ => min,
    };
    needed.clamp(min, max)
}

/// Repairs a layout: one block per kind, sizes within limits and content needs, inside the
/// columns, the greeting and favourites present, and no overlap.
pub fn normalize(blocks: &mut Vec<Block>, names: bool, inputs: usize) {
    let mut seen = Vec::new();
    blocks.retain(|b| {
        let first = !seen.contains(&b.kind);
        seen.push(b.kind);
        first
    });
    for kind in [BlockKind::Greeting, BlockKind::Favorites] {
        if !blocks.iter().any(|b| b.kind == kind) {
            let (w, h) = kind.default_size();
            let (x, y) = first_free(blocks, w, h);
            blocks.push(Block { kind, x, y, w, h });
        }
    }
    for block in blocks.iter_mut() {
        let ((min_w, _), (max_w, max_h)) = block.kind.limits();
        block.w = block.w.clamp(min_w, max_w);
        block.x = block.x.min(COLUMNS - block.w);
        block.h = block.h.clamp(min_height(block, names, inputs), max_h);
    }
    blocks.sort_by_key(|b| (b.y, b.x));
    for index in 1..blocks.len() {
        let (placed, rest) = blocks.split_at_mut(index);
        let block = &mut rest[0];
        while placed.iter().any(|other| other.overlaps(block)) {
            block.y += 1;
        }
    }
}

/// Saved layouts skip blocks this version does not know; an unusable list is Classic.
pub fn deserialize_layout<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<Block>, D::Error> {
    let blocks: Vec<Block> = match serde_json::Value::deserialize(deserializer)? {
        serde_json::Value::Array(items) => items
            .into_iter()
            .filter_map(|item| serde_json::from_value(item).ok())
            .collect(),
        _ => vec![],
    };
    Ok(if blocks.is_empty() {
        classic_layout()
    } else {
        blocks
    })
}

#[cfg(test)]
#[path = "home-grid-tests.rs"]
mod tests;
