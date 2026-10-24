//! Blocking platform work runs on one worker, never in the SDL/UI thread.
//! TV calls originate from this native executable's LS2 connection, not a CLI
//! process trying to impersonate its application identity.
use crate::{
    app_artwork,
    model::{App, Input, Inventory, demo_inventory, valid_app_id},
};
use serde_json::{Value, json};
use std::{collections::HashSet, sync::mpsc, thread};

#[path = "platform-install.rs"]
mod installation;

pub const APP_ID: &str = "com.ecklf.luma";
pub const HOME_ID: &str = "com.webos.app.home";
/// LG's Home executable. A root integration may bind-mount Luma over it, so sam starts Luma
/// under LG's own Home manifest and LS2 role (lifecycle v2, keep-alive, no sam restart).
pub const LG_HOME_EXE: &str = "/usr/bin/com.webos.app.home";

/// Whether this process runs in place of LG's Home executable.
pub fn runs_as_lg_home() -> bool {
    std::env::current_exe().is_ok_and(|exe| exe == std::path::Path::new(LG_HOME_EXE))
}

/// Directory holding assets/, lib/ and splash/: next to the executable, or the installed
/// package when the executable runs in place of LG's Home.
pub fn install_dir() -> Result<std::path::PathBuf, String> {
    static DIRECTORY: std::sync::OnceLock<Result<std::path::PathBuf, String>> =
        std::sync::OnceLock::new();
    DIRECTORY
        .get_or_init(|| {
            let executable = std::env::current_exe().map_err(|error| error.to_string())?;
            installation::find_directory(&executable, installation::directories())
        })
        .clone()
}

pub fn redirect_to_home() -> Result<bool, String> {
    // The root-only integration mounts this marker with its native Home
    // manifest, or writes it to /var/run on webOS 5/6, where it mounts this
    // executable over LG's (a tile's jail sees /usr/bin without that mount).
    // A normal tile launch then opens that same instance instead of starting
    // a second renderer writing the same preferences.
    let marked = |file| std::fs::read_to_string(file).is_ok_and(|marker| marker.trim() == APP_ID);
    if marked("/usr/palm/applications/com.webos.app.home/.luma-home")
        || marked("/var/run/luma-home")
    {
        thread::spawn(|| {
            call(
                "luna://com.webos.applicationManager/launch",
                json!({"id": HOME_ID}),
            )
        })
        .join()
        .map_err(|_| "Home launch worker failed".to_string())??;
        return Ok(true);
    }
    Ok(false)
}

pub enum Request {
    Refresh,
    Launch(String),
    SyncSplash(String),
    Recent {
        generation: u64,
        command: crate::recents::Command,
    },
}
pub enum Response {
    Inventory(Result<(Inventory, Option<String>), String>),
    Launched(Result<(), String>),
    Recent {
        generation: u64,
        command: crate::recents::Command,
        result: Result<Option<Vec<crate::recents::Entry>>, String>,
    },
}

pub struct Platform {
    sender: mpsc::Sender<Request>,
    pub receiver: mpsc::Receiver<Response>,
    pub webos: bool,
    splash_enabled: bool,
}

