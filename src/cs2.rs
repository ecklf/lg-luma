//! CS2 Score: Counter-Strike 2 matches for Home from a shared feed built from LiquipediaDB
//! (https://github.com/eluz1ve/luma-feeds). The feed holds the API key on its side and is
//! rebuilt every two minutes; the TV only reads the published file, every minute while Home
//! shows the widget, and the teams' logos from Liquipedia's commons once each.
use crate::app_artwork::Artwork;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use std::{collections::HashMap, sync::Arc};
use time::OffsetDateTime;

pub const REFRESH_SECONDS: i64 = 60;
const RETRY_SECONDS: i64 = 60;
/// Where team logos may come from.
pub const LOGO_ORIGIN: &str = "https://liquipedia.net/commons/images/";
/// Logos kept in memory; more are forgotten and fetched again if needed.
const LOGO_CACHE: usize = 64;
/// A feed older than this is shown with when it was last updated, as it may be behind.
pub const MAX_AGE: i64 = 30 * 60;
/// Matches the row lists; three cards show at a time and Left/Right scroll through the rest.
pub const SHOWN: usize = 12;
pub const VISIBLE: usize = 3;

/// Opt-in switches: only `true` turns them on, and anything malformed leaves them off.
pub fn deserialize_flag<'de, D: Deserializer<'de>>(deserializer: D) -> Result<bool, D::Error> {
    Ok(Value::deserialize(deserializer)? == Value::Bool(true))
}
/// Switches that are on by default: only `false` turns them off.
pub fn deserialize_enabled<'de, D: Deserializer<'de>>(deserializer: D) -> Result<bool, D::Error> {
    Ok(Value::deserialize(deserializer)? != Value::Bool(false))
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub enum State {
    Live,
    Upcoming,
    Finished,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Team {
    pub name: String,
    pub short: String,
    /// The logo's URL on Liquipedia's commons.
    pub logo: Option<String>,
    /// The team's page on Liquipedia, to find its coaches.
    pub page: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Coach {
    pub name: String,
    pub assistant: bool,
}

/// Which round a match is: one of Liquipedia's bracket rounds, or a header as written.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub enum Round {
    GrandFinal,
    Final,
    Semifinals,
    Quarterfinals,
    /// The round of the last `n` teams, e.g. the round of 16.
    Top(u32),
    ThirdPlace,
    Named(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub enum Bracket {
    Upper,
    Lower,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Player {
    pub name: String,
    pub flag: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Map {
    pub name: String,
    pub score: Option<[u32; 2]>,
    /// 0 or 1 once the map is decided.
    pub winner: Option<usize>,
    /// A screenshot of the map on Liquipedia's commons.
    pub image: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Match {
    pub id: String,
    /// Unix seconds.
    pub begins: i64,
    pub state: State,
    pub best_of: Option<u32>,
    pub tournament: String,
    /// The tournament's logo URL on Liquipedia's commons.
    pub icon: Option<String>,
    pub stage: String,
    pub round: Option<(Round, Option<Bracket>)>,
    /// Liquipedia's 1–5, 1 the biggest.
    pub tier: Option<u32>,
    /// `None` while a side is still to be decided.
    pub teams: [Option<Team>; 2],
    /// Maps won so far.
    pub score: Option<[u32; 2]>,
    pub maps: Vec<Map>,
    /// The players Liquipedia lists for each side.
    pub lineups: [Vec<Player>; 2],
}
impl Match {
    /// The map whose picture stands behind the score: the one being played, else the last one
    /// decided.
    pub fn featured_map(&self) -> Option<&Map> {
        match self.state {
            State::Live => self.current_map().map(|(_, map)| map),
            State::Finished => self.maps.iter().rev().find(|map| map.winner.is_some()),
            State::Upcoming => None,
        }
    }
    pub fn major(&self) -> bool {
        self.tier.is_some_and(|tier| tier <= 2)
    }
    /// The map being played: the first one not yet decided, with its number from 1.
    pub fn current_map(&self) -> Option<(usize, &Map)> {
        self.maps
            .iter()
            .enumerate()
            .find(|(_, map)| map.winner.is_none())
            .map(|(index, map)| (index + 1, map))
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Feed {
    /// Unix seconds when the feed was built.
    pub updated: i64,
    pub matches: Vec<Match>,
    /// Coaches by team page.
    pub staff: HashMap<String, Vec<Coach>>,
}
impl Feed {
    pub fn coaches(&self, team: &Team) -> &[Coach] {
        team.page
            .as_ref()
            .and_then(|page| self.staff.get(page))
            .map_or(&[], Vec::as_slice)
    }
}

fn liquipedia_image(value: &Value) -> Option<String> {
    value
        .as_str()
        .filter(|url| url.starts_with(LOGO_ORIGIN) && url.len() <= 300)
        .map(String::from)
}

fn round(value: &Value) -> Option<(Round, Option<Bracket>)> {
    let round = match value["key"].as_str() {
        Some("grand_final") => Round::GrandFinal,
        Some("final") => Round::Final,
        Some("semifinals") => Round::Semifinals,
        Some("quarterfinals") => Round::Quarterfinals,
        Some("third_place") => Round::ThirdPlace,
        Some("round_of") => Round::Top(
            value["number"]
                .as_u64()
                .filter(|n| (2..=1024).contains(n))? as u32,
        ),
        _ => {
            let name = text(&value["text"], 40);
            if name.is_empty() {
                return None;
            }
            Round::Named(name)
        }
    };
    let bracket = match value["bracket"].as_str() {
        Some("upper") => Some(Bracket::Upper),
        Some("lower") => Some(Bracket::Lower),
        _ => None,
    };
    Some((round, bracket))
}

fn text(value: &Value, limit: usize) -> String {
    value
        .as_str()
        .unwrap_or("")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .filter(|c| !c.is_control())
        .take(limit)
        .collect()
}

fn pair(value: &Value) -> Option<[u32; 2]> {
    let values = value.as_array()?;
    match values.as_slice() {
        [a, b] => Some([
            u32::try_from(a.as_u64()?).ok()?,
            u32::try_from(b.as_u64()?).ok()?,
        ]),
        _ => None,
    }
}

fn side(value: &Value) -> Option<usize> {
    value
        .as_u64()
        .filter(|side| *side < 2)
        .map(|side| side as usize)
}

fn team(value: &Value) -> Option<Team> {
    let name = text(&value["name"], 60);
    if name.is_empty() {
        return None;
    }
    let short = text(&value["short"], 16);
    let logo = liquipedia_image(&value["image"]);
    let page = Some(text(&value["page"], 120)).filter(|page| !page.is_empty());
    Some(Team {
        short: if short.is_empty() {
            name.clone()
        } else {
            short
        },
        name,
        logo,
        page,
    })
}

fn lineup(value: &Value) -> Vec<Player> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .take(10)
        .filter_map(|player| {
            let name = text(&player["name"], 32);
            (!name.is_empty()).then(|| Player {
                name,
                flag: text(&player["flag"], 40),
            })
        })
        .collect()
}

fn parse_match(value: &Value) -> Option<Match> {
    let begins = OffsetDateTime::parse(
        value["begin_at"].as_str()?,
        &time::format_description::well_known::Rfc3339,
    )
    .ok()?
    .unix_timestamp();
    let state = match value["state"].as_str()? {
        "live" => State::Live,
        "upcoming" => State::Upcoming,
        "finished" => State::Finished,
        _ => return None,
    };
    let teams = value["teams"].as_array()?;
    let [first, second] = teams.as_slice() else {
        return None;
    };
    Some(Match {
        id: text(&value["id"], 64),
        begins,
        state,
        best_of: value["best_of"]
            .as_u64()
            .filter(|n| (1..=7).contains(n))
            .map(|n| n as u32),
        tournament: text(&value["tournament"], 80),
        icon: liquipedia_image(&value["icon"]),
        stage: text(&value["stage"], 40),
        round: round(&value["round"]),
        tier: value["tier"]
            .as_u64()
            .filter(|n| (1..=5).contains(n))
            .map(|n| n as u32),
        teams: [team(first), team(second)],
        score: pair(&value["score"]),
        maps: value["maps"]
            .as_array()
            .into_iter()
            .flatten()
            .take(7)
            .filter_map(|map| {
                let name = text(&map["map"], 40);
                (!name.is_empty()).then(|| Map {
                    name,
                    score: pair(&map["score"]),
                    winner: side(&map["winner"]),
                    image: liquipedia_image(&map["image"]),
                })
            })
            .collect(),
        lineups: [lineup(&value["lineups"][0]), lineup(&value["lineups"][1])],
    })
}

/// Reads the feed; malformed matches are left out rather than failing the rest.
pub fn parse_feed(value: &Value) -> Result<Feed, String> {
    let updated = value["updated"]
        .as_i64()
        .ok_or("The score feed returned unreadable data.")?;
    let matches = value["matches"]
        .as_array()
        .ok_or("The score feed returned unreadable data.")?
        .iter()
        .take(200)
        .filter_map(parse_match)
        .collect();
    let staff = value["staff"]
        .as_object()
        .into_iter()
        .flatten()
        .take(200)
        .map(|(page, people)| {
            let coaches = people
                .as_array()
                .into_iter()
                .flatten()
                .take(3)
                .filter_map(|person| {
                    let name = text(&person["name"], 32);
                    (!name.is_empty()).then(|| Coach {
                        name,
                        assistant: text(&person["role"], 60)
                            .to_lowercase()
                            .contains("assistant"),
                    })
                })
                .collect();
            (text(&Value::String(page.clone()), 120), coaches)
        })
        .collect();
    Ok(Feed {
        updated,
        matches,
        staff,
    })
}

/// The cards Home shows: live matches first, then upcoming ones, then the latest results.
pub fn shown(feed: &Feed, major_only: bool, results: bool) -> Vec<&Match> {
    let wanted = |m: &&Match| (!major_only || m.major()) && (results || m.state != State::Finished);
    let mut picked: Vec<&Match> = Vec::new();
    for state in [State::Live, State::Upcoming, State::Finished] {
        picked.extend(
            feed.matches
                .iter()
                .filter(|m| m.state == state)
                .filter(wanted),
        );
    }
    picked.truncate(SHOWN);
    picked
}

pub struct Command {
    pub id: u64,
}

pub enum Logo {
    Loading,
    Ready(Arc<Artwork>),
    Missing,
}

#[derive(Default, Serialize)]
pub struct Scores {
    pub feed: Option<Feed>,
    pub loading: bool,
    pub failed: bool,
    #[serde(skip)]
    pub logos: HashMap<String, Logo>,
    #[serde(skip)]
    id: u64,
    #[serde(skip)]
    pending: Option<u64>,
    #[serde(skip)]
    next_fetch: i64,
}
impl Scores {
    pub fn fetching(&self) -> bool {
        self.pending.is_some()
    }
    pub fn generation(&self) -> u64 {
        self.id
    }
    /// The match with this id, for its dialog.
    pub fn find(&self, id: &str) -> Option<&Match> {
        self.feed.as_ref()?.matches.iter().find(|m| m.id == id)
    }
    /// The logo for a team, once fetched.
    pub fn logo(&self, team: &Team) -> Option<Arc<Artwork>> {
        self.image(team.logo.as_ref()?)
    }
    /// A team or tournament logo, once fetched.
    pub fn image(&self, url: &str) -> Option<Arc<Artwork>> {
        match self.logos.get(url)? {
            Logo::Ready(image) => Some(image.clone()),
            _ => None,
        }
    }
    /// Pictures not asked for yet, marked as loading: the given matches' logos and, for the
    /// match open in detail, its maps' screenshots. Each comes with whether it is a backdrop.
    pub fn wanted_images(&mut self, ids: &[String], detail: Option<&str>) -> Vec<(String, bool)> {
        let Some(feed) = &self.feed else {
            return Vec::new();
        };
        let backdrops: Vec<String> = feed
            .matches
            .iter()
            .filter(|m| Some(m.id.as_str()) == detail)
            .flat_map(|m| m.maps.iter().filter_map(|map| map.image.clone()))
            .filter(|url| !self.logos.contains_key(url))
            .collect();
        let mut urls: Vec<String> = feed
            .matches
            .iter()
            .filter(|m| ids.contains(&m.id))
            .flat_map(|m| {
                m.teams
                    .iter()
                    .flatten()
                    .filter_map(|team| team.logo.clone())
                    .chain(m.icon.clone())
            })
            .filter(|url| !self.logos.contains_key(url))
            .collect();
        urls.sort();
        urls.dedup();
        let mut wanted: Vec<(String, bool)> = urls.into_iter().map(|url| (url, false)).collect();
        for url in backdrops {
            if !wanted.iter().any(|(known, _)| *known == url) {
                wanted.push((url, true));
            }
        }
        if self.logos.len() + wanted.len() > LOGO_CACHE {
            self.logos.clear();
        }
        for (url, _) in &wanted {
            self.logos.insert(url.clone(), Logo::Loading);
        }
        wanted
    }
    pub fn accept_logo(&mut self, url: String, image: Option<Arc<Artwork>>) -> bool {
        if !matches!(self.logos.get(&url), Some(Logo::Loading)) {
            return false;
        }
        self.logos
            .insert(url, image.map_or(Logo::Missing, Logo::Ready));
        true
    }
    /// The last feed received, however old: old scores beat none while the feed is down.
    pub fn current(&self, _now: i64) -> Option<&Feed> {
        self.feed.as_ref()
    }
    /// Whether the feed is old enough to say when it was last updated.
    pub fn stale(&self, now: i64) -> bool {
        self.feed
            .as_ref()
            .is_some_and(|feed| now - feed.updated > MAX_AGE)
    }
    /// Starts a fetch when Home shows the widget and the scores are due. Returns the request to
    /// send and whether anything changed.
    pub fn sync(&mut self, enabled: bool, home_visible: bool, now: i64) -> (Option<Command>, bool) {
        let before = (self.loading, self.failed);
        let mut command = None;
        if !enabled || !home_visible {
            if self.pending.take().is_some() {
                self.id += 1;
            }
            self.loading = false;
        } else if self.pending.is_none() && now >= self.next_fetch {
            self.id += 1;
            self.pending = Some(self.id);
            self.loading = self.current(now).is_none();
            command = Some(Command { id: self.id });
        }
        (command, before != (self.loading, self.failed))
    }
    /// Takes the reply to the request in flight; replies to older requests are ignored.
    pub fn accept(&mut self, id: u64, result: Result<Feed, String>, now: i64) -> bool {
        if self.pending != Some(id) {
            return false;
        }
        self.pending = None;
        self.loading = false;
        match result {
            Ok(feed) => {
                self.feed = Some(feed);
                self.failed = false;
                self.next_fetch = now + REFRESH_SECONDS;
            }
            Err(_) => {
                self.failed = true;
                self.next_fetch = now + RETRY_SECONDS;
            }
        }
        true
    }
}

#[cfg(test)]
#[path = "cs2-tests.rs"]
mod tests;
