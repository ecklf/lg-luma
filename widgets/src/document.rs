//! The widget document (docs/widgets.md §4–5): what a transform returns, in the templates'
//! terms. Parsing is lenient but bounded: unknown fields are ignored, wrong types are dropped,
//! texts and lists are capped and images off the widget's origins are removed, so whatever a
//! transform gives cannot make a template misbehave.
use crate::Error;
use crate::manifest::{CardKind, Origin};
use serde_json::Value;

pub const ITEMS: usize = 50;
pub const TEXT_LIMIT: usize = 200;
const ID_LIMIT: usize = 64;
const URL_LIMIT: usize = 512;

/// A text field: plain, or a value Luma formats in the TV's language and time zone.
#[derive(Clone, Debug, PartialEq)]
pub enum Text {
    Plain(String),
    /// A start time with its day ("18:00", "Tomorrow 18:00").
    Start(String),
    /// Whole minutes since then, up to the data's time ("12 min").
    Elapsed(String),
    /// "Data as of 17:46".
    AsOf(i64),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tone {
    #[default]
    Neutral,
    Live,
    Gold,
    Up,
    Down,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StepState {
    Done,
    Live,
    #[default]
    Next,
}

/// A text with a tone: a chip, a ticker's change.
#[derive(Clone, Debug, PartialEq)]
pub struct Mark {
    pub text: Text,
    pub tone: Tone,
}

/// One side of a contest, on a card or in the dialog's hero.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Side {
    pub name: String,
    pub short: String,
    pub image: Option<String>,
    pub score: Option<String>,
    pub lead: bool,
    pub caption: Option<Text>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Progress {
    pub label: Option<Text>,
    pub value: Option<Text>,
    pub steps: Vec<StepState>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Scoreboard {
    pub live: bool,
    pub status: Option<Text>,
    pub logo: Option<String>,
    pub caption: Option<Text>,
    pub sides: [Option<Side>; 2],
    pub footer: Option<Text>,
    pub progress: Option<Progress>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Media {
    pub image: Option<String>,
    pub title: Text,
    pub subtitle: Option<Text>,
    pub live: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Ticker {
    pub label: Text,
    pub value: Text,
    pub change: Option<Mark>,
    pub detail: Option<Text>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Card {
    Scoreboard(Box<Scoreboard>),
    Media(Media),
    Ticker(Ticker),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Header {
    pub logo: Option<String>,
    pub title: Option<Text>,
    pub subtitle: Option<Text>,
    pub chips: Vec<Mark>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Hero {
    pub live: bool,
    pub sides: [Option<Side>; 2],
    pub lines: Vec<Text>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TileResult {
    pub image: Option<String>,
    pub name: Option<String>,
    pub text: Option<Text>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Tile {
    pub number: Option<String>,
    pub title: Option<Text>,
    pub placeholder: Option<Text>,
    pub note: Option<Text>,
    pub state: StepState,
    pub value: Option<Text>,
    pub result: Option<TileResult>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Tiles {
    pub title: Option<Text>,
    pub slots: usize,
    pub items: Vec<Tile>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub image: Option<String>,
    pub title: Text,
    pub subtitle: Option<Text>,
    pub meta: Option<Text>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Extra {
    pub label: Text,
    pub text: Text,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct List {
    pub title: Option<Text>,
    pub image: Option<String>,
    pub name: Option<String>,
    pub rows: Vec<Row>,
    pub extra: Vec<Extra>,
    pub empty: Option<Text>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Footer {
    pub label: Option<Text>,
    pub text: Option<Text>,
    pub credit: Option<Text>,
    pub note: Option<Text>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Dialog {
    pub header: Option<Header>,
    pub hero: Option<Hero>,
    pub tiles: Option<Tiles>,
    pub lists: Vec<List>,
    pub footer: Option<Footer>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub id: String,
    pub card: Card,
    pub dialog: Option<Dialog>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Document {
    pub title: Option<Text>,
    pub updated: Option<i64>,
    pub refresh: Option<u64>,
    pub empty: Option<Text>,
    pub items: Vec<Item>,
}

/// Plain text: control characters removed, trimmed, at most `limit` characters.
fn clean(text: &str, limit: usize) -> String {
    text.chars()
        .filter(|c| !c.is_control())
        .collect::<String>()
        .trim()
        .chars()
        .take(limit)
        .collect()
}

/// A plain string, or a number written as text.
fn string(value: Option<&Value>, limit: usize) -> Option<String> {
    let text = match value? {
        Value::String(text) => clean(text, limit),
        Value::Number(number) => number.to_string(),
        _ => return None,
    };
    (!text.is_empty()).then_some(text)
}

fn text(value: Option<&Value>) -> Option<Text> {
    let value = value?;
    if let Some(object) = value.as_object() {
        let stamp = |key| string(object.get(key), 40);
        return if let Some(start) = stamp("start") {
            Some(Text::Start(start))
        } else if let Some(since) = stamp("elapsed") {
            Some(Text::Elapsed(since))
        } else {
            object.get("as_of").and_then(Value::as_i64).map(Text::AsOf)
        };
    }
    string(Some(value), TEXT_LIMIT).map(Text::Plain)
}

fn flag(value: &Value, key: &str) -> bool {
    value.get(key).and_then(Value::as_bool).unwrap_or(false)
}

fn list(value: Option<&Value>) -> impl Iterator<Item = &Value> {
    value.and_then(Value::as_array).into_iter().flatten()
}

fn tone(value: Option<&Value>) -> Tone {
    match value.and_then(Value::as_str) {
        Some("live") => Tone::Live,
        Some("gold") => Tone::Gold,
        Some("up") => Tone::Up,
        Some("down") => Tone::Down,
        _ => Tone::Neutral,
    }
}

fn step_state(value: Option<&Value>) -> StepState {
    match value.and_then(Value::as_str) {
        Some("done") => StepState::Done,
        Some("live") => StepState::Live,
        _ => StepState::Next,
    }
}

struct Parser<'a> {
    origins: &'a [Origin],
}

impl Parser<'_> {
    fn image(&self, value: Option<&Value>) -> Option<String> {
        let url = value?.as_str()?;
        (url.len() <= URL_LIMIT
            && !url.contains("..")
            && !url.chars().any(|c| c.is_control() || c.is_whitespace())
            && self.origins.iter().any(|origin| origin.allows(url)))
        .then(|| url.to_owned())
    }

    fn mark(&self, value: &Value) -> Option<Mark> {
        Some(Mark {
            text: text(value.get("text"))?,
            tone: tone(value.get("tone")),
        })
    }

    fn side(&self, value: &Value) -> Option<Side> {
        value.as_object()?;
        let name = string(value.get("name"), TEXT_LIMIT).unwrap_or_default();
        let short = string(value.get("short"), 24).unwrap_or_else(|| name.clone());
        Some(Side {
            name,
            short,
            image: self.image(value.get("image")),
            score: string(value.get("score"), 12),
            lead: flag(value, "lead"),
            caption: text(value.get("caption")),
        })
    }

    fn sides(&self, value: Option<&Value>) -> [Option<Side>; 2] {
        let mut sides = list(value).take(2).map(|side| self.side(side));
        [sides.next().flatten(), sides.next().flatten()]
    }

    fn scoreboard(&self, card: &Value) -> Scoreboard {
        Scoreboard {
            live: flag(card, "live"),
            status: text(card.get("status")),
            logo: self.image(card.get("logo")),
            caption: text(card.get("caption")),
            sides: self.sides(card.get("sides")),
            footer: text(card.get("footer")),
            progress: card
                .get("progress")
                .filter(|p| p.is_object())
                .map(|p| Progress {
                    label: text(p.get("label")),
                    value: text(p.get("value")),
                    steps: list(p.get("steps"))
                        .take(7)
                        .map(|s| step_state(Some(s)))
                        .collect(),
                }),
        }
    }

    fn card(&self, card: &Value, kind: CardKind) -> Option<Card> {
        card.as_object()?;
        Some(match kind {
            CardKind::Scoreboard => Card::Scoreboard(Box::new(self.scoreboard(card))),
            CardKind::Media => Card::Media(Media {
                image: self.image(card.get("image")),
                title: text(card.get("title"))?,
                subtitle: text(card.get("subtitle")),
                live: flag(card, "live"),
            }),
            CardKind::Ticker => Card::Ticker(Ticker {
                label: text(card.get("label"))?,
                value: text(card.get("value"))?,
                change: card.get("change").and_then(|c| self.mark(c)),
                detail: text(card.get("detail")),
            }),
        })
    }

    fn dialog(&self, dialog: &Value) -> Option<Dialog> {
        dialog.as_object()?;
        let header = dialog
            .get("header")
            .filter(|h| h.is_object())
            .map(|h| Header {
                logo: self.image(h.get("logo")),
                title: text(h.get("title")),
                subtitle: text(h.get("subtitle")),
                chips: list(h.get("chips"))
                    .filter_map(|c| self.mark(c))
                    .take(8)
                    .collect(),
            });
        let hero = dialog.get("hero").filter(|h| h.is_object()).map(|h| Hero {
            live: flag(h, "live"),
            sides: self.sides(h.get("sides")),
            lines: list(h.get("lines"))
                .filter_map(|l| text(Some(l)))
                .take(2)
                .collect(),
        });
        let tiles = dialog.get("tiles").filter(|t| t.is_object()).map(|t| {
            let items: Vec<Tile> = list(t.get("items"))
                .filter(|tile| tile.is_object())
                .take(7)
                .map(|tile| Tile {
                    number: string(tile.get("number"), 4),
                    title: text(tile.get("title")),
                    placeholder: text(tile.get("placeholder")),
                    note: text(tile.get("note")),
                    state: step_state(tile.get("state")),
                    value: text(tile.get("value")),
                    result: tile
                        .get("result")
                        .filter(|r| r.is_object())
                        .map(|r| TileResult {
                            image: self.image(r.get("image")),
                            name: string(r.get("name"), 24),
                            text: text(r.get("text")),
                        }),
                })
                .collect();
            let slots = t
                .get("slots")
                .and_then(Value::as_u64)
                .map_or(items.len(), |slots| slots as usize)
                .clamp(1, 7)
                .max(items.len());
            Tiles {
                title: text(t.get("title")),
                slots,
                items,
            }
        });
        let lists = list(dialog.get("lists"))
            .filter(|l| l.is_object())
            .take(2)
            .map(|l| List {
                title: text(l.get("title")),
                image: self.image(l.get("image")),
                name: string(l.get("name"), 24),
                rows: list(l.get("rows"))
                    .filter_map(|row| {
                        Some(Row {
                            image: self.image(row.get("image")),
                            title: text(row.get("title"))?,
                            subtitle: text(row.get("subtitle")),
                            meta: text(row.get("meta")),
                        })
                    })
                    .take(12)
                    .collect(),
                extra: list(l.get("extra"))
                    .filter_map(|extra| {
                        Some(Extra {
                            label: text(extra.get("label"))?,
                            text: text(extra.get("text"))?,
                        })
                    })
                    .take(4)
                    .collect(),
                empty: text(l.get("empty")),
            })
            .collect();
        let footer = dialog
            .get("footer")
            .filter(|f| f.is_object())
            .map(|f| Footer {
                label: text(f.get("label")),
                text: text(f.get("text")),
                credit: text(f.get("credit")),
                note: text(f.get("note")),
            });
        Some(Dialog {
            header,
            hero,
            tiles,
            lists,
            footer,
        })
    }
}

impl Document {
    pub fn parse(value: &Value, card: CardKind, origins: &[Origin]) -> Result<Self, Error> {
        if !value.is_object() {
            return Err(Error("the transform must give an object".into()));
        }
        let parser = Parser { origins };
        let mut items: Vec<Item> = Vec::new();
        for item in list(value.get("items")) {
            if items.len() == ITEMS {
                break;
            }
            let (Some(id), Some(card)) = (
                string(item.get("id"), ID_LIMIT),
                item.get("card").and_then(|c| parser.card(c, card)),
            ) else {
                continue;
            };
            if items.iter().any(|other| other.id == id) {
                continue;
            }
            items.push(Item {
                id,
                card,
                dialog: item.get("dialog").and_then(|d| parser.dialog(d)),
            });
        }
        Ok(Self {
            title: text(value.get("title")),
            updated: value.get("updated").and_then(Value::as_i64),
            refresh: value.get("refresh").and_then(Value::as_u64),
            empty: text(value.get("empty")),
            items,
        })
    }
}
