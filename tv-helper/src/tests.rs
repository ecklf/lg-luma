use super::*;
use crate::system::{Output, System};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, HashMap, VecDeque},
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
    time::Duration,
};
const DIR: &str = "/media/developer/apps/usr/palm/applications/com.ecklf.luma";
const EXE: &str = "/usr/bin/com.webos.app.home";
struct Fixture {
    root: PathBuf,
    calls: RefCell<Vec<String>>,
    replies: RefCell<HashMap<String, VecDeque<Result<Value>>>>,
    mounts: RefCell<BTreeMap<String, String>>,
    front: RefCell<String>,
    launch_front: Cell<bool>,
    normal: Cell<bool>,
    close_normal: Cell<bool>,
    uid: Cell<u32>,
    pauses: Cell<usize>,
    fail_unmount: Cell<bool>,
    stuck: Cell<bool>,
    /// Other apps SAM reports as running, and the version LG's installer installs next.
    running_apps: RefCell<Vec<String>>,
    installs: RefCell<Option<String>>,
}
impl Fixture {
    fn new() -> Self {
        Self::under(&std::env::temp_dir())
    }
    /// The root is canonical like the paths Home::install derives from it: macOS TMPDIR
    /// (/var/folders/...) is a symlinked parent of /private/var/folders/....
    fn under(parent: &Path) -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let root = parent.join(format!(
            "luma-native-fixture-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        let root = fs::canonicalize(root).unwrap();
        let f = Self {
            root,
            calls: RefCell::new(vec![]),
            replies: RefCell::new(HashMap::new()),
            mounts: RefCell::new(BTreeMap::new()),
            front: RefCell::new("other.app".into()),
            launch_front: Cell::new(true),
            normal: Cell::new(false),
            close_normal: Cell::new(true),
            uid: Cell::new(0),
            pauses: Cell::new(0),
            fail_unmount: Cell::new(false),
            stuck: Cell::new(false),
            running_apps: RefCell::new(vec![]),
            installs: RefCell::new(None),
        };
        for dir in [
            "/var/lib/luma-home/app",
            "/var/lib/webosbrew/init.d",
            "/var/run",
            "/proc/self",
        ] {
            fs::create_dir_all(f.p(dir)).unwrap();
        }
        f.put("/proc/self/mountinfo", b"");
        f.put("/etc/starfish-release", b"Rockhopper release 5.6.2-21");
        for id in [home::HOME, home::FULL_HOME] {
            f.put(&format!("/usr/bin/{id}"), format!("stock-{id}").as_bytes());
            f.put_json(
                &format!("/usr/palm/applications/{id}/appinfo.json"),
                stock(id),
            );
        }
        f.put_json(&format!("{DIR}/appinfo.json"),json!({"id":APP,"type":"native","main":"luma","icon":"app-icon.png","largeIcon":"app-icon-large.png","supportGIP":true}));
        f.put(&format!("{DIR}/luma"), b"native-luma");
        f.put(&format!("{DIR}/luma-helper"), b"native-helper");
        f.put(&format!("{DIR}/home.sh"), b"#!/bin/sh\n");
        f.put(&format!("{DIR}/splash.png"), b"default");
        for name in ["dunes", "forest", "prism"] {
            f.put(&format!("{DIR}/splash/{name}.png"), name.as_bytes());
        }
        f
    }
    fn p(&self, path: &str) -> PathBuf {
        self.path(Path::new(path))
    }
    fn put(&self, path: &str, bytes: &[u8]) {
        let p = self.p(path);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, bytes).unwrap();
    }
    fn put_json(&self, path: &str, value: Value) {
        self.put(path, value.to_string().as_bytes());
    }
    fn config(&self, method: &str, id: &str) -> home::Config {
        home::Config {
            enabled: true,
            method: method.into(),
            app_dir: DIR.into(),
            stock: stock(id),
            home_id: id.into(),
            extra: BTreeMap::new(),
        }
    }
    fn reply(&self, method: &str, value: Result<Value>) {
        self.replies
            .borrow_mut()
            .entry(method.into())
            .or_default()
            .push_back(value);
    }
    fn sync_mounts(&self) {
        let data = self
            .mounts
            .borrow()
            .iter()
            .map(|(target, source)| format!("1 0 0:1 {source} {target} rw - ext4 /dev/test rw\n"))
            .collect::<String>();
        self.put("/proc/self/mountinfo", data.as_bytes());
    }
    fn mount(&self, source: &str, target: &str) {
        let p = self.p(target);
        let backup = p.with_file_name(format!(
            "{}.stock",
            p.file_name().unwrap().to_string_lossy()
        ));
        fs::rename(&p, &backup).unwrap();
        if self.p(source).is_dir() {
            #[cfg(unix)]
            std::os::unix::fs::symlink(self.p(source), &p).unwrap();
        } else {
            fs::hard_link(self.p(source), &p).unwrap();
        }
        self.mounts
            .borrow_mut()
            .insert(target.into(), source.into());
        self.sync_mounts();
    }
    fn unmount(&self, target: &str) {
        let p = self.p(target);
        fs::remove_file(&p).unwrap();
        let backup = p.with_file_name(format!(
            "{}.stock",
            p.file_name().unwrap().to_string_lossy()
        ));
        fs::rename(backup, p).unwrap();
        self.mounts.borrow_mut().remove(target);
        self.sync_mounts();
    }
    fn calls(&self) -> Vec<String> {
        self.calls.borrow().clone()
    }
    fn read(&self, path: &str) -> Vec<u8> {
        fs::read(self.p(path)).unwrap()
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
    fn uid(&self) -> u32 {
        self.uid.get()
    }
    fn pause(&self) {
        self.pauses.set(self.pauses.get() + 1);
    }
    fn terminate(&self, pid: u32) -> Result<()> {
        self.calls.borrow_mut().push(format!("kill {pid}"));
        if !self.stuck.get() {
            fs::remove_dir_all(self.p(&format!("/proc/{pid}"))).unwrap();
        }
        Ok(())
    }
    fn run(&self, program: &str, args: &[String], _: Duration, _: usize) -> Result<Output> {
        self.calls
            .borrow_mut()
            .push(format!("{program} {}", args.join(" ")));
        if program == "/bin/mount" {
            self.mount(&args[1], &args[2]);
            return Ok(ok(json!({})));
        }
        if program == "/bin/umount" {
            if self.fail_unmount.replace(false) {
                return Ok(Output {
                    stdout: vec![],
                    stderr: b"busy".to_vec(),
                    success: false,
                });
            }
            self.unmount(args.last().unwrap());
            return Ok(ok(json!({})));
        }
        if program != "/usr/bin/luna-send" {
            panic!("Unexpected system command: {program}");
        }
        let uri = &args[args.len() - 2];
        let method = uri.rsplit('/').next().unwrap();
        let payload: Value = serde_json::from_str(args.last().unwrap()).unwrap();
        if let Some(reply) = self
            .replies
            .borrow_mut()
            .get_mut(method)
            .and_then(VecDeque::pop_front)
        {
            return reply.map(ok);
        }
        let reply = match method {
            "getForegroundAppInfo" => json!({"returnValue":true,"appId":*self.front.borrow()}),
            "launch" => {
                if self.launch_front.get() {
                    *self.front.borrow_mut() = payload["id"].as_str().unwrap().into();
                }
                json!({"returnValue":true})
            }
            "running" => {
                let mut running: Vec<Value> = self
                    .running_apps
                    .borrow()
                    .iter()
                    .map(|id| json!({"id":id}))
                    .collect();
                if self.normal.get() {
                    running.push(json!({"id":APP}));
                }
                json!({"returnValue":true,"running":running})
            }
            "install" => {
                // LG's developer installer: replaces the app with the package's version.
                let id = payload["id"].as_str().unwrap();
                let package = payload["ipkUrl"].as_str().unwrap();
                assert!(
                    system::exists(self, package),
                    "installer reads the root-owned copy"
                );
                if let Some(version) = self.installs.borrow_mut().take() {
                    self.put_json(
                        &format!("{}/{id}/appinfo.json", crate::app_update::APPS),
                        json!({"id":id,"version":version}),
                    );
                }
                json!({"returnValue":true})
            }
            "closeByAppId" => {
                if self.close_normal.get() {
                    self.normal.set(false);
                    json!({"returnValue":true})
                } else {
                    json!({"returnValue":false,"errorText":"refused"})
                }
            }
            "getAppInfo" => {
                let app = system::json(
                    self,
                    format!(
                        "/usr/palm/applications/{}/appinfo.json",
                        payload["id"].as_str().unwrap()
                    ),
                )?;
                json!({"returnValue":true,"appInfo":app})
            }
            "blockSystemApp" | "releaseBlockSystemApp" | "addDevice" | "deleteDevice" => {
                json!({"returnValue":true})
            }
            _ => panic!("Unexpected Luna method: {method}"),
        };
        Ok(ok(reply))
    }
}
fn ok(reply: Value) -> Output {
    Output {
        stdout: reply.to_string().into_bytes(),
        stderr: vec![],
        success: true,
    }
}
fn stock(id: &str) -> Value {
    json!({"id":id,"type":"native","main":format!("file:///usr/bin/{id}"),"nativeLifeCycleInterfaceVersion":2})
}

#[test]
fn home_methods_reject_unknown_firmware_or_lifecycle() {
    for release in ["Rockhopper release 5.6.2", "Rockhopper release 6.5.0"] {
        assert_eq!(
            home::method(release, &stock(home::HOME), home::HOME).unwrap(),
            "executable"
        );
    }
    for release in ["Rockhopper release 10.1.0", "Rockhopper release 11.2.0"] {
        assert_eq!(
            home::method(release, &Value::Null, home::HOME).unwrap(),
            "manifest"
        );
    }
    for release in ["Rockhopper release 4.10.2", "Rockhopper release 9.2.0"] {
        assert!(home::method(release, &stock(home::HOME), home::HOME).is_err());
    }
    let mut bad = stock(home::HOME);
    bad["nativeLifeCycleInterfaceVersion"] = json!(1);
    assert!(home::method("release 5.6.2", &bad, home::HOME).is_err());
    assert!(home::method("release 10.1.0", &stock(home::FULL_HOME), home::FULL_HOME).is_err());
    assert!(home::Home::new(&Fixture::new(), "../other").is_err());
}
#[test]
fn only_luma_mounts_are_owned_even_after_package_replacement() {
    for root in [
        "/developer/apps/usr/palm/applications/com.ecklf.luma/luma",
        "/cryptofs/apps/usr/palm/applications/com.ecklf.luma/luma//deleted",
    ] {
        assert!(home::exe_owned(root));
    }
    assert!(!home::exe_owned("/var/lib/other/launcher"));
    assert!(!home::exe_owned("/com.ecklf.luma/luma/../other"));
}
#[test]
fn executable_apply_mounts_marks_and_launches_then_restore_unmounts() {
    let f = Fixture::new();
    let h = home::Home::new(&f, home::HOME).unwrap();
    let c = f.config("executable", home::HOME);
    h.apply(&c, true).unwrap();
    assert!(system::same_file(&f, EXE, format!("{DIR}/luma")));
    assert_eq!(f.read("/var/run/luma-home"), format!("{APP}\n").as_bytes());
    assert_eq!(f.front.borrow().as_str(), home::HOME);
    h.restore(&c).unwrap();
    assert!(!system::exists(&f, "/var/run/luma-home"));
    assert_eq!(f.read(EXE), format!("stock-{}", home::HOME).as_bytes());
}
#[test]
fn fullhome_round_trip_and_persisted_target_keep_their_identity() {
    let f = Fixture::new();
    home::dispatch(&f, "enable", Some(DIR), Some(home::FULL_HOME)).unwrap();
    let c = home::load(&f).unwrap().unwrap();
    assert_eq!(c.home_id, home::FULL_HOME);
    assert!(c.enabled);
    assert_eq!(
        f.read("/var/run/luma-home-target"),
        format!("{}\n", home::FULL_HOME).as_bytes()
    );
    let status = home::dispatch(&f, "status", None, None).unwrap();
    assert_eq!(status["homeId"], home::FULL_HOME);
    assert_eq!(status["mounted"], true);
    home::dispatch(&f, "disable", None, None).unwrap();
    assert_eq!(
        f.read("/usr/bin/com.webos.app.fullhome"),
        b"stock-com.webos.app.fullhome"
    );
    assert!(!system::exists(&f, "/var/run/luma-home-target"));
}
#[test]
fn installation_under_a_symlinked_temporary_parent_stays_inside_the_root() {
    let real = std::env::temp_dir().join(format!("luma-native-real-{}", std::process::id()));
    let link = std::env::temp_dir().join(format!("luma-native-link-{}", std::process::id()));
    fs::create_dir_all(&real).unwrap();
    let _ = fs::remove_file(&link);
    std::os::unix::fs::symlink(&real, &link).unwrap();
    {
        let f = Fixture::under(&link);
        home::dispatch(&f, "enable", Some(DIR), None).unwrap();
        assert_eq!(home::load(&f).unwrap().unwrap().app_dir, DIR);
        home::dispatch(&f, "disable", None, None).unwrap();
    }
    fs::remove_file(&link).unwrap();
    fs::remove_dir_all(&real).unwrap();
}
#[test]
fn copied_recovery_binary_and_shell_survive_package_removal() {
    let f = Fixture::new();
    home::dispatch(&f, "enable", Some(DIR), None).unwrap();
    assert_eq!(f.read("/var/lib/luma-home/luma-helper"), b"native-helper");
    fs::remove_dir_all(f.p(DIR)).unwrap();
    home::dispatch(&f, "disable", None, None).unwrap();
    assert_eq!(f.read(EXE), b"stock-com.webos.app.home");
}
#[test]
fn boot_apply_does_not_take_another_apps_screen() {
    let f = Fixture::new();
    home::Home::new(&f, home::HOME)
        .unwrap()
        .apply(&f.config("executable", home::HOME), false)
        .unwrap();
    assert_eq!(f.front.borrow().as_str(), "other.app");
    assert!(!f.calls().iter().any(|s| s.contains("/launch ")));
}
#[test]
fn package_update_replaces_the_old_bound_inode() {
    let f = Fixture::new();
    let h = home::Home::new(&f, home::HOME).unwrap();
    let c = f.config("executable", home::HOME);
    h.apply(&c, false).unwrap();
    let changed = f.p(&format!("{DIR}/luma.new"));
    fs::write(&changed, b"updated").unwrap();
    fs::rename(changed, f.p(&format!("{DIR}/luma"))).unwrap();
    h.apply(&c, false).unwrap();
    assert_eq!(f.read(EXE), b"updated");
    assert!(f.calls().iter().any(|s| s == &format!("/bin/umount {EXE}")));
}
#[test]
fn foreign_mounts_are_left_untouched() {
    let f = Fixture::new();
    f.put("/var/lib/other/launcher", b"foreign");
    f.mount("/var/lib/other/launcher", EXE);
    let h = home::Home::new(&f, home::HOME).unwrap();
    assert!(
        h.apply(&f.config("executable", home::HOME), true)
            .unwrap_err()
            .contains("Another integration")
    );
    assert_eq!(f.read(EXE), b"foreign");
    assert!(!f.calls().iter().any(|s| s.contains("umount")));
}
#[test]
fn failed_launch_retries_once_then_restores_stock_files() {
    let f = Fixture::new();
    f.launch_front.set(false);
    let h = home::Home::new(&f, home::HOME).unwrap();
    assert!(
        h.apply(&f.config("executable", home::HOME), true)
            .unwrap_err()
            .contains("did not come")
    );
    assert_eq!(f.read(EXE), b"stock-com.webos.app.home");
    assert!(!system::exists(&f, "/var/run/luma-home"));
    assert_eq!(
        f.calls().iter().filter(|s| s.contains("/launch ")).count(),
        2
    );
}
#[test]
fn restoration_works_even_when_foreground_query_fails() {
    let f = Fixture::new();
    let h = home::Home::new(&f, home::HOME).unwrap();
    let c = f.config("executable", home::HOME);
    h.apply(&c, true).unwrap();
    f.reply("getForegroundAppInfo", Err("offline".into()));
    h.restore(&c).unwrap();
    assert_eq!(f.read(EXE), b"stock-com.webos.app.home");
}
#[test]
fn busy_mounts_use_lazy_unmount_and_restore_reports_stuck_processes() {
    let f = Fixture::new();
    let h = home::Home::new(&f, home::HOME).unwrap();
    let c = f.config("executable", home::HOME);
    h.apply(&c, true).unwrap();
    f.fail_unmount.set(true);
    h.restore(&c).unwrap();
    assert!(
        f.calls()
            .iter()
            .any(|s| s == &format!("/bin/umount -l {EXE}"))
    );
}
#[test]
fn changing_an_enabled_target_or_using_a_foreign_hook_is_rejected() {
    let f = Fixture::new();
    home::dispatch(&f, "enable", Some(DIR), None).unwrap();
    assert!(
        home::dispatch(&f, "enable", Some(DIR), Some(home::FULL_HOME))
            .unwrap_err()
            .contains("Disable")
    );
    home::dispatch(&f, "disable", None, None).unwrap();
    system::link(
        &f,
        "/other/home.sh",
        "/var/lib/webosbrew/init.d/64-luma-home",
    )
    .unwrap();
    assert!(
        home::dispatch(&f, "enable", Some(DIR), None)
            .unwrap_err()
            .contains("already in use")
    );
}
#[test]
fn old_configuration_without_method_or_target_is_readable() {
    let f = Fixture::new();
    f.put_json(
        "/var/lib/luma-home/state.json",
        json!({"enabled":false,"appDir":DIR,"stock":stock(home::HOME),"futureFlag":42}),
    );
    let c = home::load(&f).unwrap().unwrap();
    assert_eq!(c.method, "manifest");
    assert_eq!(c.home_id, home::HOME);
    assert_eq!(c.extra["futureFlag"], 42);
}
#[test]
fn manifest_takeover_rereads_staged_metadata_and_restores_the_stock_manifest() {
    let f = Fixture::new();
    f.put("/etc/starfish-release", b"Rockhopper release 11.2.0");
    home::dispatch(&f, "enable", Some(DIR), None).unwrap();
    assert_eq!(
        system::json(&f, "/usr/palm/applications/com.webos.app.home/appinfo.json").unwrap()["main"],
        "luma"
    );
    home::dispatch(&f, "disable", None, None).unwrap();
    assert_eq!(
        system::json(&f, "/usr/palm/applications/com.webos.app.home/appinfo.json").unwrap()["main"],
        format!("file://{EXE}")
    );
}
#[test]
fn release_is_attempted_even_when_the_block_request_times_out() {
    let f = Fixture::new();
    f.put("/etc/starfish-release", b"Rockhopper release 11.2.0");
    f.reply("blockSystemApp", Err("timeout after block".into()));
    assert!(home::dispatch(&f, "enable", Some(DIR), None).is_err());
    assert!(
        f.calls()
            .iter()
            .any(|s| s.contains("/releaseBlockSystemApp "))
    );
    assert!(
        !f.mounts
            .borrow()
            .contains_key("/usr/palm/applications/com.webos.app.home")
    );
}
#[test]
fn normal_tile_is_closed_by_app_id_and_an_exiting_instance_is_tolerated() {
    let f = Fixture::new();
    f.normal.set(true);
    home::dispatch(&f, "enable", Some(DIR), None).unwrap();
    assert!(
        f.calls()
            .iter()
            .any(|s| s.contains("/closeByAppId ") && s.contains(APP))
    );
    assert!(!f.normal.get());
}
#[test]
fn normal_close_denials_and_stuck_instances_prevent_mounting() {
    let f = Fixture::new();
    f.normal.set(true);
    f.close_normal.set(false);
    assert!(home::dispatch(&f, "enable", Some(DIR), None).is_err());
    assert!(f.mounts.borrow().is_empty());
}
#[test]
fn process_matching_never_kills_a_normal_luma_or_unrelated_native_app() {
    let f = Fixture::new();
    let args = |main: &str, meta: Value| vec![main.into(), meta.to_string()];
    let exe = f.p(&format!("{DIR}/luma"));
    assert!(home::home_process(
        &args(EXE, json!({})),
        None,
        DIR,
        home::HOME,
        &f
    ));
    assert!(home::home_process(
        &args("luma", json!({"nid":home::HOME})),
        Some(&exe),
        DIR,
        home::HOME,
        &f
    ));
    assert!(!home::home_process(
        &args("luma", json!({"nid":APP})),
        Some(&exe),
        DIR,
        home::HOME,
        &f
    ));
    assert!(!home::home_process(
        &args("/usr/bin/com.webos.app.livetv", json!({})),
        None,
        DIR,
        home::HOME,
        &f
    ));
}

#[test]
fn icon_discovery_uses_live_installation_then_manifest_and_default_fallbacks() {
    let f = Fixture::new();
    let dir = "/mnt/otncabi/usr/palm/applications/sample";
    f.put_json(
        &format!("{dir}/appinfo.json"),
        json!({"id":"sample","largeIcon":"large.webp","icon":"normal.png"}),
    );
    f.put(&format!("{dir}/large.webp"), &vec![1; 512 * 1024 + 1]);
    f.put(&format!("{dir}/normal.png"), b"normal");
    f.put(&format!("{dir}/icon.png"), b"default");
    let source = files::IconSource {
        id: "sample".into(),
        directory: Some(dir.into()),
        large_icon: None,
        icon: None,
    };
    let reply = files::read_icons(&f, &source).unwrap();
    assert_eq!(reply["icons"], json!(["6e6f726d616c", "64656661756c74"]));
}
#[test]
fn icon_limits_are_aggregate_and_duplicate_paths_are_not_read_twice() {
    let f = Fixture::new();
    let dir = "/usr/palm/applications/sample";
    f.put_json(
        &format!("{dir}/appinfo.json"),
        json!({"id":"sample","largeIcon":"big.png","icon":"big.png"}),
    );
    f.put(&format!("{dir}/big.png"), &vec![7; 512 * 1024]);
    f.put(&format!("{dir}/icon.png"), b"not included");
    let source = files::IconSource {
        id: "sample".into(),
        directory: None,
        large_icon: None,
        icon: None,
    };
    let reply = files::read_icons(&f, &source).unwrap();
    assert_eq!(reply["icons"].as_array().unwrap().len(), 1);
    assert_eq!(reply["icons"][0].as_str().unwrap().len(), 1024 * 1024);
}
#[test]
fn icon_paths_cannot_escape_but_shell_punctuation_remains_literal() {
    let f = Fixture::new();
    let dir = "/usr/palm/applications/sample";
    let name = "safe;$(touch no).png";
    f.put(&format!("{dir}/{name}"), b"safe");
    for icon in [
        "../../etc/a.png",
        "file:///etc/a.png",
        "https://evil/a.png",
        "bad\n.png",
    ] {
        let source = files::IconSource {
            id: "sample".into(),
            directory: None,
            large_icon: None,
            icon: Some(icon.into()),
        };
        assert_eq!(files::read_icons(&f, &source).unwrap()["icons"], json!([]));
    }
    let source = files::IconSource {
        id: "sample".into(),
        directory: None,
        large_icon: None,
        icon: Some(name.into()),
    };
    assert_eq!(
        files::read_icons(&f, &source).unwrap()["icons"],
        json!(["73616665"])
    );
    assert!(f.calls().is_empty());
}
#[test]
fn invalid_ids_and_unrelated_manifests_cannot_select_private_artwork() {
    let f = Fixture::new();
    let mut s = files::IconSource {
        id: "../etc".into(),
        directory: None,
        large_icon: None,
        icon: None,
    };
    assert!(files::read_icons(&f, &s).is_err());
    s.id = "sample".into();
    f.put_json(
        "/usr/palm/applications/sample/appinfo.json",
        json!({"id":"other","icon":"private.png"}),
    );
    f.put("/usr/palm/applications/sample/private.png", b"private");
    assert_eq!(files::read_icons(&f, &s).unwrap()["icons"], json!([]));
}

#[test]
fn splash_updates_tile_and_only_the_enabled_own_manifest_stage() {
    let f = Fixture::new();
    f.put_json(
        "/var/lib/luma-home/state.json",
        json!({"enabled":true,"method":"manifest","appDir":DIR}),
    );
    f.put("/var/lib/luma-home/app/splash.png", b"old");
    files::sync_splash(&f, Path::new(DIR), "forest").unwrap();
    assert_eq!(f.read(&format!("{DIR}/splash.png")), b"forest");
    assert_eq!(f.read("/var/lib/luma-home/app/splash.png"), b"forest");
    let before = fs::metadata(f.p(&format!("{DIR}/splash.png")))
        .unwrap()
        .modified()
        .unwrap();
    files::sync_splash(&f, Path::new(DIR), "forest").unwrap();
    assert_eq!(
        fs::metadata(f.p(&format!("{DIR}/splash.png")))
            .unwrap()
            .modified()
            .unwrap(),
        before
    );
}
#[test]
fn executable_other_or_disabled_stages_are_never_written() {
    for c in [
        json!({"enabled":true,"method":"executable","appDir":DIR}),
        json!({"enabled":false,"appDir":DIR}),
        json!({"enabled":true,"appDir":"/other/install"}),
    ] {
        let f = Fixture::new();
        f.put_json("/var/lib/luma-home/state.json", c);
        f.put("/var/lib/luma-home/app/splash.png", b"old");
        files::sync_splash(&f, Path::new(DIR), "prism").unwrap();
        assert_eq!(f.read("/var/lib/luma-home/app/splash.png"), b"old");
    }
}
#[test]
fn missing_or_invalid_splash_sources_preserve_previous_images() {
    let f = Fixture::new();
    for background in ["../forest", "bad/name", "missing"] {
        assert!(files::sync_splash(&f, Path::new(DIR), background).is_err());
        assert_eq!(f.read(&format!("{DIR}/splash.png")), b"default");
    }
    assert!(files::sync_splash(&f, Path::new("/tmp/com.ecklf.luma"), "forest").is_err());
}
#[test]
fn failed_atomic_write_cleans_its_private_temporary_file() {
    let f = Fixture::new();
    fs::create_dir_all(f.p("/directory/destination")).unwrap();
    assert!(system::write(&f, "/directory/destination", b"bytes", 0o644).is_err());
    assert!(fs::read_dir(f.p("/directory")).unwrap().all(|entry| {
        !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with('.')
    }));
}
#[test]
fn stage_uses_persistent_selection_and_retains_manual_background_over_rotation() {
    let f = Fixture::new();
    f.put_json(
        &format!("/media/developer/{APP}/preferences.json"),
        json!({"background":"prism","splash_background":"forest"}),
    );
    files::stage_splash(&f, Path::new(DIR)).unwrap();
    assert_eq!(f.read("/var/lib/luma-home/app/splash.png"), b"forest");
}
#[test]
fn null_manual_selection_falls_back_to_saved_background() {
    let f = Fixture::new();
    f.put_json(
        &format!("/media/developer/{APP}/preferences.json"),
        json!({"background":"prism","splash_background":null}),
    );
    files::stage_splash(&f, Path::new(DIR)).unwrap();
    assert_eq!(f.read("/var/lib/luma-home/app/splash.png"), b"prism");
}
#[test]
fn malformed_missing_and_unavailable_preferences_use_package_splash() {
    for preferences in [
        b"not-json".as_slice(),
        b"{}",
        b"{\"background\":\"missing\"}",
        b"{\"background\":\"../bad\"}",
    ] {
        let f = Fixture::new();
        f.put(
            &format!("/media/developer/{APP}/preferences.json"),
            preferences,
        );
        files::stage_splash(&f, Path::new(DIR)).unwrap();
        assert!(matches!(
            f.read("/var/lib/luma-home/app/splash.png").as_slice(),
            b"dunes" | b"default"
        ));
    }
}
#[test]
fn symlink_storage_directories_are_not_used_as_root_preferences() {
    let f = Fixture::new();
    f.put_json("/other/preferences.json", json!({"background":"prism"}));
    system::link(&f, "/other", format!("/media/developer/{APP}")).unwrap();
    f.put_json(
        &format!("/media/developer/temp/{APP}/preferences.json"),
        json!({"background":"forest"}),
    );
    files::stage_splash(&f, Path::new(DIR)).unwrap();
    assert_eq!(f.read("/var/lib/luma-home/app/splash.png"), b"forest");
}

#[test]
fn uninstall_requires_matching_completion_not_just_acceptance() {
    let mut s = uninstall::Stream::default();
    assert!(!s.feed(br#"{"returnValue":true}"#, false, "sample").unwrap());
    assert!(s.feed(br#"{"returnValue":true,"statusValue":31,"id":"sample","details":{"packageId":"sample"}}"#,false,"sample").unwrap());
    assert_eq!(s.replies.len(), 2);
}
#[test]
fn uninstall_parser_handles_split_pretty_unicode_and_braces_in_strings() {
    let text = "{\n\"returnValue\":true,\"text\":\"Ä {quoted}\\\"\"}\n{\"statusValue\":31,\"id\":\"sample\"}";
    let mut s = uninstall::Stream::default();
    let mut done = false;
    for byte in text.bytes() {
        done = s.feed(&[byte], false, "sample").unwrap();
    }
    assert!(done);
    assert_eq!(s.replies.len(), 2);
}
#[test]
fn uninstall_rejects_failures_wrong_identity_and_resource_exhaustion() {
    for reply in [
        json!({"returnValue":false}),
        json!({"statusValue":25}),
        json!({"details":{"errorCode":1}}),
        json!({"statusValue":31,"id":"other"}),
        json!({"statusValue":31,"id":"sample","details":{"packageId":"other"}}),
    ] {
        assert!(uninstall::completed(&reply, "sample").is_err());
    }
    let mut s = uninstall::Stream::default();
    assert!(s.feed(&vec![0; 65537], true, "sample").is_err());
    let mut s = uninstall::Stream::default();
    for _ in 0..64 {
        s.feed(br#"{}"#, false, "sample").unwrap();
    }
    assert!(s.feed(br#"{}"#, false, "sample").is_err());
}
#[test]
fn uninstall_protects_system_apps_and_does_not_spawn_when_denied() {
    for id in [
        APP,
        "com.webos.app.home",
        "org.webosbrew.hbchannel",
        "bad/id",
        "",
    ] {
        assert!(!uninstall::allowed(id));
        assert_eq!(uninstall::remove(id, 0)["returnValue"], false);
    }
    assert!(uninstall::allowed("org.example.app"));
    assert_eq!(uninstall::remove("sample", 1000)["returnValue"], false);
}
#[test]
fn input_verification_failure_rolls_back_and_existing_registration_is_preserved() {
    let f = Fixture::new();
    f.reply(
        "getAllInputStatus",
        Ok(json!({"returnValue":true,"devices":[]})),
    );
    f.reply(
        "getAllInputStatus",
        Ok(json!({"returnValue":true,"devices":[]})),
    );
    assert!(
        input::configure(&f, "enable", Path::new(DIR))
            .unwrap_err()
            .contains("registration removed")
    );
    assert!(f.calls().iter().any(|s| s.contains("/deleteDevice ")));
    let f = Fixture::new();
    f.reply(
        "getAllInputStatus",
        Ok(json!({"returnValue":true,"devices":[{"appId":APP}]})),
    );
    assert_eq!(
        input::configure(&f, "enable", Path::new(DIR)).unwrap()["registered"],
        true
    );
    assert!(!f.calls().iter().any(|s| s.contains("/addDevice ")));
}
#[test]
fn root_mutations_reject_non_root_callers() {
    let f = Fixture::new();
    f.uid.set(1000);
    assert!(home::dispatch(&f, "enable", Some(DIR), None).is_err());
    assert!(input::configure(&f, "enable", Path::new(DIR)).is_err());
    assert!(f.calls().is_empty());
}
#[test]
fn process_runner_enforces_combined_output_and_deadline() {
    let out = system::run_bounded(
        "/bin/sh",
        &["-c".into(), "printf good; printf error >&2".into()],
        Duration::from_secs(2),
        32,
    )
    .unwrap();
    assert_eq!(out.stdout, b"good");
    assert_eq!(out.stderr, b"error");
    assert!(
        system::run_bounded(
            "/bin/sh",
            &["-c".into(), "printf 123456789".into()],
            Duration::from_secs(2),
            3
        )
        .is_err()
    );
    assert!(
        system::run_bounded("/bin/sleep", &["2".into()], Duration::from_millis(30), 32).is_err()
    );
}

#[test]
fn restoring_a_background_home_never_opens_over_another_app() {
    let f = Fixture::new();
    let h = home::Home::new(&f, home::HOME).unwrap();
    let c = f.config("executable", home::HOME);
    h.apply(&c, false).unwrap();
    h.restore(&c).unwrap();
    assert_eq!(f.front.borrow().as_str(), "other.app");
    assert!(!f.calls().iter().any(|s| s.contains("/launch ")));
}
#[test]
fn a_stuck_home_process_is_reported_after_unmount_recovery() {
    let f = Fixture::new();
    f.put("/proc/777/cmdline", format!("{EXE}\0{{}}\0").as_bytes());
    f.stuck.set(true);
    let error = home::Home::new(&f, home::HOME)
        .unwrap()
        .apply(&f.config("executable", home::HOME), true)
        .unwrap_err();
    assert!(error.contains("Restore also failed"));
    assert_eq!(f.read(EXE), b"stock-com.webos.app.home");
    assert!(!system::exists(&f, "/var/run/luma-home"));
}
#[test]
fn accepted_close_waits_boundedly_and_a_close_race_can_succeed() {
    let f = Fixture::new();
    f.normal.set(true);
    f.reply("closeByAppId", Ok(json!({"returnValue":true})));
    assert!(
        home::dispatch(&f, "enable", Some(DIR), None)
            .unwrap_err()
            .contains("Close Luma")
    );
    assert_eq!(f.pauses.get(), 5);
    assert!(f.mounts.borrow().is_empty());
    let f = Fixture::new();
    f.reply(
        "running",
        Ok(json!({"returnValue":true,"running":[{"id":APP}]})),
    );
    f.reply("running", Ok(json!({"returnValue":true,"running":[]})));
    f.reply("closeByAppId", Ok(json!({"returnValue":false})));
    home::dispatch(&f, "enable", Some(DIR), None).unwrap();
}
#[test]
fn developer_and_production_splash_precedence_matches_native_storage() {
    let f = Fixture::new();
    f.put_json(
        &format!("/media/developer/{APP}/preferences.json"),
        json!({"background":"forest"}),
    );
    f.put_json(
        &format!("/media/internal/.{APP}/preferences.json"),
        json!({"background":"prism"}),
    );
    files::stage_splash(&f, Path::new(DIR)).unwrap();
    assert_eq!(f.read("/var/lib/luma-home/app/splash.png"), b"forest");
    let production = format!("/media/cryptofs/apps/usr/palm/applications/{APP}");
    f.put(&format!("{production}/splash/prism.png"), b"prism");
    files::stage_splash(&f, Path::new(&production)).unwrap();
    assert_eq!(f.read("/var/lib/luma-home/app/splash.png"), b"prism");
}
#[test]
fn persistent_preferences_win_over_package_local_migration_and_active_stages_refresh() {
    let f = Fixture::new();
    f.put("/etc/starfish-release", b"Rockhopper release 11.2.0");
    f.put_json(
        &format!("{DIR}/preferences.json"),
        json!({"background":"prism"}),
    );
    f.put_json(
        &format!("/media/developer/{APP}/preferences.json"),
        json!({"background":"forest"}),
    );
    home::dispatch(&f, "enable", Some(DIR), None).unwrap();
    assert_eq!(
        f.read("/usr/palm/applications/com.webos.app.home/splash.png"),
        b"forest"
    );
    f.put_json(
        &format!("/media/developer/{APP}/preferences.json"),
        json!({"background":"prism"}),
    );
    home::dispatch(&f, "apply", None, None).unwrap();
    assert_eq!(
        f.read("/usr/palm/applications/com.webos.app.home/splash.png"),
        b"prism"
    );
}
mod app_update_tests {
    use super::*;
    use crate::app_update;
    const MOON: &str = "com.limelight.webos";
    const PACKAGE: &[u8] = b"moonlight 1.6.37 package";
    // sha256 of PACKAGE
    fn sha() -> String {
        ring::digest::digest(&ring::digest::SHA256, PACKAGE)
            .as_ref()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }
    fn tv() -> Fixture {
        let f = Fixture::new();
        f.put_json(
            &format!("{}/{MOON}/appinfo.json", app_update::APPS),
            json!({"id":MOON,"version":"1.6.36"}),
        );
        f.put("/tmp/luma-update-x/package.ipk", PACKAGE);
        f
    }
    fn update(f: &Fixture, id: &str, version: &str, sha: &str) -> Result<Value> {
        app_update::install(
            f,
            id,
            version,
            sha,
            Path::new("/tmp/luma-update-x/package.ipk"),
        )
    }
    #[test]
    fn a_verified_package_is_installed_from_a_root_owned_copy_and_confirmed() {
        let f = tv();
        *f.installs.borrow_mut() = Some("1.6.37".into());
        let reply = update(&f, MOON, "1.6.37", &sha()).unwrap();
        assert_eq!(reply["changed"], true);
        assert_eq!(app_update::installed_version(&f, MOON).unwrap(), "1.6.37");
        assert!(
            f.calls()
                .iter()
                .any(|c| c.contains("appInstallService/dev/install")
                    && c.contains("/var/lib/luma-update/apps/com.limelight.webos.ipk"))
        );
        // The copy is gone afterwards; an already current app is left alone.
        assert!(!system::exists(
            &f,
            "/var/lib/luma-update/apps/com.limelight.webos.ipk"
        ));
        assert_eq!(
            update(&f, MOON, "1.6.37", &sha()).unwrap()["changed"],
            false
        );
    }
    #[test]
    fn a_wrong_checksum_never_reaches_the_installer() {
        let f = tv();
        let error = update(&f, MOON, "1.6.37", &"0".repeat(64)).unwrap_err();
        assert!(error.contains("checksum"));
        assert!(!f.calls().iter().any(|c| c.contains("dev/install")));
        assert_eq!(app_update::installed_version(&f, MOON).unwrap(), "1.6.36");
        assert!(!system::exists(
            &f,
            "/var/lib/luma-update/apps/com.limelight.webos.ipk"
        ));
    }
    #[test]
    fn running_excluded_or_foreign_apps_are_not_updated() {
        let f = tv();
        f.running_apps.borrow_mut().push(MOON.into());
        assert!(
            update(&f, MOON, "1.6.37", &sha())
                .unwrap_err()
                .contains("running")
        );
        for id in [APP, "org.webosbrew.hbchannel", "../etc", "netflix"] {
            assert!(update(&f, id, "1.6.37", &sha()).is_err(), "{id}");
        }
        assert!(!f.calls().iter().any(|c| c.contains("dev/install")));
    }
    #[test]
    fn malformed_requests_and_non_root_callers_are_refused() {
        let f = tv();
        for (version, sha) in [
            ("1.6.37-beta", sha()),
            ("", sha()),
            ("1.6.37", "ABC".into()),
            ("1.6.37", sha().to_uppercase()),
        ] {
            assert!(update(&f, MOON, version, &sha).is_err(), "{version} {sha}");
        }
        assert!(
            app_update::install(&f, MOON, "1.6.37", &sha(), Path::new("relative.ipk")).is_err()
        );
        f.uid.set(1000);
        assert!(update(&f, MOON, "1.6.37", &sha()).is_err());
    }
    #[test]
    fn an_installer_that_never_finishes_is_reported() {
        let f = tv();
        // The installer accepts but never changes the installed version.
        let error = update(&f, MOON, "1.6.37", &sha()).unwrap_err();
        assert!(error.contains("not updated"));
        assert_eq!(f.pauses.get(), 240);
    }
}
