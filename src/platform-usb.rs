//! TV USB storage is discovered separately from EIM's HDMI source inventory.
//! Do not synthesize app IDs from USB names or guess firmware-specific deep links.
use super::{Input, Value, call, inventory_read, inventory_read_with_payload, json, title};
use std::time::{Duration, Instant};

const LIST: &str = "luna://com.webos.service.attachedstoragemanager/listDevices";
const MEDIA_APPS: [&str; 3] = [
    "com.webos.app.mediadiscovery",
    "com.webos.app.photovideo",
    "com.webos.app.smartshare",
];

#[derive(Default)]
pub(super) struct Storage {
    entries: Vec<Input>,
    retry: Option<Instant>,
}

impl Storage {
    pub(super) fn refresh(&mut self) -> &[Input] {
        self.refresh_with(Instant::now(), read)
    }

    fn refresh_with(
        &mut self,
        now: Instant,
        read: impl FnOnce() -> Result<Vec<Input>, String>,
    ) -> &[Input] {
        if self.retry.is_some_and(|retry| now < retry) {
            return &self.entries;
        }
        match read() {
            Ok(entries) => {
                self.entries = entries;
                self.retry = None;
            }
            Err(error) => {
                // Optional service: never hide HDMI or replace a failed read with demo data.
                eprintln!("Luma USB storage refresh: {error}");
                self.retry = Some(now + Duration::from_secs(10));
            }
        }
        &self.entries
    }
}

fn read() -> Result<Vec<Input>, String> {
    normalize(&inventory_read_with_payload(
        LIST,
        "devices",
        json!({"deviceType": ["usb"]}),
    )?)
}

fn normalize(raw: &Value) -> Result<Vec<Input>, String> {
    let entries = raw
        .as_array()
        .filter(|entries| entries.len() <= 128)
        .ok_or("Invalid USB storage inventory")?;
    let mut result = Vec::new();
    for entry in entries {
        if entry.get("deviceType").and_then(Value::as_str) != Some("usb") {
            continue;
        }
        let Some(device) = entry
            .get("deviceId")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty() && id.len() <= 1024 && !id.chars().any(char::is_control))
        else {
            continue;
        };
        // Bounded, preference-safe identity; never sent as an app ID. The service's
        // opaque ID can contain punctuation or be longer than a preference key.
        let digest = ring::digest::digest(&ring::digest::SHA256, device.as_bytes());
        let id = format!(
            "luma.usb.{}",
            digest
                .as_ref()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        );
        // An ambiguous identity must not become a launch target.
        if entries
            .iter()
            .filter(|entry| entry.get("deviceId").and_then(Value::as_str) == Some(device))
            .count()
            != 1
        {
            continue;
        }
        result.push(Input {
            id,
            title: title(entry.get("deviceName").and_then(Value::as_str), "USB"),
            port: "USB".into(),
            connected: Some(true),
            signal: None,
        });
    }
    result.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(result)
}

fn media_app(apps: &Value) -> Result<&'static str, String> {
    let apps = apps
        .as_array()
        .filter(|apps| apps.len() <= 1000)
        .ok_or("Invalid app inventory")?;
    MEDIA_APPS
        .into_iter()
        .find(|id| {
            apps.iter()
                .any(|app| app.get("id").and_then(Value::as_str) == Some(id))
        })
        .ok_or_else(|| "No supported LG media browser is installed".into())
}

