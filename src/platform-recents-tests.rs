use super::*;
use crate::recents::{Command, normalize};

fn running() -> Value {
    json!({"returnValue": true, "running": [
        {"id":"org.example.video", "defaultWindowType":"card"},
        {"id":"netflix", "defaultWindowType":"card"},
        {"id":"com.ecklf.luma", "defaultWindowType":"card"},
        {"id":"com.webos.app.hdmi1", "defaultWindowType":"card"},
        {"id":"com.webos.app.livetv", "defaultWindowType":"card"},
        {"id":"airplay", "defaultWindowType":"card"},
        {"id":"com.webos.app.container", "defaultWindowType":"card"},
        {"id":"native.settings", "defaultWindowType":"overlay"}
    ]})
}

#[test]
fn legacy_list_uses_real_running_apps_without_touching_absent_history_service() {
    let result = recent_call_with(&Command::List, true, |uri, _| {
        assert_eq!(uri, "luna://com.webos.applicationManager/running");
        Ok(running())
    })
    .unwrap();
    let ids: Vec<_> = normalize(&result)
        .unwrap()
        .into_iter()
        .map(|e| e.app.id)
        .collect();
    assert_eq!(ids, ["org.example.video", "netflix"]);
}

#[test]
fn legacy_close_all_preserves_inputs_services_home_and_overlays_and_skips_history() {
    let mut closed = vec![];
    recent_call_with(
        &Command::CloseAll(vec!["netflix".into()]),
        true,
        |uri, payload| {
            if uri.ends_with("/running") {
                return Ok(running());
            }
            assert_eq!(uri, "luna://com.webos.applicationManager/closeByAppId");
            closed.push(payload["id"].as_str().unwrap().to_owned());
            Ok(json!({"returnValue":true}))
        },
    )
    .unwrap();
    assert_eq!(closed, ["org.example.video", "netflix"]);
}

#[test]
fn legacy_single_close_never_removes_nonexistent_history_and_reports_refusals() {
    let error = recent_call_with(&Command::Close("netflix".into()), true, |uri, _| {
        assert_eq!(uri, "luna://com.webos.applicationManager/closeByAppId");
        Err("close refused".into())
    })
    .unwrap_err();
    assert_eq!(error, "close refused");
}

#[test]
fn unavailable_running_inventory_stays_an_error_instead_of_demo_or_empty_data() {
    assert!(recent_call_with(&Command::List, true, |_, _| Err("offline".into())).is_err());
}

#[test]
fn modern_history_path_is_preserved() {
    let reply = recent_call_with(&Command::List, false, |uri, _| {
        assert_eq!(uri, "luna://com.webos.service.recent/listRecents");
        Ok(json!({"recentsAppList":[{"appId":"netflix"}]}))
    })
    .unwrap();
    assert_eq!(normalize(&reply).unwrap()[0].app.id, "netflix");
}
