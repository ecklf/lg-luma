//! LG tuner channels and now-playing guide (apiadapter / Glasshouse-compatible).

use crate::app_artwork;
use crate::model::{Channel, ChannelGuideEntry, ChannelProgram, valid_app_id};
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;

pub const MAX_FAVORITES: usize = 40;
pub const LOGO_ROOT: &str = "/mnt/lg/cmn_data/channel_logo";

/// `luna://com.webos.service.apiadapter/tv/getChannelList` → `channelList`.
pub fn normalize_channels(raw: &Value) -> Result<Vec<Channel>, String> {
    let entries = raw
        .as_array()
        .filter(|a| a.len() <= 2000)
        .ok_or("Invalid channel inventory")?;
    let mut seen = std::collections::HashSet::new();
    let mut channels = Vec::new();
    for entry in entries {
        let Some(id) = entry
            .get("channelId")
            .and_then(Value::as_str)
            .filter(|id| valid_app_id(id))
        else {
            continue;
        };
        if entry.get("skipped") == Some(&Value::Bool(true))
            || entry.get("Invisible") == Some(&Value::Bool(true))
            || !seen.insert(id)
        {
            continue;
        }
        let number = entry
            .get("channelNumber")
            .and_then(Value::as_str)
            .unwrap_or("")
            .chars()
            .filter(|c| !c.is_control())
            .take(16)
            .collect::<String>();
        let name = entry
            .get("channelName")
            .and_then(Value::as_str)
            .unwrap_or("")
            .chars()
            .filter(|c| !c.is_control())
            .take(120)
            .collect::<String>();
        let name = {
            let trimmed = name.trim();
            if trimmed.is_empty() {
                if number.is_empty() {
                    id.to_owned()
                } else {
                    number.clone()
                }
            } else {
                trimmed.to_owned()
            }
        };
        let logo = entry
            .get("imgUrl")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|url| {
                !url.is_empty() && url.len() <= 4096 && !url.chars().any(char::is_control)
            })
            .filter(|url| {
                let path = Path::new(url.strip_prefix("file://").unwrap_or(url));
                path.is_absolute()
                    && path.starts_with(LOGO_ROOT)
                    && path
                        .components()
                        .all(|c| !matches!(c, std::path::Component::ParentDir))
            })
            .map(str::to_owned);
        let icon = logo
            .as_deref()
            .map(|path| app_artwork::Source::channel_logo(id, path));
        channels.push(Channel {
            id: id.into(),
            number,
            name,
            radio: entry.get("Radio") == Some(&Value::Bool(true)),
            logo,
            icon,
            program: None,
            next: None,
        });
    }
    fill_missing_logos_from_name_variants(&mut channels);
    Ok(channels)
}

/// Strip trailing quality tags so "TV3 UHD" and "TV3" share a logo key.
pub fn logo_match_key(name: &str) -> String {
    let mut key = name
        .chars()
        .filter(|c| !c.is_control())
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    const SUFFIXES: &[&str] = &[
        " uhd", " fhd", " hd", " sd", " 4k", " hdr", " hevc", " uhdtv", " hdtv",
    ];
    loop {
        let before = key.clone();
        for suffix in SUFFIXES {
            if let Some(rest) = key.strip_suffix(suffix) {
                key = rest.trim_end().into();
            }
        }
        if key == before || key.is_empty() {
            break;
        }
    }
    key
}

/// When LG omits `imgUrl` on a HD/UHD duplicate, reuse a logo from the same base name.
fn fill_missing_logos_from_name_variants(channels: &mut [Channel]) {
    let mut donors: HashMap<(String, bool), (String, bool)> = HashMap::new();
    for channel in channels.iter() {
        let Some(logo) = channel.logo.as_ref() else {
            continue;
        };
        let key = logo_match_key(&channel.name);
        if key.is_empty() {
            continue;
        }
        let normalized = channel
            .name
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase();
        let exact = key == normalized;
        match donors.entry((key, channel.radio)) {
            std::collections::hash_map::Entry::Vacant(slot) => {
                slot.insert((logo.clone(), exact));
            }
            std::collections::hash_map::Entry::Occupied(mut slot) => {
                if exact && !slot.get().1 {
                    slot.insert((logo.clone(), true));
                }
            }
        }
    }
    for channel in channels.iter_mut() {
        if channel.logo.is_some() {
            continue;
        }
        let key = logo_match_key(&channel.name);
        let Some((logo, _)) = donors.get(&(key, channel.radio)) else {
            continue;
        };
        channel.icon = Some(app_artwork::Source::channel_logo(&channel.id, logo));
        channel.logo = Some(logo.clone());
    }
}

