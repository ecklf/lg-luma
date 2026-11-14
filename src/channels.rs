//! LG tuner channels and now-playing guide (apiadapter / Glasshouse-compatible).

use crate::app_artwork;
use crate::model::{Channel, ChannelProgram, valid_app_id};
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
        });
    }
    Ok(channels)
}

/// `getChannelCurrentProgramInfo` reply → now-playing when the show is current.
pub fn normalize_program(raw: &Value) -> Option<ChannelProgram> {
    if raw.get("returnValue") == Some(&Value::Bool(false)) {
        return None;
    }
    let title = raw
        .get("programName")
        .and_then(Value::as_str)
        .unwrap_or("")
        .chars()
        .filter(|c| !c.is_control())
        .take(160)
        .collect::<String>();
    let title = title.trim();
    if title.is_empty() {
        return None;
    }
    let start = guide_time(raw.get("startTime").and_then(Value::as_str));
    let end = guide_time(raw.get("endTime").and_then(Value::as_str));
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs();
    if let (Some(start), Some(end)) = (start, end) {
        if start > now || end <= now {
            return None;
        }
    }
    Some(ChannelProgram {
        title: title.into(),
        start,
        end,
    })
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

pub fn apply_programs(channels: &mut [Channel], programs: &HashMap<String, ChannelProgram>) {
    for channel in channels {
        channel.program = programs.get(&channel.id).cloned();
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
}
