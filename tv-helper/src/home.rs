use crate::{
    APP, Result, accepted, files,
    system::{self, System},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs, path::Path};

const STATE: &str = "/var/lib/luma-home";
const CONFIG: &str = "/var/lib/luma-home/state.json";
const STAGE: &str = "/var/lib/luma-home/app";
const HOOK: &str = "/var/lib/webosbrew/init.d/64-luma-home";
const MARKER: &str = "/var/run/luma-home";
pub const HOME: &str = "com.webos.app.home";
pub const FULL_HOME: &str = "com.webos.app.fullhome";
// Early cold-boot start for the executable method (src/bin/luma-early.rs, home-filter.js). ls-hubd
// reads these volatile LS2 directories at core-boot-done and starts dynamic services as root, so
// the compositor key filter can have Luma mounted ~30 s before the Homebrew hook runs.
pub const EARLY: &str = "com.webos.service.lumahome";
// A copy beside home.sh: ls-hubd treats programs under /media/developer as developer-mode
// services and does not deliver the system key filter's calls to them.
const EARLY_EXE: &str = "/var/lib/luma-home/luma-early";
const LS2: &str = "/var/luna-service2";
pub const FILTER: &str = "/var/lib/luma-home/home-filter.js";
const KEY_FILTERS: &str = "com.webos.surfacemanager.keyFilters";
const FILTER_HANDLER: &str = "lumaHomeKey";
const FILTER_TARGET: &str = "var lumaHomeTarget = 'com.webos.app.home';";

/// The LS2 definitions of luma-early: dynamic service, role bound to its executable, the one
/// method in API group "all" (the only group LG grants com.webos.starfish.keyfilters), callable
/// by the key filter alone.
pub fn early_files() -> BTreeMap<String, String> {
    let service = format!("{LS2}/services.d/{EARLY}.service");
    let role = format!("{LS2}/roles.d/{EARLY}.json");
    let api = format!("{LS2}/api-permissions.d/{EARLY}.json");
    let manifest = format!("{LS2}/manifests.d/{EARLY}.json");
    let line = |value: Value| format!("{value}\n");
    BTreeMap::from([
        (
            service.clone(),
            format!("[D-BUS Service]\nName={EARLY}\nExec={EARLY_EXE}\nType=dynamic\n"),
        ),
        (
            role.clone(),
            line(json!({
                "exeName": EARLY_EXE,
                "type": "regular",
                "allowedNames": [EARLY],
                "permissions": [{"service": EARLY, "inbound": ["com.webos.starfish.keyfilters"], "outbound": []}],
            })),
        ),
        (
            api.clone(),
            line(json!({"all": [format!("{EARLY}/prepare")]})),
        ),
        (
            manifest,
            line(json!({
                "id": EARLY,
                // ls-hubd silently drops a manifest whose version is not x.y.z.
                "version": "1.0.0",
                "serviceFiles": [service],
                "roleFiles": [role],
                "apiPermissionFiles": [api],
            })),
        ),
    ])
}

/// LG's filter chain with ours removed, and appended again when `on`. Keys pass through it.
pub fn key_filters(list: &[Value], on: bool) -> Vec<Value> {
    let mut rest: Vec<Value> = list
        .iter()
        .filter(|entry| entry["file"] != FILTER)
        .cloned()
        .collect();
    if on {
        rest.push(json!({"file": FILTER, "handler": FILTER_HANDLER}));
    }
    rest
}

/// The installed key filter launches the selected Home target.
pub fn filter_source(source: &str, id: &str) -> Result<String> {
    target(id)?;
    if source.matches(FILTER_TARGET).count() != 1 {
        return Err("home-filter.js has no Home target line".into());
    }
    Ok(source.replace(FILTER_TARGET, &format!("var lumaHomeTarget = '{id}';")))
}
fn default_home() -> String {
    HOME.into()
}
fn default_method() -> String {
    "manifest".into()
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    pub enabled: bool,
    #[serde(default = "default_method")]
    pub method: String,
    pub app_dir: String,
    pub stock: Value,
    #[serde(default = "default_home")]
    pub home_id: String,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}
