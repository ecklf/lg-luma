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
pub const FULL_HOME_ID: &str = "com.webos.app.fullhome";

pub fn is_home_id(id: &str) -> bool {
    matches!(id, HOME_ID | FULL_HOME_ID)
}

/// Keep the window surface and SAM lifecycle identity aligned with the executed Home app.
fn home_identity(executable: Option<&std::path::Path>, metadata: Option<&Value>) -> &'static str {
    if let Some(executable) = executable {
        if executable == std::path::Path::new("/usr/bin/com.webos.app.fullhome") {
            return FULL_HOME_ID;
        }
        if executable == std::path::Path::new(LG_HOME_EXE) {
            return HOME_ID;
        }
    }
    if metadata.is_some_and(|value| {
        ["nid", "appId"]
            .iter()
            .any(|key| value.get(key).and_then(Value::as_str) == Some(FULL_HOME_ID))
    }) {
        FULL_HOME_ID
    } else {
        HOME_ID
    }
}

/// Derive the identity from SAM's actual launch, never from a saved preference.
pub fn home_id() -> &'static str {
    let executable = std::env::current_exe().ok();
    let metadata = std::env::args()
        .nth(1)
        .and_then(|arg| serde_json::from_str::<Value>(&arg).ok());
    home_identity(executable.as_deref(), metadata.as_ref())
}
/// LG's Home executable. A root integration may bind-mount Luma over it, so sam starts Luma
/// under LG's own Home manifest and LS2 role (lifecycle v2, keep-alive, no sam restart).
pub const LG_HOME_EXE: &str = "/usr/bin/com.webos.app.home";

/// Whether this process runs in place of LG's Home executable.
pub fn runs_as_lg_home() -> bool {
    std::env::current_exe().is_ok_and(|exe| {
        exe == std::path::Path::new(LG_HOME_EXE)
            || exe == std::path::Path::new("/usr/bin/com.webos.app.fullhome")
    })
}

/// While this file exists, a staged Home (webOS 10/11) does not register with sam and is
/// restarted per Home key as before. Luma creates it when a relaunch did not bring it to the
/// front; delete it to try registering again.
#[cfg_attr(
    not(all(feature = "webos", target_os = "linux", target_arch = "arm")),
    allow(dead_code)
)]
pub const HOME_LIFECYCLE_OFF: &str = "/var/lib/luma-home/no-lifecycle";

/// Whether a relaunch actually brought a staged Home (webOS 10/11) to the front. Only the
/// compositor's focus or remote input counts: SDL_ShowWindow emits Shown by itself, so Shown
/// and Restored say nothing about whether the raise reached the screen.
#[derive(Debug, Default)]
pub struct ForegroundCheck {
    deadline: Option<std::time::Instant>,
}

impl ForegroundCheck {
    pub const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(3);

    /// A relaunch arrived. `in_front`: Luma was already focused and visible, so nothing to check.
    pub fn relaunched(&mut self, now: std::time::Instant, in_front: bool) {
        if !in_front {
            self.deadline = Some(now + Self::TIMEOUT);
        }
    }

    /// The compositor focused Luma, or remote input reached it.
    pub fn confirmed(&mut self) {
        self.deadline = None;
    }

    /// Whether a pending relaunch has failed by `now`. Reports each failure once.
    pub fn failed(&mut self, now: std::time::Instant) -> bool {
        let failed = self.deadline.is_some_and(|deadline| now >= deadline);
        if failed {
            self.deadline = None;
        }
        failed
    }
}

/// What sam asks of a registered Home, from one lifecycle reply. Only TV builds register.
#[cfg_attr(
    not(all(feature = "webos", target_os = "linux", target_arch = "arm")),
    allow(dead_code)
)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lifecycle {
    /// Bring the running instance to the front instead of a restart.
    Relaunch,
    /// Exit.
    Close,
}

