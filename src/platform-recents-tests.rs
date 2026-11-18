use super::*;
use crate::recents::{Command, normalize};

#[test]
fn legacy_list_uses_real_running_cards_and_preserves_system_apps() {
    let result = recent_call_with(&Command::List, true, |uri, _| {
        assert_eq!(uri, "luna://com.webos.applicationManager/running");
        Ok(json!({"running": [
            {"id":"netflix", "defaultWindowType":"card"},
            {"id":"netflix", "defaultWindowType":"card"},
            {"id":"com.ecklf.luma", "defaultWindowType":"card"},
            {"id":"com.webos.app.home", "defaultWindowType":"card"},
            {"id":"com.webos.app.hdmi1", "defaultWindowType":"card"},
            {"id":"com.webos.app.livetv", "defaultWindowType":"card"},
            {"id":"airplay", "defaultWindowType":"card"},
            {"id":"com.webos.app.container", "defaultWindowType":"card"},
            {"id":"native.settings", "defaultWindowType":"overlay"}
        ]}))
    })
    .unwrap();
    let ids: Vec<_> = normalize(&result)
        .unwrap()
        .into_iter()
        .map(|e| e.app.id)
        .collect();
    assert_eq!(ids, ["netflix"]);
}

#[test]
fn legacy_close_uses_sam_without_mutating_missing_history() {
    let result = recent_call_with(&Command::Close("netflix".into()), true, |uri, payload| {
        assert_eq!(uri, "luna://com.webos.applicationManager/closeByAppId");
        assert_eq!(payload, json!({"id":"netflix"}));
        Err("close refused".into())
    });
    assert_eq!(result.unwrap_err(), "close refused");
}

#[test]
fn legacy_launch_uses_older_application_manager() {
    recent_call_with(&Command::Launch("netflix".into()), true, |uri, payload| {
        assert_eq!(uri, "luna://com.webos.applicationManager/launch");
        assert_eq!(payload, json!({"id":"netflix"}));
        Ok(json!({"returnValue":true}))
    })
    .unwrap();
}

#[test]
fn unavailable_or_invalid_running_inventory_remains_an_error() {
    assert!(recent_call_with(&Command::List, true, |_, _| Err("offline".into())).is_err());
    assert!(recent_call_with(&Command::List, true, |_, _| Ok(json!({}))).is_err());
}

#[test]
fn protected_home_is_rejected_before_a_mutation() {
    assert!(
        recent_call_with(&Command::Close(APP_ID.into()), true, |_, _| {
            panic!("protected application must not be closed")
        })
        .is_err()
    );
}

#[test]
fn modern_history_operations_are_unchanged_even_with_empty_history() {
    for command in [
        Command::List,
        Command::Close("netflix".into()),
        Command::Launch("netflix".into()),
    ] {
        let expected = command.request().unwrap();
        recent_call_with(&command, false, |uri, payload| {
            assert_eq!(uri, expected.0);
            assert_eq!(payload, expected.1);
            Ok(json!({"recentsAppList": []}))
        })
        .unwrap();
    }
}