impl Platform {
    pub fn new(webos: bool, splash_enabled: bool) -> Self {
        let (sender, requests) = mpsc::channel();
        let (responses, receiver) = mpsc::channel();
        thread::spawn(move || {
            let mut splash_background = None;
            let mut desktop_recents = if webos {
                vec![]
            } else {
                crate::recents::demo()
            };
            while let Ok(request) = requests.recv() {
                let response = match request {
                    Request::Recent {
                        generation,
                        command,
                    } => {
                        let result = if webos {
                            recent_call(&command).and_then(|reply| {
                                if command == crate::recents::Command::List {
                                    crate::recents::normalize(&reply).map(Some)
                                } else {
                                    Ok(None)
                                }
                            })
                        } else {
                            // Isolated in-memory history; desktop actions never call a TV.
                            match &command {
                                crate::recents::Command::List => Ok(Some(desktop_recents.clone())),
                                crate::recents::Command::Close(id) => {
                                    desktop_recents.retain(|entry| &entry.app.id != id);
                                    Ok(None)
                                }
                                crate::recents::Command::Launch(_) => Ok(None),
                            }
                        };
                        Response::Recent {
                            generation,
                            command,
                            result,
                        }
                    }
                    Request::Refresh => Response::Inventory(if webos {
                        inventory()
                    } else {
                        Ok((demo_inventory(), None))
                    }),
                    Request::Launch(id) => Response::Launched(if webos {
                        if crate::scroll_preview::is_placeholder(&id) {
                            Err("Scroll preview cards cannot be launched".into())
                        } else if !valid_app_id(&id) {
                            Err("Invalid application identifier".into())
                        } else {
                            call(
                                "luna://com.webos.applicationManager/launch",
                                json!({"id": id}),
                            )
                            .map(|_| ())
                        }
                    } else {
                        Ok(())
                    }),
                    Request::SyncSplash(background) => {
                        if webos
                            && splash_enabled
                            && splash_background.as_ref() != Some(&background)
                        {
                            match sync_splash(&background) {
                                Ok(()) => splash_background = Some(background),
                                Err(error) => eprintln!("Luma splash: {error}"),
                            }
                        }
                        continue;
                    }
                };
                if responses.send(response).is_err() {
                    break;
                }
            }
        });
        Self {
            sender,
            receiver,
            webos,
            splash_enabled,
        }
    }

    pub fn send(&self, request: Request) {
        let _ = self.sender.send(request);
    }

    pub fn sync_splash(&self, background: &str) {
        if self.webos && self.splash_enabled {
            self.send(Request::SyncSplash(background.to_owned()));
        }
    }
}

fn sync_splash(background: &str) -> Result<(), String> {
    let directory = install_dir()?;
    let quote = |value: &str| format!("'{}'", value.replace('\'', "'\\''"));
    let command = format!(
        "/usr/bin/node -e {} {}",
        quote(include_str!("../tv/sync-splash.cjs")),
        quote(&json!({"directory": directory, "background": background}).to_string())
    );
    let envelope = call(
        "luna://org.webosbrew.hbchannel.service/exec",
        json!({"command": command}),
    )?;
    let saved = inventory_payload(&envelope, "background")?;
    if saved.as_str() != Some(background) {
        return Err("Splash background was not updated".into());
    }
    Ok(())
}

fn call(uri: &str, payload: Value) -> Result<Value, String> {
    #[cfg(all(feature = "webos", target_os = "linux", target_arch = "arm"))]
    let response = crate::luna::call(uri, &payload);
    #[cfg(not(all(feature = "webos", target_os = "linux", target_arch = "arm")))]
    let response: Result<Value, String> = {
        let _ = payload;
        Err("Native Luna services are only available in an ARM webOS build".into())
    };
    response.and_then(checked_response).map_err(|error| {
        let error = format!("{uri}: {error}");
        eprintln!("Luma TV service: {error}");
        error
    })
}

fn checked_response(value: Value) -> Result<Value, String> {
    if value.get("returnValue").and_then(Value::as_bool) != Some(true) {
        return Err(value
            .get("errorText")
            .and_then(Value::as_str)
            .unwrap_or("The TV denied this action")
            .chars()
            .take(240)
            .collect());
    }
    Ok(value)
}

fn inventory_read(uri: &str, field: &str) -> Result<Value, String> {
    // Fixed inventory commands use Homebrew's elevated service.
    // Use the same one-reply invocation as the web launcher, without guessing
    // CLI behavior from a firmware-version string.
    let command = format!(
        "if [ \"$(id -u)\" = 0 ]; then /usr/bin/luna-send -n 1 -w 4000 {uri} '{{}}'; else printf '%s\\n' '{{\"returnValue\":false,\"errorText\":\"Homebrew Channel needs root access\"}}'; fi"
    );
    let envelope = call(
        "luna://org.webosbrew.hbchannel.service/exec",
        json!({"command": command}),
    )?;
    inventory_payload(&envelope, field).map_err(|error| {
        let error = format!("{uri}: {error}");
        eprintln!("Luma TV inventory: {error}");
        error
    })
}

