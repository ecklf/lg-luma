//! Home's design tokens and row geometry, shared by the view and by focus navigation. The rules
//! behind them are in docs/design.md; widgets use these instead of sizes, colours or type of
//! their own.
use crate::home_grid::Block;
use crate::renderer::{Color, TextStyle};
use crate::state::{HOME_FADE_LEFT, HOME_FADE_RIGHT, HOME_TILE_PITCH, HOME_TILE_WIDTH, Rect};

/// An app tile's width; the favourites set Home's rhythm.
pub const TILE_W: f32 = HOME_TILE_WIDTH;
/// Between cards in a row.
pub const GAP: f32 = HOME_TILE_PITCH - HOME_TILE_WIDTH;
/// Two tiles and the gap between them: inputs and premieres.
pub const WIDE_W: f32 = 2. * TILE_W + GAP;
/// Input cards keep the height their two lines had before rows; a taller block adds room below.
pub const INPUT_H: f32 = 104.;
/// Every card's corners.
pub const RADIUS: f32 = 19.2;
/// The band above a row's cards that holds its title.
pub const ROW_HEADER: f32 = 66.;
/// Inside a card.
pub const PAD: f32 = 20.;

pub const TEXT: u32 = 0xf5f5f5;
/// Live states only.
pub const LIVE: u32 = 0xe5484d;
pub const LIVE_TEXT: u32 = 0xff6b6f;
pub const UP: u32 = 0x4ade80;
pub const DOWN: u32 = 0xf87171;
/// Prize money and other amounts worth singling out.
pub const GOLD: u32 = 0xf5c451;

pub fn color(rgb: u32, alpha: f32) -> Color {
    [
        ((rgb >> 16) & 0xff) as f32 / 255.,
        ((rgb >> 8) & 0xff) as f32 / 255.,
        (rgb & 0xff) as f32 / 255.,
        alpha,
    ]
}

/// Text in `TEXT` at this opacity: 1, 0.85, 0.6 or 0.45.
pub fn ink(alpha: f32) -> Color {
    color(TEXT, alpha)
}

fn style(size: f32, weight: u16, tracking: f32) -> TextStyle {
    TextStyle::new(size).weight(weight).tracking(tracking)
}

/// A row's title.
pub fn section() -> TextStyle {
    style(23.2, 550, -0.035).leading(1.3)
}

/// The main line of a card: a show, an input.
pub fn card_title(tall: bool) -> TextStyle {
    style(if tall { 28. } else { 22. }, 650, -0.015)
}

/// Secondary lines and messages.
pub fn body() -> TextStyle {
    style(18.4, 500, -0.01)
}

/// Events, freshness, counts.
pub fn meta() -> TextStyle {
    style(15.2, 500, 0.)
}

/// Small labels, written in upper case.
pub fn caps() -> TextStyle {
    style(14.4, 650, 0.1)
}

/// Scores and prices.
pub fn figure(size: f32) -> TextStyle {
    style(size, 650, -0.01)
}

/// Where a row's cards go: their line on screen, size and pitch, how many whole ones show and
/// where drawing is clipped (the screen's edge when the block reaches it, as for the favourites).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Track {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub pitch: f32,
    /// Whole cards in view.
    pub columns: usize,
    pub clip_left: f32,
    pub clip_right: f32,
    /// Whether the row has a header; a block one cell tall does not.
    pub header: bool,
}

/// The track of a row block drawn at `rect` with cards `width` wide.
pub fn track(block: &Block, rect: Rect, width: f32) -> Track {
    let header = rect.h > 150.;
    let top = if header { ROW_HEADER } else { 0. };
    Track {
        x: rect.x,
        y: rect.y + top,
        width,
        height: rect.h - top,
        pitch: width + GAP,
        columns: (((rect.w + GAP) / (width + GAP)).floor() as usize).max(1),
        clip_left: if block.x == 0 { HOME_FADE_LEFT } else { rect.x },
        clip_right: if block.x + block.w == crate::home_grid::COLUMNS {
            HOME_FADE_RIGHT
        } else {
            rect.x + rect.w
        },
        header,
    }
}

impl Track {
    /// The first card in view for a row scrolled to `scroll` with `count` cards.
    pub fn first(&self, scroll: usize, count: usize) -> usize {
        scroll.min(count.saturating_sub(self.columns))
    }

    /// The scroll that keeps card `index` whole in view, starting from `scroll`.
    pub fn reveal(&self, scroll: usize, index: usize) -> usize {
        scroll
            .min(index)
            .max((index + 1).saturating_sub(self.columns))
    }

    /// Card `index` when the row starts at `first`.
    pub fn card(&self, index: usize, first: usize) -> Rect {
        Rect::new(
            self.x + (index as f32 - first as f32) * self.pitch,
            self.y,
            self.width,
            self.height,
        )
    }
}

#[cfg(test)]
#[path = "design-tests.rs"]
mod tests;