fn program_title(raw: &Value) -> Option<String> {
    let title = raw
        .get("programName")
        .and_then(Value::as_str)
        .unwrap_or("")
        .chars()
        .filter(|c| !c.is_control())
        .take(160)
        .collect::<String>();
    let title = title.trim();
    (!title.is_empty()).then(|| title.into())
}

fn program_from_entry(raw: &Value) -> Option<ChannelProgram> {
    Some(ChannelProgram {
        title: program_title(raw)?,
        start: guide_time(raw.get("startTime").and_then(Value::as_str)),
        end: guide_time(raw.get("endTime").and_then(Value::as_str)),
    })
}

/// `getChannelCurrentProgramInfo` reply → now-playing when the show is current.
pub fn normalize_program(raw: &Value) -> Option<ChannelProgram> {
    if raw.get("returnValue") == Some(&Value::Bool(false)) {
        return None;
    }
    let program = program_from_entry(raw)?;
    let now = unix_now()?;
    if let (Some(start), Some(end)) = (program.start, program.end)
        && (start > now || end <= now)
    {
        return None;
    }
    Some(program)
}

/// `getChannelProgramInfo` → now and the following show when times are present.
pub fn normalize_schedule(raw: &Value) -> ChannelGuideEntry {
    if raw.get("returnValue") == Some(&Value::Bool(false)) {
        return ChannelGuideEntry::default();
    }
    let Some(list) = raw.get("programList").and_then(Value::as_array) else {
        return ChannelGuideEntry {
            program: normalize_program(raw),
            next: None,
        };
    };
    let Some(now) = unix_now() else {
        return ChannelGuideEntry::default();
    };
    let mut programs: Vec<ChannelProgram> = list
        .iter()
        .filter_map(program_from_entry)
        .take(64)
        .collect();
    programs.sort_by_key(|program| program.start.unwrap_or(u64::MAX));
    let current = programs.iter().position(|program| {
        matches!(
            (program.start, program.end),
            (Some(start), Some(end)) if start <= now && now < end
        )
    });
    let program = current.map(|index| programs[index].clone());
    let next = match current {
        Some(index) => programs.get(index + 1).cloned(),
        None => programs
            .into_iter()
            .find(|program| program.start.is_some_and(|start| start > now)),
    };
    ChannelGuideEntry { program, next }
}

fn unix_now() -> Option<u64> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_secs())
}

/// Glasshouse `guideTime`: `"2026,11,14,12,05,00"` is UTC.
pub fn guide_time(value: Option<&str>) -> Option<u64> {
    let parts: Vec<_> = value?.split(',').collect();
    if parts.len() < 5 {
        return None;
    }
    let year: i32 = parts[0].parse().ok()?;
    let month: u8 = parts[1].parse().ok()?;
    let day: u8 = parts[2].parse().ok()?;
    let hour: u8 = parts[3].parse().ok()?;
    let minute: u8 = parts[4].parse().ok()?;
    let second: u8 = parts.get(5).and_then(|s| s.parse().ok()).unwrap_or(0);
    let date =
        time::Date::from_calendar_date(year, time::Month::try_from(month).ok()?, day).ok()?;
    let datetime = date.with_hms(hour, minute, second).ok()?.assume_utc();
    Some(datetime.unix_timestamp().max(0) as u64)
}