pub fn read_app_icons(source: &app_artwork::Source) -> Result<Vec<Vec<u8>>, String> {
    if !cfg!(feature = "webos") {
        return Err("Installed artwork is only read on TV builds".into());
    }
    let quote = |value: &str| format!("'{}'", value.replace('\'', "'\\''"));
    let command = format!(
        "/usr/bin/node -e {} {}",
        quote(include_str!("../tv/read-app-icon.cjs")),
        quote(&serde_json::to_string(source).map_err(|e| e.to_string())?)
    );
    let envelope = call(
        "luna://org.webosbrew.hbchannel.service/exec",
        json!({"command": command}),
    )?;
    let icons = inventory_payload(&envelope, "icons")?;
    let mut remaining = app_artwork::MAX_FILE;
    icons
        .as_array()
        .ok_or("Invalid installed icon reply")?
        .iter()
        .map(|value| {
            let hex = value.as_str().ok_or("Invalid installed icon data")?;
            if hex.len() % 2 != 0 || hex.len() / 2 > remaining {
                return Err("Installed icon reply exceeds its size limit".into());
            }
            remaining -= hex.len() / 2;
            hex.as_bytes()
                .chunks_exact(2)
                .map(|pair| {
                    let digit = |b: u8| {
                        (b as char)
                            .to_digit(16)
                            .ok_or("Invalid installed icon encoding")
                    };
                    Ok((digit(pair[0])? * 16 + digit(pair[1])?) as u8)
                })
                .collect()
        })
        .collect()
}

fn output_excerpt(output: &str) -> String {
    output
        .chars()
        .map(|c| if c.is_whitespace() { ' ' } else { c })
        .filter(|c| !c.is_control())
        .take(320)
        .collect()
}

fn inventory_payload(envelope: &Value, field: &str) -> Result<Value, String> {
    let response = command_reply(envelope)?;
    response
        .get(field)
        .cloned()
        .ok_or_else(|| format!("Missing TV inventory field: {field}"))
}

fn command_reply(envelope: &Value) -> Result<Value, String> {
    let stdout = envelope
        .get("stdoutString")
        .and_then(Value::as_str)
        .unwrap_or("");
    let stderr = envelope
        .get("stderrString")
        .and_then(Value::as_str)
        .unwrap_or("");
    let failure = |reason: &str| {
        format!(
            "{reason}; stdout: {}; stderr: {}",
            output_excerpt(stdout),
            output_excerpt(stderr),
        )
    };
    for key in ["returnCode", "exitCode"] {
        if let Some(code) = envelope.get(key)
            && code != 0
            && code != "0"
        {
            return Err(failure(&format!(
                "Homebrew inventory command failed ({key}={code})"
            )));
        }
    }
    if stdout.trim().is_empty() {
        return Err(failure("Homebrew returned no inventory"));
    }
    if stdout.len() > 2 * 1024 * 1024 {
        return Err("Homebrew inventory exceeds the 2 MiB limit".into());
    }
    // Parse an entire JSON object, including pretty-printed multi-line replies.
    // A stream parser permits CLI timing text after that object; a preamble can
    // precede its opening brace. Never mistake a nested one-line app for a reply.
    let start = stdout
        .find('{')
        .ok_or_else(|| failure("Homebrew inventory contains no JSON object"))?;
    let response = serde_json::Deserializer::from_str(&stdout[start..])
        .into_iter::<Value>()
        .next()
        .ok_or_else(|| failure("Homebrew inventory contains no JSON reply"))?
        .map_err(|error| {
            failure(&format!(
                "Homebrew inventory JSON could not be parsed: {error}"
            ))
        })?;
    if !response.is_object() || !response.get("returnValue").is_some_and(Value::is_boolean) {
        return Err(failure("Homebrew inventory JSON is not a Luna response"));
    }
    checked_response(response)
}

fn recent_call(command: &crate::recents::Command) -> Result<Value, String> {
    let (uri, payload) = command.request()?;
    // The native process uses its own authenticated Homebrew connection, as
    // inventory does. Fixed operations only; no app-ID impersonation or retries
    // of mutations after an ambiguous timeout. LG owns close/keep-alive policy.
    let quote = |s: &str| format!("'{}'", s.replace('\'', "'\\''"));
    let command = format!(
        "if [ \"$(id -u)\" = 0 ]; then /usr/bin/luna-send -n 1 -w 4000 {uri} {}; else printf '%s\\n' '{{\"returnValue\":false,\"errorText\":\"Homebrew Channel needs root access\"}}'; fi",
        quote(&payload.to_string())
    );
    let envelope = call(
        "luna://org.webosbrew.hbchannel.service/exec",
        json!({"command": command}),
    )?;
    command_reply(&envelope)
}

