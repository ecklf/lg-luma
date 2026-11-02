//! Runtime-only transformations of the TV's installed input ribbon. LG assets are never bundled.
use crate::{
    Result, accepted,
    system::{self, System},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

pub const ALLOWED_KEYS: [&str; 8] = [
    "home_dashboard",
    "live_tv",
    "HDMI_1",
    "HDMI_2",
    "HDMI_3",
    "HDMI_4",
    "usb",
    "media_server",
];
const STATE: &str = "/var/lib/luma-input-menu";
const SAVED: &str = "/var/lib/luma-input-menu/original-main.json";
const MANAGER: &str = "com.webos.surfacemanager.systemUIManager";
const SETTINGS: &str = "com.ecklf.luma.inputMenu";
const PICKER: &str = "com.webos.app.quickinputpicker";
const CONTROLLER: &str = include_str!("../../tv/input-menu-controller.qml");
const MODEL: &str = include_str!("../../tv/input-menu-model.qml");

#[derive(Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Config {
    #[serde(default)]
    enabled: bool,
    #[serde(default)]
    entries: BTreeMap<String, bool>,
}
fn config(request: &Value) -> Result<Config> {
    let mut cfg: Config = serde_json::from_value(request.clone()).map_err(|e| e.to_string())?;
    if cfg
        .entries
        .keys()
        .any(|key| !ALLOWED_KEYS.contains(&key.as_str()))
    {
        return Err("Unknown input-menu entry".into());
    }
    for key in ALLOWED_KEYS {
        cfg.entries.entry(key.into()).or_insert(true);
    }
    Ok(cfg)
}
fn luna(system: &impl System, method: &str, payload: Value) -> Result<Value> {
    accepted(system::luna(
        system,
        "com.webos.service.config",
        method,
        &payload,
    )?)
}
fn get_manager(system: &impl System) -> Result<Value> {
    let reply = luna(system, "getConfigs", json!({"configNames":[MANAGER]}))?;
    let value = reply["configs"][MANAGER].clone();
    if !value["appinfo"].is_array() {
        return Err("Unsupported system UI inventory".into());
    }
    Ok(value)
}
fn main_index(manager: &Value) -> Result<usize> {
    let rows = manager["appinfo"]
        .as_array()
        .ok_or("Missing UI inventory")?;
    let ids: Vec<_> = rows
        .iter()
        .enumerate()
        .filter(|(_, r)| r["id"] == PICKER)
        .map(|(i, _)| i)
        .collect();
    if ids.len() != 1 {
        return Err("Expected one input ribbon".into());
    }
    Ok(ids[0])
}
fn owned(main: &str) -> bool {
    main.strip_prefix("file:///var/lib/luma-input-menu/revisions/")
        .and_then(|s| s.strip_suffix("/InputPicker.qml"))
        .is_some_and(|id| id.len() == 16 && id.bytes().all(|c| c.is_ascii_hexdigit()))
}
fn stock(main: &str) -> bool {
    matches!(
        main,
        "file:///usr/lib/qt5/qml/InputPicker/InputPicker.qml"
            | "file:///usr/lib/qml/InputPicker/InputPicker.qml"
    )
}
fn set_pointer(system: &impl System, expected: &str, main: &str) -> Result<()> {
    let mut manager = get_manager(system)?;
    let index = main_index(&manager)?;
    if manager["appinfo"][index]["main"] != expected {
        return Err("The input-menu owner changed during configuration".into());
    }
    manager["appinfo"][index]["main"] = main.into();
    luna(system, "setConfigs", json!({"configs":{MANAGER:manager}}))?;
    accepted(system::luna(
        system,
        "com.webos.service.systemUIManager",
        "closeByAppId",
        &json!({"appId":PICKER}),
    )?)?;
    Ok(())
}
pub fn configure(system: &impl System, request: &Value, _directory: &Path) -> Result<Value> {
    system::root(system)?;
    let cfg = config(request)?;
    if !cfg.enabled && !system::exists(system, SAVED) {
        return Ok(json!({"returnValue":true,"enabled":false,"changed":false}));
    }
    let manager = get_manager(system)?;
    let index = main_index(&manager)?;
    let previous = manager["appinfo"][index]["main"]
        .as_str()
        .ok_or("Missing ribbon source")?
        .to_string();
    if !owned(&previous) && !stock(&previous) {
        return Err("Another input-menu customization is active".into());
    }
    if !cfg.enabled {
        if owned(&previous) {
            let saved = system::json(system, SAVED)?;
            let original = saved["main"]
                .as_str()
                .filter(|s| stock(s))
                .ok_or("Invalid original ribbon source")?;
            set_pointer(system, &previous, original)?;
        }
        luna(
            system,
            "setConfigs",
            json!({"configs":{SETTINGS:{"enabled":false,"entries":cfg.entries}}}),
        )?;
        return Ok(json!({"returnValue":true,"enabled":false}));
    }
    if owned(&previous) && !system::exists(system, SAVED) {
        return Err("Missing original ribbon backup".into());
    }
    let source = ["/usr/lib/qml/InputPicker", "/usr/lib/qt5/qml/InputPicker"]
        .into_iter()
        .find(|p| system::exists(system, Path::new(p).join("InputPicker.qml")))
        .ok_or("This firmware has no supported input ribbon")?;
    let main = prepare(system, Path::new(source))?;
    if stock(&previous) {
        system::write_json(system, SAVED, &json!({"main":previous}))?;
    }
    luna(
        system,
        "setConfigs",
        json!({"configs":{SETTINGS:{"enabled":true,"entries":cfg.entries}}}),
    )?;
    if previous != main {
        set_pointer(system, &previous, &main)?;
    }
    Ok(json!({"returnValue":true,"enabled":true}))
}