pub fn apply_programs(channels: &mut [Channel], programs: &HashMap<String, ChannelGuideEntry>) {
    for channel in channels {
        if let Some(entry) = programs.get(&channel.id) {
            channel.program = entry.program.clone();
            channel.next = entry.next.clone();
        } else {
            channel.program = None;
            channel.next = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn normalizes_glasshouse_shaped_channel_list() {
        let raw = json!([
            {
                "channelId": "1_31_1_0_1008_494_8916",
                "channelNumber": "1",
                "channelName": "La 1 HD",
                "Radio": false,
                "skipped": false,
                "Invisible": false,
                "imgUrl": "/mnt/lg/cmn_data/channel_logo/major_logo_img/la1.png"
            },
            {
                "channelId": "skip_me",
                "channelNumber": "2",
                "channelName": "Hidden",
                "Invisible": true
            }
        ]);
        let channels = normalize_channels(&raw).unwrap();
        assert_eq!(channels.len(), 1);
        assert_eq!(channels[0].id, "1_31_1_0_1008_494_8916");
        assert_eq!(channels[0].number, "1");
        assert_eq!(channels[0].name, "La 1 HD");
        assert!(channels[0].logo.is_some());
        assert!(channels[0].icon.is_some());
    }

    #[test]
    fn borrows_logo_from_same_name_without_quality_suffix() {
        let raw = json!([
            {
                "channelId": "tv3_uhd",
                "channelNumber": "3",
                "channelName": "TV3 UHD",
                "Radio": false,
                "skipped": false,
                "Invisible": false
            },
            {
                "channelId": "tv3_sd",
                "channelNumber": "16",
                "channelName": "TV3",
                "Radio": false,
                "imgUrl": "/mnt/lg/cmn_data/channel_logo/minor_logo_img/tv3.png"
            },
            {
                "channelId": "telecinco",
                "channelNumber": "5",
                "channelName": "Telecinco",
                "Radio": false
            }
        ]);
        let channels = normalize_channels(&raw).unwrap();
        let uhd = channels.iter().find(|c| c.id == "tv3_uhd").unwrap();
        assert_eq!(
            uhd.logo.as_deref(),
            Some("/mnt/lg/cmn_data/channel_logo/minor_logo_img/tv3.png")
        );
        assert!(uhd.icon.is_some());
        assert!(
            channels
                .iter()
                .find(|c| c.id == "telecinco")
                .unwrap()
                .logo
                .is_none()
        );
        assert_eq!(logo_match_key("La 1 HD"), "la 1");
        assert_eq!(logo_match_key("TV3 UHD"), "tv3");
    }

    #[test]
    fn program_requires_current_window_when_times_present() {
        let past = json!({
            "returnValue": true,
            "programName": "Old",
            "startTime": "2020,01,01,00,00,00",
            "endTime": "2020,01,01,01,00,00"
        });
        assert!(normalize_program(&past).is_none());
        let open = json!({
            "returnValue": true,
            "programName": "Live show",
            "startTime": "2000,01,01,00,00,00",
            "endTime": "2099,01,01,00,00,00"
        });
        let program = normalize_program(&open).unwrap();
        assert_eq!(program.title, "Live show");
    }

    #[test]
    fn schedule_picks_current_and_following_show() {
        let raw = json!({
            "returnValue": true,
            "programList": [
                {
                    "programName": "Earlier",
                    "startTime": "2000,01,01,00,00,00",
                    "endTime": "2000,01,01,01,00,00"
                },
                {
                    "programName": "Now playing",
                    "startTime": "2000,01,01,00,00,00",
                    "endTime": "2099,01,01,00,00,00"
                },
                {
                    "programName": "Up next",
                    "startTime": "2099,01,01,00,00,00",
                    "endTime": "2099,01,01,01,00,00"
                }
            ]
        });
        let guide = normalize_schedule(&raw);
        assert_eq!(
            guide.program.as_ref().map(|p| p.title.as_str()),
            Some("Now playing")
        );
        assert_eq!(
            guide.next.as_ref().map(|p| p.title.as_str()),
            Some("Up next")
        );
    }
}
