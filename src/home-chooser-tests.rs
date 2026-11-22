use super::*;
use std::cell::Cell;

fn status(version: u64) -> Value {
    json!({"returnValue": true, "apiVersion": version, "root": true,
        "permitted": true, "isSelectedHome": false})
}

#[test]
fn detects_versions_and_allows_reads_before_home_selection() {
    for version in [1, 2, 3, 99] {
        assert_eq!(info(status(version)).unwrap().version, version);
    }
    for reply in [
        json!({}),
        status(0),
        json!({"returnValue": false, "errorCode": "forbidden"}),
    ] {
        assert!(info(reply).is_err());
    }
}

#[test]
fn root_and_permission_setup_errors_are_actionable() {
    let mut reply = status(3);
    reply["root"] = json!(false);
    reply["permitted"] = json!(false);
    assert!(
        info(reply.clone())
            .unwrap_err()
            .contains("Open Home Chooser")
    );
    reply["root"] = json!(true);
    assert!(info(reply).unwrap_err().contains("Reinstall Luma"));
}

#[test]
fn level_two_methods_are_never_sent_to_level_one() {
    let called = Cell::new(false);
    let result = request(
        &info(status(1)).unwrap(),
        "listInputs",
        json!({}),
        2,
        |_, _| {
            called.set(true);
            Ok(json!({"returnValue": true}))
        },
    );
    assert!(result.is_err());
    assert!(!called.get());
}

#[test]
fn errors_do_not_become_successful_empty_inventory_or_retry_mutations() {
    for reply in [
        json!({"returnValue": false, "errorCode": "noPermission", "errorText": "denied"}),
        json!({"returnValue": false, "errorCode": 1, "errorText": "ServiceUnknown"}),
        json!({"apiVersion": 3}),
    ] {
        assert!(
            request(&Info { version: 3 }, "listApps", json!({}), 1, |_, _| Ok(
                reply
            ))
            .is_err()
        );
    }
    assert!(
        request(&Info { version: 3 }, "listApps", json!({}), 1, |_, _| Err(
            "timeout".into()
        ))
        .is_err()
    );
    assert!(array(&json!({"apps": {}}), "apps", 1000).is_err());
    assert!(array(&json!({}), "apps", 1000).is_err());
    assert!(array(&json!({"apps": [1, 2]}), "apps", 1).is_err());
    assert_eq!(
        array(&json!({"apps": []}), "apps", 1000).unwrap(),
        json!([])
    );
}

#[test]
fn chooser_app_fields_feed_existing_normalization_and_versioned_icon_keys() {
    let apps = crate::platform::normalize_apps(&json!([
        {"id": "org.example.app", "title": "App", "version": "1.0.0", "icon": "icon.png"},
        {"id": crate::platform::APP_ID, "appCategory": ["home"]},
        {"id": "com.webos.app.home"},
        {"id": "org.example.hidden", "visible": false}
    ]))
    .unwrap();
    assert_eq!(apps.len(), 1);
    assert_eq!(apps[0].id, "org.example.app");
    assert_ne!(
        apps[0].icon.as_ref().unwrap(),
        &crate::app_artwork::Source::new(
            "org.example.app",
            &json!({"version": "2.0.0", "icon": "icon.png"})
        )
    );
}

#[test]
fn input_fields_preserve_unknown_signal_and_connection() {
    let reply = json!({"inputs": [
        {"id": "com.webos.app.hdmi1", "inputId": "HDMI_1", "port": 1, "label": "Console", "connected": true, "signal": false},
        {"id": "com.webos.app.hdmi2", "inputId": "HDMI_2", "port": 2}
    ]});
    let inputs = crate::platform::normalize_inputs(&input_devices(&reply).unwrap()).unwrap();
    assert_eq!(inputs.len(), 2);
    assert_eq!(inputs[0].title, "Console");
    assert_eq!(inputs[0].connected, Some(true));
    assert_eq!(inputs[0].signal, Some(false));
    assert_eq!(inputs[1].connected, None);
    assert_eq!(inputs[1].signal, None);
    assert!(input_devices(&json!({"inputs": [null]})).is_err());
}

fn icon() -> Value {
    json!({"mimeType": "image/png", "dataUrl": "data:image/png;base64,AQID", "byteLength": 3})
}

#[test]
fn icon_transport_checks_mime_base64_and_exact_lengths() {
    assert_eq!(icon_bytes(&icon()).unwrap(), [1, 2, 3]);
    for (field, value) in [
        ("mimeType", json!("image/svg+xml")),
        ("dataUrl", json!("file:///etc/passwd")),
        ("dataUrl", json!("data:image/png;base64,!!!!")),
        ("dataUrl", json!("data:image/jpeg;base64,AQID")),
        ("byteLength", json!(2)),
        ("byteLength", json!(crate::app_artwork::MAX_FILE + 1)),
    ] {
        let mut raw = icon();
        raw[field] = value;
        assert!(icon_bytes(&raw).is_err(), "{field}");
    }
}

#[test]
fn icon_batch_is_large_first_and_missing_candidates_are_normal() {
    let result = icons_with("org.example.app", |payload| {
        assert_eq!(payload["items"].as_array().unwrap().len(), 2);
        assert_eq!(payload["items"][0]["size"], "largeIcon");
        assert_eq!(payload["items"][1]["size"], "icon");
        Ok(json!({"icons": [{"errorCode": "noIcon"}, icon()]}))
    })
    .unwrap();
    assert_eq!(result, vec![vec![1, 2, 3]]);
    assert!(icons_with("../bad", |_| panic!("must not submit")).is_err());
}

#[test]
fn budget_exhaustion_retries_only_the_affected_read_once() {
    let mut calls = 0;
    let result = icons_with("org.example.app", |payload| {
        calls += 1;
        if calls == 1 {
            Ok(json!({"icons": [{"errorCode": "noIcon"}, {"errorCode": "overBudget"}]}))
        } else {
            assert_eq!(
                payload["items"],
                json!([{"id": "org.example.app", "size": "icon"}])
            );
            Ok(json!({"icons": [icon()]}))
        }
    })
    .unwrap();
    assert_eq!(calls, 2);
    assert_eq!(result, vec![vec![1, 2, 3]]);
}

#[test]
fn malformed_batches_and_unknown_item_errors_trigger_fallback() {
    for reply in [
        json!({"icons": []}),
        json!({"icons": [{"errorCode": "newError"}, icon()]}),
        json!({"icons": [null, icon()]}),
        json!({"icons": [icon(), icon(), icon()]}),
    ] {
        assert!(icons_with("org.example.app", |_| Ok(reply.clone())).is_err());
    }
    let mut calls = 0;
    assert!(
        icons_with("org.example.app", |_| {
            calls += 1;
            Ok(if calls == 1 {
                json!({"icons": [{"errorCode": "overBudget"}, icon()]})
            } else {
                json!({"icons": [{"errorCode": "overBudget"}]})
            })
        })
        .is_err()
    );
    assert_eq!(calls, 2);
}