pub(super) fn launch(id: &str) -> Result<(), String> {
    // Recheck presence: a retained snapshot after a failed read is not launch authority.
    if !read()?.iter().any(|input| input.id == id) {
        return Err("USB storage is no longer connected".into());
    }
    let apps = inventory_read("luna://com.webos.applicationManager/listApps", "apps")?;
    let app = media_app(&apps)?;
    // Open the installed media browser's device chooser. Device-specific launch params
    // vary by firmware and are deliberately not guessed or retried after a failure.
    call(
        "luna://com.webos.applicationManager/launch",
        json!({"id": app}),
    )
    .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usb_is_storage_not_an_hdmi_app_and_uses_device_names() {
        let inputs = normalize(&json!([
            {"deviceType":"usb", "deviceId":"usb:drive/1", "deviceName":" Movies\n "},
            {"deviceType":"dms", "deviceId":"server"},
            {"deviceType":"internal", "deviceId":"internal"}
        ]))
        .unwrap();
        assert_eq!(inputs.len(), 1);
        let input = &inputs[0];
        assert!(input.is_usb_storage());
        assert!(crate::model::valid_app_id(&input.id));
        assert_eq!(input.title, "Movies");
        assert_eq!(input.connected, Some(true));
        assert_eq!(input.signal, None);
        let mut preferences = crate::model::Preferences::default();
        assert_eq!(preferences.input_title(input), "Movies");
        assert!(preferences.input_visible(input));
        preferences.set_input_name(&input.id, "Videos");
        assert_eq!(preferences.input_title(input), "Videos");
    }

    #[test]
    fn malformed_ambiguous_and_oversized_devices_are_not_sources() {
        assert!(normalize(&json!({})).is_err());
        assert!(normalize(&json!(vec![json!({}); 129])).is_err());
        let inputs = normalize(&json!([
            {"deviceType":"usb", "deviceId":"duplicate"},
            {"deviceType":"usb", "deviceId":"duplicate"},
            {"deviceType":"usb", "deviceId":""},
            {"deviceType":"usb", "deviceId":"bad\n"},
            {"deviceType":"usb", "deviceId":"x".repeat(1025)},
            {"deviceType":"usb"}
        ]))
        .unwrap();
        assert!(inputs.is_empty());
        assert!(normalize(&json!([])).unwrap().is_empty());
    }

    #[test]
    fn media_browser_must_be_installed_even_when_hidden_from_app_catalogue() {
        assert_eq!(
            media_app(&json!([
                {"id":"com.webos.app.photovideo", "visible":false},
                {"id":"com.webos.app.mediadiscovery", "visible":false}
            ]))
            .unwrap(),
            MEDIA_APPS[0]
        );
        assert!(media_app(&json!([{"id":"third.party.media"}])).is_err());
        assert!(media_app(&json!({})).is_err());
    }

    #[test]
    fn failed_reads_retain_storage_back_off_and_successful_empty_reads_remove_it() {
        let now = Instant::now();
        let inputs = normalize(&json!([{"deviceType":"usb", "deviceId":"drive"}])).unwrap();
        let mut storage = Storage::default();
        assert_eq!(storage.refresh_with(now, || Ok(inputs.clone())), inputs);
        assert_eq!(storage.refresh_with(now, || Err("denied".into())), inputs);
        assert_eq!(
            storage.refresh_with(now + Duration::from_secs(9), || panic!("must back off")),
            inputs
        );
        assert!(
            storage
                .refresh_with(now + Duration::from_secs(10), || Ok(vec![]))
                .is_empty()
        );
        assert!(storage.retry.is_none());
    }

    #[test]
    fn order_and_labels_do_not_change_device_identity() {
        let first = normalize(&json!([
            {"deviceType":"usb", "deviceId":"one", "deviceName":"A"},
            {"deviceType":"usb", "deviceId":"two", "deviceName":"B"}
        ]))
        .unwrap();
        let second = normalize(&json!([
            {"deviceType":"usb", "deviceId":"two", "deviceName":"C"},
            {"deviceType":"usb", "deviceId":"one", "deviceName":"D"}
        ]))
        .unwrap();
        assert_eq!(
            first.iter().map(|input| &input.id).collect::<Vec<_>>(),
            second.iter().map(|input| &input.id).collect::<Vec<_>>()
        );
        let preferences = crate::model::Preferences::default();
        assert_eq!(
            crate::view::input_status(&first[0], false, preferences.language),
            preferences.language.t("connected")
        );
    }
}