impl Lifecycle {
    /// Lifecycle v2 (`registerApp`) names the event in `event`; v1 (`registerNativeApp`) in
    /// `message`. Other replies (`registered`, `pause`, keep-alive) need no action.
    #[cfg_attr(
        not(all(feature = "webos", target_os = "linux", target_arch = "arm")),
        allow(dead_code)
    )]
    pub fn from_reply(reply: &Value) -> Option<Self> {
        let name = reply
            .get("event")
            .or_else(|| reply.get("message"))
            .and_then(Value::as_str)?;
        match name {
            "relaunch" => Some(Self::Relaunch),
            "close" => Some(Self::Close),
            _ => None,
        }
    }
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
                json!({"id": std::fs::read_to_string("/var/run/luma-home-target").ok()
                    .filter(|id| is_home_id(id.trim())).map(|id| id.trim().to_owned()).unwrap_or_else(|| HOME_ID.into())}),
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
    RefreshInputs,
    Launch(String),
    Uninstall(String),
    SyncSplash(String),
    Recent {
        generation: u64,
        command: crate::recents::Command,
    },
}
pub enum Response {
    Inventory(Result<(Inventory, Option<String>), String>),
    Inputs(Result<Vec<Input>, String>),
    Launched(Result<(), String>),
    Uninstalled {
        id: String,
        result: Result<(), String>,
    },
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
            let mut desktop_inventory = demo_inventory();
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
                        Ok((desktop_inventory.clone(), None))
                    }),
                    Request::RefreshInputs => Response::Inputs(if webos {
                        input_inventory()
                    } else {
                        Ok(desktop_inventory.inputs.clone())
                    }),
                    Request::Uninstall(id) => {
                        // Validate again at the worker boundary, not just in the UI.
                        let result = uninstall_request(&id).and_then(|(_, _)| {
                            if webos {
                                let quote =
                                    |value: &str| format!("'{}'", value.replace('\'', "'\\''"));
                                let command = format!(
                                    "/usr/bin/node -e {} {}",
                                    quote(include_str!("../tv/uninstall-app.cjs")),
                                    quote(&id)
                                );
                                #[cfg(all(
                                    feature = "webos",
                                    target_os = "linux",
                                    target_arch = "arm"
                                ))]
                                let result = crate::luna::call_with_timeout(
                                    "luna://org.webosbrew.hbchannel.service/exec",
                                    &json!({"command": command}),
                                    std::time::Duration::from_secs(20),
                                );
                                #[cfg(not(all(
                                    feature = "webos",
                                    target_os = "linux",
                                    target_arch = "arm"
                                )))]
                                let result = raw_call(
                                    "luna://org.webosbrew.hbchannel.service/exec",
                                    &json!({"command": command}),
                                );
                                let result = result
                                    .and_then(checked_response)
                                    .and_then(|envelope| command_reply(&envelope))
                                    .map(|_| ());
                                if let Err(error) = result {
                                    // A lost completion reply does not imply the mutation
                                    // failed. Reconcile against complete inventory reads,
                                    // never by issuing the uninstall a second time.
                                    let mut confirmed = false;
                                    for attempt in 0..3 {
                                        if attempt > 0 {
                                            thread::sleep(std::time::Duration::from_millis(500));
                                        }
                                        let check = inventory_read(
                                            "luna://com.webos.applicationManager/listApps",
                                            "apps",
                                        )
                                        .and_then(|apps| inventory_confirms_removal(&apps, &id));
                                        match check {
                                            Ok(true) => {
                                                confirmed = true;
                                                break;
                                            }
                                            Ok(false) => {}
                                            Err(read_error) => {
                                                eprintln!("Luma uninstall inventory: {read_error}");
                                                break;
                                            }
                                        }
                                    }
                                    if confirmed { Ok(()) } else { Err(error) }
                                } else {
                                    Ok(())
                                }
                            } else {
                                desktop_inventory.apps.retain(|app| app.id != id);
                                desktop_recents.retain(|entry| entry.app.id != id);
                                Ok(())
                            }
                        });
                        Response::Uninstalled { id, result }
                    }
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
        if self.webos && self.splash_enabled && !crate::wallpapers::valid_id(background) {
            self.send(Request::SyncSplash(background.to_owned()));
        }
    }
}

/// Quote for the exec service's shell and systemd's ExecStart environment expansion. Shell
/// quoting alone does not protect `$` variables inside the embedded installer script.
/// Doubling dollars works on older TV systemd versions without --expand-environment=no.
/// Leave `%` untouched: systemd-run passes ExecStart over D-Bus, not through unit-file parsing.
fn update_argument(value: &str) -> String {
    let escaped = value.replace('$', "$$");
    format!("'{}'", escaped.replace('\'', "'\\''"))
}

