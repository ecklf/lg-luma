use super::{AppSubscription, Spawner, spawn_thread, subscribe};
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::{
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

/// A fake `luna-send`: `script` runs in sh with `$DIR` set to a fresh directory and SIGPIPE ignored.
struct Fake {
    dir: PathBuf,
    messages: mpsc::Receiver<Value>,
    ends: mpsc::Receiver<bool>,
}

impl Fake {
    fn start(
        script: &str,
        timeout: Duration,
        spawn: Spawner,
    ) -> (Self, Result<AppSubscription, String>) {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "luma-app-subscription-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let mut command = Command::new("sh");
        // Ignore SIGPIPE, as a closed pipe would otherwise end the script on its next write and
        // hide whether it was killed. luna-send only meets that on a later reply, if ever.
        let script = format!("trap '' PIPE\n{script}");
        command.args(["-c", &script]).env("DIR", &dir);
        let (message_sender, messages) = mpsc::channel();
        let (end_sender, ends) = mpsc::channel();
        let result = subscribe(
            command,
            timeout,
            move |message| {
                let _ = message_sender.send(message);
            },
            move |registered| {
                let _ = end_sender.send(registered);
            },
            spawn,
        );
        (
            Self {
                dir,
                messages,
                ends,
            },
            result,
        )
    }

    fn file(&self, name: &str) -> PathBuf {
        self.dir.join(name)
    }

    fn end(&self) -> Option<bool> {
        self.ends.recv_timeout(Duration::from_secs(5)).ok()
    }

    fn messages(&self) -> Vec<String> {
        self.messages
            .try_iter()
            .map(|message| message["message"].as_str().unwrap_or_default().to_owned())
            .collect()
    }
}

impl Drop for Fake {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn appears(path: &Path, within: Duration) -> bool {
    let deadline = Instant::now() + within;
    while Instant::now() < deadline {
        if path.exists() {
            return true;
        }
        thread::sleep(Duration::from_millis(20));
    }
    false
}

/// Kept alive, the child would create `survived` one second after its replies.
const SURVIVE: &str = r#"sleep 1; touch "$DIR/survived"; exec sleep 30"#;

#[test]
fn delivers_replies_skips_noise_and_ends_on_request() {
    let script = r#"echo '{"returnValue":true,"message":"registered"}'
echo '{"returnValue":true,"message":"relaunch"}'
echo 'not json'
printf '\377\376 not utf-8\n'
echo '{"returnValue":true,"message":"close"}'
touch "$DIR/replied"; sleep 2; touch "$DIR/survived"; exec sleep 30"#;
    let started = Instant::now();
    let (fake, subscription) = Fake::start(script, Duration::from_secs(10), spawn_thread);
    let subscription = subscription.unwrap();
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "subscribe must not wait for the service"
    );
    assert!(appears(&fake.file("replied"), Duration::from_secs(5)));
    thread::sleep(Duration::from_millis(100));
    assert_eq!(fake.messages(), ["registered", "relaunch", "close"]);
    subscription.end();
    assert_eq!(fake.end(), Some(true));
    assert!(!appears(&fake.file("survived"), Duration::from_secs(3)));
}

#[test]
fn a_refused_first_reply_ends_unregistered() {
    let script = format!(r#"echo '{{"returnValue":false,"errorText":"nope"}}'; {SURVIVE}"#);
    let (fake, subscription) = Fake::start(&script, Duration::from_secs(10), spawn_thread);
    assert!(subscription.is_ok());
    assert_eq!(fake.end(), Some(false));
    assert!(fake.messages().is_empty());
    assert!(!appears(&fake.file("survived"), Duration::from_secs(2)));
}

#[test]
fn no_reply_within_the_timeout_ends_unregistered() {
    let (fake, subscription) = Fake::start(SURVIVE, Duration::from_millis(200), spawn_thread);
    assert!(subscription.is_ok());
    assert_eq!(fake.end(), Some(false));
    assert!(!appears(&fake.file("survived"), Duration::from_secs(2)));
}

#[test]
fn a_child_that_exits_first_ends_unregistered() {
    let (fake, subscription) = Fake::start("exit 1", Duration::from_secs(10), spawn_thread);
    assert!(subscription.is_ok());
    assert_eq!(fake.end(), Some(false));
}

#[test]
fn a_registration_lost_later_ends_registered() {
    let script = format!(
        r#"echo '{{"returnValue":true,"message":"registered"}}'
echo '{{"returnValue":false,"errorText":"sam restarted"}}'; {SURVIVE}"#
    );
    let (fake, subscription) = Fake::start(&script, Duration::from_secs(10), spawn_thread);
    assert!(subscription.is_ok());
    assert_eq!(fake.end(), Some(true));
    assert_eq!(fake.messages(), ["registered"]);
    assert!(!appears(&fake.file("survived"), Duration::from_secs(2)));
}

fn fail_every_thread(_: &str, _: Box<dyn FnOnce() + Send>) -> std::io::Result<()> {
    Err(std::io::Error::other("injected"))
}

fn fail_reader_thread(name: &str, body: Box<dyn FnOnce() + Send>) -> std::io::Result<()> {
    if name == "luna-app-subscription" {
        Err(std::io::Error::other("injected"))
    } else {
        spawn_thread(name, body)
    }
}

#[test]
fn thread_start_failures_kill_the_child() {
    let script = format!(r#"echo '{{"returnValue":true,"message":"registered"}}'; {SURVIVE}"#);
    for spawn in [fail_every_thread as Spawner, fail_reader_thread] {
        let (fake, subscription) = Fake::start(&script, Duration::from_secs(10), spawn);
        assert!(subscription.is_err());
        assert!(!appears(&fake.file("survived"), Duration::from_secs(2)));
        // A failed setup does not report an end; the caller already has the error.
        assert_eq!(
            fake.ends.recv_timeout(Duration::from_millis(300)).ok(),
            None
        );
    }
}