impl Config {
    fn validate(&self) -> Result<()> {
        if !matches!(self.method.as_str(), "manifest" | "executable") {
            return Err("Invalid Home method".into());
        }
        target(&self.home_id)?;
        if !Path::new(&self.app_dir).is_absolute()
            || Path::new(&self.app_dir)
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            return Err("Invalid installed directory".into());
        }
        if self.stock["id"] != self.home_id {
            return Err("Saved stock Home identity does not match target".into());
        }
        Ok(())
    }
}
fn target(id: &str) -> Result<()> {
    if matches!(id, HOME | FULL_HOME) {
        Ok(())
    } else {
        Err("Unsupported Home target".into())
    }
}
pub struct Home<'a, S: System> {
    system: &'a S,
    id: String,
    target: String,
    exe: String,
}
impl<'a, S: System> Home<'a, S> {
    pub fn new(system: &'a S, id: &str) -> Result<Self> {
        target(id)?;
        Ok(Self {
            system,
            id: id.into(),
            target: format!("/usr/palm/applications/{id}"),
            exe: format!("/usr/bin/{id}"),
        })
    }
    fn read(&self, path: impl AsRef<Path>) -> Result<Vec<u8>> {
        system::read(self.system, path, 2 * 1024 * 1024)
    }
    fn exists(&self, path: impl AsRef<Path>) -> bool {
        system::exists(self.system, path)
    }
    fn luna(&self, method: &str, payload: Value) -> Result<Value> {
        system::luna(
            self.system,
            "com.webos.applicationManager",
            method,
            &payload,
        )
    }
    fn foreground(&self) -> Result<String> {
        accepted(self.luna("getForegroundAppInfo", json!({}))?)?["appId"]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| "Invalid foreground app".into())
    }
    fn mounted(&self) -> bool {
        system::same_file(self.system, STAGE, &self.target)
    }
    fn mounts(&self) -> Result<Vec<(String, String)>> {
        Ok(String::from_utf8(self.read("/proc/self/mountinfo")?)
            .map_err(|e| e.to_string())?
            .lines()
            .filter_map(|line| {
                let fields: Vec<_> = line.split_whitespace().collect();
                (fields.len() > 4).then(|| (unescape(fields[3]), unescape(fields[4])))
            })
            .collect())
    }
    fn foreign_mount(&self) -> Result<bool> {
        Ok(!self.mounted()
            && self
                .mounts()?
                .iter()
                .any(|(_, p)| p == &self.target || p.starts_with(&format!("{}/", self.target))))
    }
    fn exe_mount(&self) -> Result<Option<String>> {
        Ok(self
            .mounts()?
            .into_iter()
            .filter(|(_, p)| p == &self.exe)
            .map(|(root, _)| root)
            .next_back())
    }
    fn exe_mounted(&self, app: &str) -> bool {
        system::same_file(self.system, Path::new(app).join("luma"), &self.exe)
    }
    fn owned_mount(&self) -> Result<Option<String>> {
        let root = self.exe_mount()?;
        if root.as_ref().is_some_and(|root| !exe_owned(root)) {
            return Err("Another integration is mounted over LG Home".into());
        }
        Ok(root)
    }
    fn unmount(&self, path: &str) -> Result<()> {
        if let Err(error) = system::command(self.system, "/bin/umount", &[path]) {
            eprintln!("Luma Home: {error}; detaching mount");
            system::command(self.system, "/bin/umount", &["-l", path])?;
        }
        Ok(())
    }
    fn stop_home(&self, app: &str) -> Result<()> {
        let mut stopped = vec![];
        for entry in fs::read_dir(self.system.path(Path::new("/proc")))
            .map_err(|e| e.to_string())?
            .flatten()
        {
            let Some(pid) = entry
                .file_name()
                .to_str()
                .and_then(|s| s.parse::<u32>().ok())
                .filter(|pid| *pid > 1 && *pid <= i32::MAX as u32)
            else {
                continue;
            };
            let proc = format!("/proc/{pid}");
            let Ok(bytes) = self.read(format!("{proc}/cmdline")) else {
                continue;
            };
            let args: Vec<_> = bytes
                .split(|b| *b == 0)
                .map(|b| String::from_utf8_lossy(b).into_owned())
                .collect();
            let executable =
                fs::read_link(self.system.path(Path::new(&format!("{proc}/exe")))).ok();
            if home_process(&args, executable.as_deref(), app, &self.id, self.system) {
                self.system.terminate(pid)?;
                stopped.push(proc);
            }
        }
        for _ in 0..5 {
            if stopped.iter().all(|p| !self.exists(p)) {
                return Ok(());
            }
            self.system.pause();
        }
        Err("The previous Home process has not stopped".into())
    }
    fn launch(&self) -> Result<()> {
        accepted(self.luna("launch", json!({"id":self.id}))?).map(|_| ())
    }
    fn open_executable(&self) -> Result<()> {
        for i in 0..15 {
            if (i == 0 || i == 6)
                && let Err(error) = self.launch()
            {
                eprintln!("Luma Home: {error}");
            }
            if self.foreground()? == self.id {
                return Ok(());
            }
            self.system.pause();
        }
        Err("Home did not come to the front".into())
    }
    fn release(&self) -> Result<()> {
        let mut last = String::new();
        for _ in 0..3 {
            match self.luna("releaseBlockSystemApp", json!({"id":self.id})) {
                Ok(reply) => {
                    let text = reply["errorText"]
                        .as_str()
                        .unwrap_or("")
                        .to_ascii_lowercase();
                    if reply["returnValue"] == true
                        || (text.contains("not") && text.contains("blocked"))
                    {
                        return Ok(());
                    }
                    last = reply.to_string();
                }
                Err(error) => last = error,
            }
            self.system.pause();
        }
        Err(format!("Could not release Home: {last}"))
    }
    fn reread(&self, expected: &Value) -> Result<()> {
        let mut payload = json!({"id":self.id});
        if self
            .read("/etc/palm/schemas/sam/applicationManager.blockSystemApp.schema")
            .is_ok_and(|bytes| String::from_utf8_lossy(&bytes).contains("\"forceBlockSystemApp\""))
        {
            payload["forceBlockSystemApp"] = json!(false);
        }
        // Release even if the request timed out after taking effect; preserve the original error.
        let blocked = self.luna("blockSystemApp", payload).and_then(|reply| {
            let text = reply["errorText"]
                .as_str()
                .unwrap_or("")
                .to_ascii_lowercase();
            if text.contains("already") && text.contains("blocked") {
                Ok(reply)
            } else {
                accepted(reply)
            }
        });
        let released = self.release();
        match (blocked, released) {
            (Err(block), Err(release)) => {
                return Err(format!("{block}; release also failed: {release}"));
            }
            (Err(error), _) | (_, Err(error)) => return Err(error),
            _ => {}
        }
        for _ in 0..15 {
            let reply = accepted(self.luna("getAppInfo", json!({"id":self.id}))?)?;
            let info = &reply["appInfo"];
            let main = expected["main"].as_str();
            if info.is_object()
                && info["type"] == expected["type"]
                && main.is_none_or(|main| {
                    Path::new(info["main"].as_str().unwrap_or("")).file_name()
                        == Path::new(main).file_name()
                })
                && (main != Some("luma")
                    || info["appCategory"]
                        .as_array()
                        .is_some_and(|a| a.iter().any(|v| v == "home")))
            {
                return Ok(());
            }
            self.system.pause();
        }
        Err("SAM did not read the expected Home manifest".into())
    }
    fn normal_running(&self) -> Result<bool> {
        let reply = accepted(self.luna("running", json!({}))?)?;
        Ok(reply["running"]
            .as_array()
            .is_some_and(|apps| apps.iter().any(|app| app["id"] == APP)))
    }
    fn close_normal(&self) -> Result<()> {
        if !self.normal_running()? {
            return Ok(());
        }
        if let Err(error) = self
            .luna("closeByAppId", json!({"id":APP}))
            .and_then(accepted)
        {
            if self.normal_running()? {
                return Err(error);
            }
            return Ok(());
        }
        for i in 0..=5 {
            if !self.normal_running()? {
                return Ok(());
            }
            if i < 5 {
                self.system.pause();
            }
        }
        Err("Close Luma before enabling Home integration".into())
    }
    fn set_key_filters(&self, on: bool) -> Result<()> {
        let reply = accepted(system::luna(
            self.system,
            "com.webos.service.config",
            "getConfigs",
            &json!({"configNames": [KEY_FILTERS]}),
        )?)
        .map_err(|e| format!("Read key filters: {e}"))?;
        let list = reply["configs"][KEY_FILTERS]
            .as_array()
            .ok_or_else(|| format!("Missing {KEY_FILTERS}"))?;
        let next = key_filters(list, on);
        if &next == list {
            return Ok(());
        }
        accepted(system::luna(
            self.system,
            "com.webos.service.config",
            "setConfigs",
            &json!({"configs": {KEY_FILTERS: next}}),
        )?)
        .map_err(|e| format!("Write key filters: {e}"))?;
        Ok(())
    }
    /// Best effort: without it a cold boot still reaches Luma through the Homebrew hook.
    fn install_early(&self, app: &Path) {
        let early = app.join("luma-early");
        if !self.exists(&early) {
            return;
        }
        let result = (|| {
            let bytes = system::read(self.system, &early, 16 * 1024 * 1024)?;
            system::write(self.system, EARLY_EXE, &bytes, 0o755)?;
            system::mkdir(self.system, LS2)?;
            for (file, contents) in early_files() {
                if let Some(parent) = Path::new(&file).parent() {
                    system::mkdir(self.system, parent)?;
                }
                system::write(self.system, &file, contents.as_bytes(), 0o644)?;
            }
            system::command(self.system, "/usr/sbin/ls-control", &["scan-volatile-dirs"])?;
            self.set_key_filters(false)?;
            self.set_key_filters(true)
        })();
        if let Err(error) = result {
            eprintln!("Luma Home: early start not installed: {error}");
        }
    }
    /// Best effort as well: a failed step is reported and must not keep disable from restoring
    /// LG Home.
    fn remove_early(&self) {
        let attempt = |result: Result<()>| {
            if let Err(error) = result {
                eprintln!("Luma Home: early start cleanup: {error}");
            }
        };
        attempt(self.set_key_filters(false));
        let mut changed = false;
        for file in early_files().into_keys() {
            if self.exists(&file) {
                attempt(system::remove(self.system, &file));
                changed = true;
            }
        }
        attempt(system::remove(self.system, EARLY_EXE));
        if changed {
            attempt(system::command(
                self.system,
                "/usr/sbin/ls-control",
                &["scan-volatile-dirs"],
            ));
        }
    }
    /// Disable keeps going after cleanup failures so LG Home is restored and the hook removed.
    pub fn disable(&self, config: &mut Config) -> Result<()> {
        config.enabled = false;
        system::write_json(self.system, CONFIG, &*config)?;
        if config.method == "executable" {
            self.remove_early();
        }
        let restored = self.restore(config);
        if hook_owned(self.system) {
            system::remove(self.system, HOOK)?;
        }
        restored
    }
    pub fn restore(&self, config: &Config) -> Result<()> {
        let was_home = match self.foreground() {
            Ok(id) => id == self.id,
            Err(e) => {
                eprintln!("Luma Home: {e}; restoring files first");
                false
            }
        };
        if config.method == "executable" {
            let root = self.owned_mount()?;
            system::remove(self.system, MARKER)?;
            system::remove(self.system, format!("{MARKER}-target"))?;
            if root.is_some() {
                self.unmount(&self.exe)?;
                if self.exe_mount()?.is_some() {
                    return Err("The Home bind mount is still active".into());
                }
            }
        } else {
            if self.foreign_mount()? {
                return Err("Another integration is mounted over LG Home".into());
            }
            if self.mounted() {
                self.unmount(&self.target)?;
                if self.mounted() {
                    return Err("The Home bind mount is still active".into());
                }
            }
            self.reread(&system::json(
                self.system,
                format!("{}/appinfo.json", self.target),
            )?)?;
        }
        self.stop_home(&config.app_dir)?;
        if was_home {
            self.launch()?;
        }
        Ok(())
    }
    pub fn apply(&self, config: &Config, open: bool) -> Result<()> {
        if !self.exists(Path::new(&config.app_dir).join("luma")) {
            return Err("Luma is no longer installed".into());
        }
        let root = if config.method == "executable" {
            self.owned_mount()?
        } else {
            if self.foreign_mount()? {
                return Err("Another integration is mounted over LG Home".into());
            }
            None
        };
        if config.method != "executable"
            && let Err(e) = files::stage_splash(self.system, Path::new(&config.app_dir))
        {
            eprintln!("Luma Home: splash not staged: {e}");
        }
        let front = self.foreground()?;
        let result = (|| {
            if config.method == "executable" {
                if root.is_some() && !self.exe_mounted(&config.app_dir) {
                    self.unmount(&self.exe)?;
                }
                if !self.exe_mounted(&config.app_dir) {
                    system::command(
                        self.system,
                        "/bin/mount",
                        &["--bind", &format!("{}/luma", config.app_dir), &self.exe],
                    )?;
                }
                if !self.exe_mounted(&config.app_dir) {
                    return Err("The Home bind mount did not take".into());
                }
                system::write(
                    self.system,
                    format!("{MARKER}-target"),
                    format!("{}\n", self.id).as_bytes(),
                    0o644,
                )?;
                system::write(self.system, MARKER, format!("{APP}\n").as_bytes(), 0o644)?;
            } else {
                if !self.mounted() {
                    system::command(self.system, "/bin/mount", &["--bind", STAGE, &self.target])?;
                }
                if !self.mounted() {
                    return Err("The Home bind mount did not take".into());
                }
                self.reread(&system::json(self.system, format!("{STAGE}/appinfo.json"))?)?;
            }
            self.stop_home(&config.app_dir)?;
            if open || (front == self.id && self.foreground()? == self.id) {
                if config.method == "executable" {
                    self.open_executable()?;
                } else {
                    self.launch()?;
                }
            }
            Ok(())
        })();
        if let Err(error) = result {
            return match self.restore(config) {
                Ok(()) => Err(error),
                Err(recovery) => Err(format!("{error}\nRestore also failed: {recovery}")),
            };
        }
        Ok(())
    }
    fn install_scripts(&self, app: &Path) -> Result<()> {
        // The copied native helper lets disable/recovery work after an app package was removed.
        for (name, mode) in [("home.sh", 0o755), ("luma-helper", 0o755)] {
            let bytes = system::read(self.system, app.join(name), 16 * 1024 * 1024)?;
            system::write(self.system, format!("{STATE}/{name}"), &bytes, mode)?;
        }
        // The compositor key filter that opens Home early in a cold boot.
        let source = String::from_utf8(system::read(
            self.system,
            app.join("home-filter.js"),
            1024 * 1024,
        )?)
        .map_err(|e| e.to_string())?;
        let filter = filter_source(&source, &self.id)?;
        system::write(self.system, FILTER, filter.as_bytes(), 0o644)?;
        // Retire the former interpreter script only after both replacement files are available.
        system::remove(self.system, format!("{STATE}/home.cjs"))?;
        Ok(())
    }
    pub fn install(&self, app: &Path) -> Result<()> {
        let physical = fs::canonicalize(self.system.path(app)).map_err(|e| e.to_string())?;
        let base = self.system.path(Path::new("/"));
        let app = if base == Path::new("/") {
            physical
        } else {
            Path::new("/").join(
                physical
                    .strip_prefix(base)
                    .map_err(|_| "Installation escaped test root")?,
            )
        };
        let mut manifest = system::json(self.system, app.join("appinfo.json"))?;
        if manifest["id"] != APP || manifest["type"] != "native" || manifest["main"] != "luma" {
            return Err("Enable this from an installed native Luma package".into());
        }
        let previous = load(self.system)?;
        if previous
            .as_ref()
            .is_some_and(|c| c.enabled && c.home_id != self.id)
        {
            return Err("Disable the existing Home integration before changing its target".into());
        }
        let stock = if self.mounted() {
            previous
                .as_ref()
                .map(|c| c.stock.clone())
                .ok_or("Missing saved stock manifest")?
        } else {
            system::json(self.system, format!("{}/appinfo.json", self.target))?
        };
        let release =
            String::from_utf8(self.read("/etc/starfish-release")?).map_err(|e| e.to_string())?;
        let method = method(&release, &stock, &self.id)?;
        if self.foreign_mount()? {
            return Err("Another integration is mounted over LG Home".into());
        }
        self.owned_mount()?;
        if self.exists(HOOK) && !hook_owned(self.system) {
            return Err("64-luma-home is already in use".into());
        }
        if self.exists("/var/lib/webosbrew/init.d/65-luma") {
            return Err(
                "Disable the standalone cold-boot hook with autostart.sh disable first".into(),
            );
        }
        if stock["id"] != self.id || stock["main"] == "luma" {
            return Err("Could not identify the original LG Home manifest".into());
        }
        self.close_normal()?;
        if method == "manifest" {
            system::mkdir(self.system, STAGE)?;
            let mut links = vec![
                "luma".to_owned(),
                "assets".into(),
                "lib".into(),
                "splash".into(),
            ];
            for key in ["icon", "largeIcon"] {
                if let Some(name) = manifest[key].as_str() {
                    if Path::new(name).is_absolute()
                        || Path::new(name)
                            .components()
                            .any(|c| matches!(c, std::path::Component::ParentDir))
                    {
                        return Err("Invalid package artwork path".into());
                    }
                    links.push(name.into());
                }
            }
            links.sort();
            links.dedup();
            for name in links {
                let link = Path::new(STAGE).join(&name);
                let tmp = link.with_file_name(format!(
                    "{}.new",
                    link.file_name()
                        .ok_or("Invalid stage link")?
                        .to_string_lossy()
                ));
                system::remove(self.system, &tmp)?;
                system::link(self.system, app.join(&name), &tmp)?;
                fs::rename(self.system.path(&tmp), self.system.path(&link))
                    .map_err(|e| e.to_string())?;
            }
            manifest["id"] = json!(self.id);
            manifest["title"] = json!("Home");
            manifest["visible"] = json!(false);
            manifest["appCategory"] = json!(["home"]);
            system::write_json(self.system, format!("{STAGE}/appinfo.json"), &manifest)?;
            system::write(
                self.system,
                format!("{STAGE}/.luma-home"),
                format!("{APP}\n").as_bytes(),
                0o644,
            )?;
        }
        self.install_scripts(&app)?;
        let mut config = Config {
            enabled: false,
            method: method.into(),
            app_dir: app.to_string_lossy().into_owned(),
            stock,
            home_id: self.id.clone(),
            extra: previous.map_or_else(BTreeMap::new, |c| c.extra),
        };
        system::write_json(self.system, CONFIG, &config)?;
        system::mkdir(self.system, "/var/lib/webosbrew")?;
        system::mkdir(self.system, "/var/lib/webosbrew/init.d")?;
        if !hook_owned(self.system) {
            system::link(self.system, format!("{STATE}/home.sh"), HOOK)?;
        }
        self.apply(&config, true)?;
        config.enabled = true;
        system::write_json(self.system, CONFIG, &config)?;
        if config.method == "executable" {
            self.install_early(&app);
        }
        Ok(())
    }
}
fn unescape(s: &str) -> String {
    s.replace("\\040", " ")
        .replace("\\011", "\t")
        .replace("\\012", "\n")
        .replace("\\134", "\\")
}
pub fn exe_owned(root: &str) -> bool {
    root.strip_suffix("//deleted")
        .unwrap_or(root)
        .ends_with("/com.ecklf.luma/luma")
}
pub fn home_process(
    args: &[String],
    executable: Option<&Path>,
    app: &str,
    id: &str,
    system: &impl System,
) -> bool {
    let main = args.first().map(String::as_str).unwrap_or("");
    let native = main == format!("/usr/bin/{id}");
    let flutter = Path::new(main)
        .file_name()
        .is_some_and(|name| name == "flutter-client")
        && args.windows(2).any(|pair| pair[0] == "-i" && pair[1] == id);
    let luma = executable.is_some_and(|path| {
        Path::new(path.to_str().unwrap_or("").trim_end_matches(" (deleted)"))
            == system.path(&Path::new(app).join("luma"))
    });
    let metadata = args
        .get(1)
        .and_then(|arg| serde_json::from_str::<Value>(arg).ok());
    native || flutter || (luma && metadata.is_some_and(|m| m["nid"] == id || m["appId"] == id))
}
pub fn method<'a>(release: &str, stock: &Value, id: &str) -> Result<&'a str> {
    target(id)?;
    let version = release
        .split("release")
        .nth(1)
        .and_then(|s| s.trim_start().split('.').next())
        .and_then(|s| s.parse::<u32>().ok());
    if matches!(version, Some(10 | 11)) && id == HOME {
        return Ok("manifest");
    }
    if matches!(version, Some(5 | 6))
        && stock["type"] == "native"
        && stock["main"] == format!("file:///usr/bin/{id}")
        && stock["nativeLifeCycleInterfaceVersion"] == 2
    {
        return Ok("executable");
    }
    Err("This Home integration currently targets webOS 5/6 and 25/26 (10.x/11.x)".into())
}
pub fn hook_owned(system: &impl System) -> bool {
    fs::read_link(system.path(Path::new(HOOK)))
        .is_ok_and(|p| p == system.path(Path::new("/var/lib/luma-home/home.sh")))
}
pub fn load(system: &impl System) -> Result<Option<Config>> {
    if !system::exists(system, CONFIG) {
        return Ok(None);
    }
    let config: Config =
        serde_json::from_value(system::json(system, CONFIG)?).map_err(|e| e.to_string())?;
    config.validate()?;
    Ok(Some(config))
}
pub fn dispatch(
    system: &impl System,
    command: &str,
    app: Option<&str>,
    selected: Option<&str>,
) -> Result<Value> {
    system::root(system)?;
    let saved = load(system)?;
    let id = selected
        .filter(|id| !id.is_empty())
        .or_else(|| saved.as_ref().map(|c| c.home_id.as_str()))
        .unwrap_or(HOME);
    let home = Home::new(system, id)?;
    match command {
        "status" => Ok(
            json!({"enabled":saved.as_ref().is_some_and(|c|c.enabled),"method":saved.as_ref().map(|c|c.method.as_str()),"homeId":id,"homeKeyVerified":null,"availableTargets":([HOME,FULL_HOME].into_iter().filter(|id|home.exists(format!("/usr/palm/applications/{id}/appinfo.json"))).collect::<Vec<_>>()),"mounted":saved.as_ref().is_some_and(|c|if c.method=="executable"{home.exe_mounted(&c.app_dir)}else{home.mounted()}),"bootHook":hook_owned(system),"earlyStart":saved.as_ref().is_some_and(|c|c.method=="executable")&&home.exists(format!("{LS2}/services.d/{EARLY}.service")),"appDir":saved.as_ref().map(|c|c.app_dir.as_str())}),
        ),
        "enable" => {
            home.install(Path::new(app.ok_or("Missing installation directory")?))?;
            Ok(json!({"returnValue":true}))
        }
        "apply" | "disable" | "restore" => {
            let mut config = saved.ok_or("Luma Home has not been enabled")?;
            if command == "apply" && config.enabled {
                home.apply(&config, false)?;
            } else {
                home.disable(&mut config)?;
            }
            Ok(json!({"returnValue":true}))
        }
        _ => Err("Unknown Home integration command".into()),
    }
}