/// Hands a downloaded, verified update to `tv/self-update.sh`, run as root through Homebrew
/// Channel's exec in its own systemd unit: exec ends its children when the call returns, and the
/// install closes Luma.
pub fn start_update(
    release: &crate::update::Release,
    attempt: &str,
    ipk: &std::path::Path,
) -> Result<(), String> {
    let directory = install_dir()?;
    let command = format!(
        "systemd-run --unit=luma-update-{} --collect /bin/sh -c {} self-update {} {} {} {} {}",
        attempt,
        update_argument(include_str!("../tv/self-update.sh")),
        update_argument(&ipk.to_string_lossy()),
        update_argument(&release.sha256),
        update_argument(&release.version),
        update_argument(&directory.to_string_lossy()),
        update_argument(attempt),
    );
    let envelope = call(
        "luna://org.webosbrew.hbchannel.service/exec",
        json!({"command": command}),
    )?;
    // systemd-run prints nothing on stdout once the unit started; only its exit status counts.
    // A transport error can be ambiguous: the worker preserves the IPK and observes the result.
    for key in ["returnCode", "exitCode"] {
        if let Some(code) = envelope.get(key)
            && code != 0
            && code != "0"
        {
            let stderr = envelope.get("stderrString").and_then(Value::as_str);
            return Err(format!(
                "The installer could not be started ({key}={code}): {}",
                output_excerpt(stderr.unwrap_or(""))
            ));
        }
    }
    Ok(())
}