fn once(source: &str, old: &str, new: &str) -> Result<String> {
    if source.matches(old).count() != 1 {
        return Err(format!("Unsupported input ribbon marker: {old}"));
    }
    Ok(source.replacen(old, new, 1))
}
fn function(source: &str, signature: &str, body: &str) -> Result<String> {
    let start = source
        .find(signature)
        .ok_or_else(|| format!("Missing ribbon function: {signature}"))?;
    if source[start + signature.len()..].contains(signature) {
        return Err("Duplicate ribbon function".into());
    }
    let open = start + source[start..].find('{').ok_or("Missing function body")?;
    let bytes = source.as_bytes();
    let (mut depth, mut quote, mut escaped) = (0usize, 0u8, false);
    let (mut line_comment, mut block_comment) = (false, false);
    let mut i = open;
    while i < bytes.len() {
        let c = bytes[i];
        let next = bytes.get(i + 1).copied().unwrap_or(0);
        if line_comment {
            if c == b'\n' {
                line_comment = false;
            }
        } else if block_comment {
            if c == b'*' && next == b'/' {
                block_comment = false;
                i += 1;
            }
        } else if quote != 0 {
            if escaped {
                escaped = false;
            } else if c == b'\\' {
                escaped = true;
            } else if c == quote {
                quote = 0;
            }
        } else if c == b'/' && next == b'/' {
            line_comment = true;
            i += 1;
        } else if c == b'/' && next == b'*' {
            block_comment = true;
            i += 1;
        } else if matches!(c, b'\'' | b'"') {
            quote = c;
        } else if c == b'{' {
            depth += 1;
        } else if c == b'}' {
            depth -= 1;
            if depth == 0 {
                return Ok(format!(
                    "{}{}{{\n{}\n    }}{}",
                    &source[..start],
                    signature,
                    body,
                    &source[i + 1..]
                ));
            }
        }
        i += 1;
    }
    Err("Unterminated ribbon function".into())
}
pub fn transform(path: &str, original: &str) -> Result<String> {
    match path {
        "InputPicker.qml" => {
            let text = once(
                original,
                "        id: inputPickerRoot",
                &format!("        id: inputPickerRoot\n{CONTROLLER}"),
            )?;
            // Keep the expanded ribbon floating above the screen edge so its lower corners
            // remain visible, like Luma's home input cards.
            once(
                &text,
                "to: -inputPickerRoot.styler.mainRibbonAnimMargin",
                "to: -36",
            )
        }
        "Style.qml" => {
            let text = once(
                original,
                "property color focusColor: isSupportBNO ? \"#68819b\" : \"#cf0652\"",
                "property color focusColor: \"#7DA8FF\"",
            )?;
            let text = once(
                &text,
                "property int mainRibbonHeight : 330",
                "property int mainRibbonHeight : 300",
            )?;
            let text = once(
                &text,
                "property int mainRibbonTileHeight : 210",
                "property int mainRibbonTileHeight : 180",
            )?;
            let text = once(
                &text,
                "property int mainRibbonDefaultTileWidth : 280",
                "property int mainRibbonDefaultTileWidth : mainRibbonWidth",
            )?;
            let text = once(
                &text,
                "property int mainRibbonWidth : 300",
                "property int lumaVisibleCount : 4\n    property int mainRibbonWidth : Math.max(300, Math.min(600, Math.floor((screenWidth - 2 * mainRibbonMargin - mainRibbonSeperatorMargin - (lumaVisibleCount - 1) * mainRibbonTileSpacing) / Math.max(1, lumaVisibleCount))))",
            )?;
            let text = once(
                &text,
                "property color rootBgColor: \"#121212\"",
                "property color rootBgColor: \"#171C22\"",
            )?;
            let text = once(
                &text,
                "property color defaultTileBgColor: \"#181818\"",
                "property color defaultTileBgColor: \"#20252B\"",
            )?;
            let text = once(
                &text,
                "property color tileBgColor: \"#FFFFFF\"",
                "property color tileBgColor: \"#30363C\"",
            )?;
            let text = once(
                &text,
                "property color inactiveTileBgColor: \"#5A5A5A\"",
                "property color inactiveTileBgColor: \"#252B31\"",
            )?;
            let text = once(
                &text,
                "property color tileSelectedMarkColor: \"#cf0652\"",
                "property color tileSelectedMarkColor: \"#7DA8FF\"",
            )?;
            Ok(text)
        }
        "View/InputPickerListItem.qml" => {
            let marker = if original.contains("border.color:  \"#cf0652\"") {
                "border.color:  \"#cf0652\""
            } else {
                "border.color: \"#cf0652\""
            };
            let text = once(
                original,
                marker,
                "border.color: inputPickerRoot.styler.focusColor",
            )?;
            let text = once(&text, "id: bgBorder", "id: bgBorder\n        radius: 18")?;
            let text = once(&text, "id: bg\n", "id: bg\n        radius: 18\n")?;
            once(
                &text,
                "id: focusedMark",
                "id: focusedMark\n        radius: 22",
            )
        }
        "View/InputPickerListItemTile.qml" => {
            let text = once(
                original,
                "color: model.activate ? \"#333333\" : \"#FFFFFF\"",
                "color: \"#F4F6FC\"",
            )?;
            let text = once(
                &text,
                "color: model.activate ? \"#888888\" : \"#FFFFFF\"",
                "color: \"#AEBBD0\"",
            )?;
            let marker = "    ScrollText {\n        id: deviceName";
            once(
                &text,
                marker,
                "    ColorOverlay { anchors.fill: curInputMark; source: curInputMark; color: inputPickerRoot.styler.focusColor; visible: curInputMark.visible }\n\n    ScrollText {\n        id: deviceName",
            )
        }
        "View/InputPickerMainView.qml" => {
            let text = once(
                original,
                "removeAnimation.targetItem = delegate;\n                    removeAnimation.start();",
                "delegate.ListView.delayRemove = false;",
            )?;
            let text = once(
                &text,
                "opacity: inputPickerRoot.isMenuTransparency ? 0.85 : 1",
                "opacity: 0",
            )?;
            let text = once(
                &text,
                "anchors.leftMargin: inputPickerRoot.styler.mainRibbonMargin",
                "anchors.leftMargin: Math.max(inputPickerRoot.styler.mainRibbonMargin, (inputPickerRoot.styler.screenWidth - width) / 2)",
            )?;
            let text = once(
                &text,
                "height: 210",
                "height: inputPickerRoot.styler.mainRibbonTileHeight",
            )?;
            once(
                &text,
                "    CustomRibbon {",
                "    Rectangle { anchors.fill: parent; anchors.leftMargin: 24; anchors.rightMargin: 24; anchors.topMargin: 12; anchors.bottomMargin: 60; radius: 27.2; border.color: \"#18FFFFFF\"; border.width: 1; gradient: Gradient { GradientStop { position: 0.0; color: \"#20252B\" } GradientStop { position: 1.0; color: \"#171C22\" } } }\n\n    CustomRibbon {",
            )
        }
        "Model/InputItemListModel.qml" => {
            let text = once(
                original,
                "    id: inputPickerListModel",
                &format!("    id: inputPickerListModel\n{MODEL}"),
            )?;
            let text = function(
                &text,
                "function createListModel()",
                "        titleItem=inputPickerRoot.inputPickerUtils.titleItemObj;\n        liveTV=inputPickerRoot.inputPickerUtils.fetchLiveTV(inputPickerRoot.props.channelListCount,inputPickerRoot.props.channelMode);\n        lumaReady=true; lumaRebuild(true);",
            )?;
            function(
                &text,
                "function updateListModel()",
                "        if(lumaReady) lumaRebuild(false);",
            )
        }
        _ => Ok(original.into()),
    }
}