fn inventory() -> Result<(Inventory, Option<String>), String> {
    let apps = normalize_apps(&inventory_read(
        "luna://com.webos.applicationManager/listApps",
        "apps",
    )?)?;
    let inputs = inventory_read("luna://com.webos.service.eim/getAllInputStatus", "devices")
        .and_then(|raw| normalize_inputs(&raw));
    match inputs {
        Ok(inputs) => {
            eprintln!(
                "Luma TV inventory: {} apps, {} inputs",
                apps.len(),
                inputs.len()
            );
            Ok((Inventory { apps, inputs }, None))
        }
        Err(error) => Ok((
            Inventory {
                apps,
                inputs: vec![],
            },
            Some(format!("Inputs could not be refreshed: {error}")),
        )),
    }
}

fn title(value: Option<&str>, fallback: &str) -> String {
    let clean: String = value
        .unwrap_or("")
        .chars()
        .filter(|c| !c.is_control())
        .take(120)
        .collect();
    if clean.trim().is_empty() {
        fallback.into()
    } else {
        clean.trim().into()
    }
}

pub fn normalize_apps(raw: &Value) -> Result<Vec<App>, String> {
    let entries = raw
        .as_array()
        .filter(|a| a.len() <= 1000)
        .ok_or("Invalid app inventory")?;
    let mut seen = HashSet::new();
    let mut apps = Vec::new();
    for entry in entries {
        let Some(id) = entry
            .get("id")
            .and_then(Value::as_str)
            .filter(|id| valid_app_id(id))
        else {
            continue;
        };
        let live = id == "com.webos.app.livetv";
        let visible = entry.get("visible");
        if id == APP_ID
            || id == "com.luma.native"
            || id == "com.luma.launcher"
            || id == "com.luma.launcher.native"
            || id == "com.webos.app.home"
            || id.starts_with("com.webos.app.hdmi")
            || id.starts_with("com.webos.app.externalinput.")
            || (!live && (visible == Some(&json!(false)) || visible == Some(&json!("false"))))
            || (!live
                && ["com.webos.", "com.palm.", "com.lge."]
                    .iter()
                    .any(|prefix| id.starts_with(prefix))
                && visible != Some(&json!(true)))
            || !seen.insert(id)
        {
            continue;
        }
        let mut app = App::new(id, &title(entry.get("title").and_then(Value::as_str), id));
        app.icon = Some(app_artwork::Source::new(id, entry));
        apps.push(app);
    }
    apps.sort_by_key(|a| a.title.to_lowercase());
    Ok(apps)
}

pub fn normalize_inputs(raw: &Value) -> Result<Vec<Input>, String> {
    let entries = raw
        .as_array()
        .filter(|a| a.len() <= 128)
        .ok_or("Invalid input inventory")?;
    let mut inputs = Vec::new();
    for entry in entries {
        let Some(id) = entry.get("appId").and_then(Value::as_str) else {
            continue;
        };
        let Some(number) = id
            .strip_prefix("com.webos.app.hdmi")
            .and_then(|s| s.parse::<u8>().ok())
            .filter(|n| *n > 0 && *n < 100)
        else {
            continue;
        };
        if id != format!("com.webos.app.hdmi{number}")
            || entries
                .iter()
                .filter(|e| e.get("appId").and_then(Value::as_str) == Some(id))
                .count()
                != 1
        {
            continue;
        }
        if entry
            .get("id")
            .is_some_and(|v| v != &format!("HDMI_{number}"))
        {
            continue;
        }
        if entry
            .get("port")
            .is_some_and(|v| v != &json!(number) && v != &number.to_string())
        {
            continue;
        }
        let port = format!("HDMI {number}");
        inputs.push(Input {
            id: id.into(),
            title: title(entry.get("label").and_then(Value::as_str), &port),
            port,
            // Some firmware reports `connected: true` for every existing port;
            // `hdmiPlugIn` is the actual cable state where it is provided.
            connected: entry
                .get("hdmiPlugIn")
                .and_then(Value::as_bool)
                .or_else(|| entry.get("connected").and_then(Value::as_bool)),
        });
    }
    Ok(inputs)
}