/// Read only the root-owned result; the caller matches its unguessable attempt ID.
pub fn update_result_for(
    attempt: Option<&str>,
) -> Result<Option<crate::update::InstallResult>, String> {
    let file = match attempt {
        Some(id) if id.len() == 32 && id.bytes().all(|b| b.is_ascii_hexdigit()) => {
            format!("result.{id}.json")
        }
        Some(_) => return Err("Invalid update attempt".into()),
        None => "result.json".into(),
    };
    let envelope = call(
        "luna://org.webosbrew.hbchannel.service/exec",
        json!({
            "command": format!("cat /var/lib/luma-update/{file} 2>/dev/null || true")
        }),
    )?;
    let output = envelope
        .get("stdoutString")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim();
    if output.is_empty() {
        return Ok(None);
    }
    serde_json::from_str(output)
        .map(Some)
        .map_err(|e| format!("Invalid installer result: {e}"))
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

/// Protect launcher identities and vendor system namespaces, never titles.
/// Conservatively includes removable vendor apps until firmware provides a
/// reliable cross-version removability flag.
pub fn can_uninstall(id: &str) -> bool {
    valid_app_id(id)
        && !id.starts_with(crate::model::CUSTOM_TILE_PREFIX)
        && !crate::scroll_preview::is_placeholder(id)
        && !["com.webos.", "com.palm.", "com.lge."]
            .iter()
            .any(|prefix| id.starts_with(prefix))
        && !matches!(
            id,
            APP_ID
                | HOME_ID
                | "com.luma.native"
                | "com.luma.launcher"
                | "com.luma.launcher.native"
                | "org.webosbrew.hbchannel"
                | "org.webosbrew.hbchannel.service"
        )
}

pub fn uninstall_request(id: &str) -> Result<(&'static str, Value), String> {
    if !can_uninstall(id) {
        return Err("This application is protected from deletion".into());
    }
    // The elevated helper subscribes to normal remove and waits for status 31.
    // Homebrew's uninstall uses dev/remove, which rejects Content Store apps.
    // Send once: an ambiguous timeout must never automatically retry a mutation.
    Ok((
        "luna://com.webos.appInstallService/remove",
        json!({"id": id, "subscribe": true}),
    ))
}

/// Use the unfiltered inventory: a hidden/system app must not look uninstalled
/// merely because it would be excluded from Luma's visible catalogue.
pub fn inventory_confirms_removal(apps: &Value, id: &str) -> Result<bool, String> {
    let apps = apps
        .as_array()
        .filter(|apps| apps.len() <= 1000)
        .ok_or("Invalid app inventory while confirming deletion")?;
    let mut present = false;
    for app in apps {
        let app_id = app
            .get("id")
            .and_then(Value::as_str)
            .filter(|id| valid_app_id(id))
            .ok_or("Incomplete app inventory while confirming deletion")?;
        present |= app_id == id;
    }
    Ok(!present)
}

fn raw_call(uri: &str, payload: &Value) -> Result<Value, String> {
    #[cfg(all(feature = "webos", target_os = "linux", target_arch = "arm"))]
    let response = crate::luna::call(uri, payload);
    #[cfg(not(all(feature = "webos", target_os = "linux", target_arch = "arm")))]
    let response: Result<Value, String> = {
        let _ = (uri, payload);
        Err("Native Luna services are only available in an ARM webOS build".into())
    };
    response
}

fn call(uri: &str, payload: Value) -> Result<Value, String> {
    raw_call(uri, &payload)
        .and_then(checked_response)
        .map_err(|error| {
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
    // In place of LG's Home, Luma holds LG Home's LS2 role and asks sam/EIM itself: an early
    // root integration can bring this Home up long before Homebrew's service starts (~21 s vs
    // ~51 s into a cold boot). A refused call answers at once and falls back below.
    if runs_as_lg_home()
        && let Ok(response) = call(uri, json!({}))
        && let Some(value) = response.get(field)
    {
        return Ok(value.clone());
    }
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
    let inputs = input_inventory();
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

fn input_inventory() -> Result<Vec<Input>, String> {
    inventory_read("luna://com.webos.service.eim/getAllInputStatus", "devices")
        .and_then(|raw| normalize_inputs(&raw))
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
            || is_home_id(id)
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
            signal: entry.get("hdmiSignalExist").and_then(Value::as_bool),
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

#[cfg(test)]
mod update_argument_tests {
    use super::*;

    #[test]
    fn installer_variables_are_escaped_and_printf_formats_are_preserved() {
        let script = include_str!("../tv/self-update.sh");
        let argument = update_argument(script);
        assert!(argument.contains("$${#ATTEMPT}"));
        assert!(argument.contains("$$(date +%T)"));
        assert!(argument.contains("$$IPK"));
        assert!(argument.contains("%s"));
        assert!(!argument.contains("%%s"));
    }

    #[test]
    fn exec_shell_preserves_systemd_escapes_and_argument_boundaries() {
        for value in [
            include_str!("../tv/self-update.sh"),
            "path with 'quotes' and $VARIABLE ${BRACES} $(command) %n %% \\slashes",
        ] {
            let output = std::process::Command::new("/bin/sh")
                .args(["-c", &format!("printf '%s' {}", update_argument(value))])
                .output()
                .unwrap();
            assert!(output.status.success());
            let argument = String::from_utf8(output.stdout).unwrap();
            assert_eq!(argument, value.replace('$', "$$"));
            // systemd reduces each escaped dollar pair, leaving expansion to /bin/sh.
            assert_eq!(argument.replace("$$", "$"), value);
        }
    }
}

#[cfg(test)]
mod home_identity_tests {
    use super::*;
    #[test]
    fn executable_identity_has_priority_over_conflicting_launch_metadata() {
        assert_eq!(
            home_identity(
                Some(std::path::Path::new(LG_HOME_EXE)),
                Some(&json!({"appId": FULL_HOME_ID}))
            ),
            HOME_ID
        );
        assert_eq!(
            home_identity(
                Some(std::path::Path::new("/usr/bin/com.webos.app.fullhome")),
                Some(&json!({"nid": HOME_ID}))
            ),
            FULL_HOME_ID
        );
        assert_eq!(
            home_identity(None, Some(&json!({"appId": FULL_HOME_ID}))),
            FULL_HOME_ID
        );
        assert_eq!(
            home_identity(None, Some(&json!({"nid": FULL_HOME_ID}))),
            FULL_HOME_ID
        );
        assert_eq!(
            home_identity(None, Some(&json!({"nid": "other.app"}))),
            HOME_ID
        );
        assert_eq!(home_identity(None, None), HOME_ID);
    }
    #[test]
    fn both_home_variants_are_hidden_from_application_inventory() {
        let raw = json!([{"id":HOME_ID,"title":"Home","visible":true}, {"id":FULL_HOME_ID,"title":"Home","visible":true}]);
        assert!(normalize_apps(&raw).unwrap().is_empty());
    }
}