fn hash(bytes: &[u8], value: &mut u64) {
    for b in bytes {
        *value ^= u64::from(*b);
        *value = value.wrapping_mul(0x100000001b3);
    }
}
fn gather(
    system: &impl System,
    root: &Path,
    relative: &Path,
    files: &mut Vec<(PathBuf, Vec<u8>)>,
    total: &mut usize,
) -> Result<()> {
    if files.len() > 4096 || relative.components().count() > 12 {
        return Err("Input ribbon tree limit exceeded".into());
    }
    let mut entries = fs::read_dir(system.path(&root.join(relative)))
        .map_err(|e| e.to_string())?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let next = relative.join(entry.file_name());
        let metadata = entry.file_type().map_err(|e| e.to_string())?;
        if metadata.is_dir() {
            gather(system, root, &next, files, total)?;
        } else if metadata.is_file() {
            let mut bytes = system::read(system, root.join(&next), 4 * 1024 * 1024)?;
            if next.extension().is_some_and(|e| e == "qml") {
                let name = next.to_string_lossy().replace('\\', "/");
                bytes = transform(
                    &name,
                    std::str::from_utf8(&bytes).map_err(|e| e.to_string())?,
                )?
                .into_bytes();
            }
            *total += bytes.len();
            if *total > 32 * 1024 * 1024 {
                return Err("Input ribbon byte limit exceeded".into());
            }
            files.push((next, bytes));
        } else {
            return Err("Unsupported input ribbon link or special file".into());
        }
    }
    Ok(())
}
fn prepare(system: &impl System, source: &Path) -> Result<String> {
    let mut files = vec![];
    let mut total = 0;
    gather(system, source, Path::new(""), &mut files, &mut total)?;
    for required in [
        "InputPicker.qml",
        "Style.qml",
        "View/InputPickerListItem.qml",
        "View/InputPickerListItemTile.qml",
        "View/InputPickerMainView.qml",
        "Model/InputItemListModel.qml",
    ] {
        if !files.iter().any(|(p, _)| p == Path::new(required)) {
            return Err("Incomplete input ribbon module".into());
        }
    }
    let mut digest = 0xcbf29ce484222325u64;
    for (path, bytes) in &files {
        hash(path.to_string_lossy().as_bytes(), &mut digest);
        hash(bytes, &mut digest);
    }
    let directory = format!("{STATE}/revisions/{digest:016x}");
    let main = format!("file://{directory}/InputPicker.qml");
    if !system::exists(system, format!("{directory}/.complete")) {
        for (path, bytes) in files {
            let target = Path::new(&directory).join(path);
            fs::create_dir_all(system.path(target.parent().ok_or("Invalid ribbon path")?))
                .map_err(|e| e.to_string())?;
            system::write(system, target, &bytes, 0o644)?;
        }
        system::write(
            system,
            format!("{directory}/.complete"),
            b"luma-input-menu-v1\n",
            0o644,
        )?;
    }
    Ok(main)
}

#[cfg(test)]
#[path = "input_menu_tests.rs"]
mod tests;
