use super::*;
use crate::system::Output;
use std::{
    cell::RefCell,
    sync::atomic::{AtomicUsize, Ordering},
    time::Duration,
};

struct Fixture {
    root: PathBuf,
    manager: RefCell<Value>,
    settings: RefCell<Value>,
    calls: RefCell<Vec<(String, Value)>>,
}
impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let root = std::env::temp_dir().join(format!(
            "luma-input-menu-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        Self {
            root,
            manager: RefCell::new(
                json!({"otherSetting":17,"appinfo":[{"id":"other.app","main":"file:///other.qml","extra":true},{"id":PICKER,"main":"file:///usr/lib/qt5/qml/InputPicker/InputPicker.qml","defaultWindowType":"overlay"}]}),
            ),
            settings: RefCell::new(Value::Null),
            calls: RefCell::new(vec![]),
        }
    }
    fn seed(&self) {
        for (name, text) in [
            (
                "InputPicker.qml",
                "import WebOSServices 1.0\nItem {\n        id: inputPickerRoot\n        function launchApp() { var test=1; }\n        NumberAnimation { to: -inputPickerRoot.styler.mainRibbonAnimMargin }\n}",
            ),
            (
                "Style.qml",
                "Item { property color focusColor: isSupportBNO ? \"#68819b\" : \"#cf0652\"\nproperty int mainRibbonHeight : 330\nproperty int mainRibbonTileHeight : 210\nproperty int mainRibbonDefaultTileWidth : 280\nproperty int mainRibbonWidth : 300\nproperty color rootBgColor: \"#121212\"\nproperty color defaultTileBgColor: \"#181818\"\nproperty color tileBgColor: \"#FFFFFF\"\nproperty color inactiveTileBgColor: \"#5A5A5A\"\nproperty color tileSelectedMarkColor: \"#cf0652\"\n}",
            ),
            (
                "View/InputPickerListItem.qml",
                "Item { id: bgBorder\n} Item { id: bg\n} Item { id: focusedMark\nborder.color:  \"#cf0652\"\n}",
            ),
            (
                "View/InputPickerListItemTile.qml",
                "Item { color: model.activate ? \"#333333\" : \"#FFFFFF\"\ncolor: model.activate ? \"#888888\" : \"#FFFFFF\"\n    ScrollText {\n        id: deviceName\n}}",
            ),
            (
                "View/InputPickerMainView.qml",
                "Item { Rectangle { id: listBackground\n        anchors.fill: parent\n        color: inputPickerRoot.styler.rootBgColor\n        opacity: inputPickerRoot.isMenuTransparency ? 0.85 : 1\n    } ListView.onRemove: ScriptAction { script: { removeAnimation.targetItem = delegate;\n                    removeAnimation.start(); } }\n    CustomRibbon { anchors.leftMargin: inputPickerRoot.styler.mainRibbonMargin\n id: mainRibbon } }",
            ),
            (
                "Model/InputItemListModel.qml",
                "Item {\n    id: inputPickerListModel\nfunction createListModel() { var original=1; }\nfunction updateListModel() { var original=2; }\n}",
            ),
        ] {
            let path = self.path(&Path::new("/usr/lib/qml/InputPicker").join(name));
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text).unwrap();
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
impl System for Fixture {
    fn path(&self, p: &Path) -> PathBuf {
        self.root.join(p.strip_prefix("/").unwrap_or(p))
    }
    fn run(&self, program: &str, args: &[String], _: Duration, _: usize) -> Result<Output> {
        assert_eq!(program, "/usr/bin/luna-send");
        let uri = &args[4];
        let payload: Value = serde_json::from_str(&args[5]).unwrap();
        self.calls.borrow_mut().push((uri.clone(), payload.clone()));
        let result = if uri.ends_with("/getConfigs") {
            json!({"returnValue":true,"configs":{MANAGER:self.manager.borrow().clone()}})
        } else if uri.ends_with("/setConfigs") {
            if let Some(m) = payload["configs"].get(MANAGER) {
                *self.manager.borrow_mut() = m.clone();
            }
            if let Some(m) = payload["configs"].get(SETTINGS) {
                *self.settings.borrow_mut() = m.clone();
            }
            json!({"returnValue":true})
        } else if uri.ends_with("/getAppInfo") {
            json!({"returnValue":false})
        } else {
            json!({"returnValue":true})
        };
        Ok(Output {
            stdout: result.to_string().into_bytes(),
            stderr: vec![],
            success: true,
        })
    }
    fn uid(&self) -> u32 {
        0
    }
    fn pause(&self) {}
    fn terminate(&self, _: u32) -> Result<()> {
        panic!("Input ribbon must never terminate a process")
    }
}
#[test]
fn disabled_unowned_is_a_true_noop_and_unknown_keys_are_rejected() {
    let f = Fixture::new();
    configure(&f, &json!({"enabled":false}), Path::new("/unused")).unwrap();
    assert!(f.calls.borrow().is_empty());
    for key in ["unknown", "airplay", "brazil_tv"] {
        assert!(
            configure(
                &f,
                &json!({"enabled":true,"entries":{key:true}}),
                Path::new("/unused")
            )
            .is_err()
        );
    }
    assert!(f.calls.borrow().is_empty());
}
#[test]
fn apply_preserves_other_ui_config_and_live_updates_do_not_close_the_ribbon() {
    let f = Fixture::new();
    f.seed();
    let original = f.manager.borrow().clone();
    configure(
        &f,
        &json!({"enabled":true,"entries":{"home_dashboard":false,"HDMI_3":false}}),
        Path::new("/unused"),
    )
    .unwrap();
    let main = f.manager.borrow()["appinfo"][1]["main"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(owned(&main));
    assert_eq!(f.manager.borrow()["appinfo"][0], original["appinfo"][0]);
    assert_eq!(f.manager.borrow()["otherSetting"], 17);
    assert_eq!(f.settings.borrow()["entries"]["home_dashboard"], false);
    assert_eq!(f.settings.borrow()["entries"]["usb"], true);
    assert_eq!(f.settings.borrow()["entries"].as_object().unwrap().len(), 8);
    f.calls.borrow_mut().clear();
    configure(
        &f,
        &json!({"enabled":true,"entries":{"HDMI_3":true}}),
        Path::new("/unused"),
    )
    .unwrap();
    assert!(
        !f.calls
            .borrow()
            .iter()
            .any(|(u, _)| u.ends_with("/closeByAppId"))
    );
    assert_eq!(f.manager.borrow()["appinfo"][1]["main"], main);
    configure(&f, &json!({"enabled":false}), Path::new("/unused")).unwrap();
    assert_eq!(*f.manager.borrow(), original);
}
#[test]
fn reenabling_refreshes_the_supported_firmware_restore_target() {
    let f = Fixture::new();
    f.seed();
    configure(&f, &json!({"enabled":true}), Path::new("/unused")).unwrap();
    configure(&f, &json!({"enabled":false}), Path::new("/unused")).unwrap();
    let updated = "file:///usr/lib/qml/InputPicker/InputPicker.qml";
    f.manager.borrow_mut()["appinfo"][1]["main"] = updated.into();
    configure(&f, &json!({"enabled":true}), Path::new("/unused")).unwrap();
    assert_eq!(system::json(&f, SAVED).unwrap()["main"], updated);
    configure(&f, &json!({"enabled":false}), Path::new("/unused")).unwrap();
    assert_eq!(f.manager.borrow()["appinfo"][1]["main"], updated);
}
#[test]
fn foreign_override_is_never_overwritten_on_apply_or_disable() {
    let f = Fixture::new();
    f.manager.borrow_mut()["appinfo"][1]["main"] = "file:///other/custom.qml".into();
    assert!(configure(&f, &json!({"enabled":true}), Path::new("/unused")).is_err());
    assert!(
        f.calls
            .borrow()
            .iter()
            .all(|(u, _)| !u.ends_with("/setConfigs"))
    );
    let saved = f.path(Path::new(SAVED));
    fs::create_dir_all(saved.parent().unwrap()).unwrap();
    fs::write(saved, b"{}").unwrap();
    assert!(configure(&f, &json!({"enabled":false}), Path::new("/unused")).is_err());
}
#[test]
fn invalid_vendor_source_never_changes_the_system_ui_pointer() {
    let f = Fixture::new();
    f.seed();
    fs::write(
        f.path(Path::new("/usr/lib/qml/InputPicker/Style.qml")),
        b"changed vendor structure",
    )
    .unwrap();
    assert!(configure(&f, &json!({"enabled":true}), Path::new("/unused")).is_err());
    assert!(
        f.calls
            .borrow()
            .iter()
            .all(|(u, _)| !u.ends_with("/setConfigs"))
    );
}
#[test]
fn function_replacement_handles_nested_blocks_strings_and_comments() {
    let text =
        "before function example() { var s='}'; /* } */ if(true) { action(); } // }\n} after";
    assert_eq!(
        function(text, "function example()", "return 7;").unwrap(),
        "before function example(){\nreturn 7;\n    } after"
    );
    assert!(function("function example() {", "function example()", "").is_err());
}
#[test]
fn ownership_and_pointer_write_reject_traversal_and_preserve_fresh_inventory() {
    assert!(!owned(
        "file:///var/lib/luma-input-menu/revisions/../foreign/InputPicker.qml"
    ));
    let target = "file:///var/lib/luma-input-menu/revisions/1234567890abcdef/InputPicker.qml";
    assert!(owned(target));
    let f = Fixture::new();
    f.manager.borrow_mut()["otherSetting"] = 99.into();
    set_pointer(
        &f,
        "file:///usr/lib/qt5/qml/InputPicker/InputPicker.qml",
        target,
    )
    .unwrap();
    assert_eq!(f.manager.borrow()["otherSetting"], 99);
    f.manager.borrow_mut()["appinfo"][1]["main"] = "file:///foreign.qml".into();
    f.calls.borrow_mut().clear();
    assert!(
        set_pointer(
            &f,
            target,
            "file:///usr/lib/qt5/qml/InputPicker/InputPicker.qml"
        )
        .is_err()
    );
    assert!(
        f.calls
            .borrow()
            .iter()
            .all(|(u, _)| !u.ends_with("/setConfigs"))
    );
}
#[test]
fn ribbon_rebuild_does_not_retarget_a_shared_removal_animation() {
    let source = "Rectangle { id: listBackground\n        anchors.fill: parent\n        color: inputPickerRoot.styler.rootBgColor\n        opacity: inputPickerRoot.isMenuTransparency ? 0.85 : 1\n    } ListView.onRemove: ScriptAction { script: { removeAnimation.targetItem = delegate;\n                    removeAnimation.start(); } }\n    CustomRibbon { anchors.leftMargin: inputPickerRoot.styler.mainRibbonMargin\n id: mainRibbon }";
    let patched = transform("View/InputPickerMainView.qml", source).unwrap();
    assert!(patched.contains("delegate.ListView.delayRemove = false;"));
    assert!(patched.contains("radius: 27.2"));
    assert!(patched.contains("#20252B"));
    assert!(patched.contains("#171C22"));
    assert!(patched.contains("#18FFFFFF"));
    assert!(patched.contains("anchors.leftMargin: 24"));
    assert!(patched.contains("anchors.rightMargin: 24"));
    assert!(patched.contains("anchors.topMargin: 12"));
    assert!(patched.contains("anchors.bottomMargin: 60"));
    assert!(patched.contains("color: \"#000000\""));
    assert!(patched.contains("opacity: 0.48"));
    assert!(!patched.contains("removeAnimation.start()"));
}
#[test]
fn controller_uses_the_native_service_response_instead_of_global_settings() {
    let source = "import WebOSServices 1.0\nItem {\n        id: inputPickerRoot\n        function launchApp() {}\n        to: -inputPickerRoot.styler.mainRibbonAnimMargin\n}";
    let patched = transform("InputPicker.qml", source).unwrap();
    assert!(patched.contains("property var lumaMenu: ({\"enabled\":false,\"entries\":{}})"));
    assert!(patched.contains("service: \"com.webos.service.config\""));
    assert!(patched.contains("\"subscribe\":true"));
    assert!(patched.contains("onCallSuccess: inputPickerRoot.lumaAcceptSettings(response)"));
    assert!(!patched.contains("Settings.subscribe"));
    assert!(!patched.contains("import WebOS.Global"));
    assert!(!patched.contains("airplay.service"));
    assert!(patched.contains("function launchApp() {}"));
    assert!(patched.contains("to: -36"));
}
#[test]
fn optional_installed_module_fixture_matches_every_transformation() {
    let Ok(root) = std::env::var("LUMA_INPUT_PICKER_FIXTURE") else {
        return;
    };
    for name in [
        "InputPicker.qml",
        "Style.qml",
        "View/InputPickerListItem.qml",
        "View/InputPickerListItemTile.qml",
        "View/InputPickerMainView.qml",
        "Model/InputItemListModel.qml",
    ] {
        let source = fs::read_to_string(Path::new(&root).join(name)).unwrap();
        let patched = transform(name, &source).unwrap();
        assert_ne!(source, patched, "{name}");
    }
}
