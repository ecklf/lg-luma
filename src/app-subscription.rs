//! A Luna subscription held by a child process (on the TV, a root `luna-send -a …`), for calls
//! Luma's own LS2 connection may not make. Free of LS2, so the host tests can exercise it.
use serde_json::Value;
use std::{
    io::{BufRead, BufReader},
    os::unix::process::CommandExt,
    panic::{AssertUnwindSafe, catch_unwind},
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex, mpsc},
    thread,
    time::Duration,
};

#[cfg(test)]
#[path = "app-subscription-tests.rs"]
mod tests;

const MAX_REPLY: usize = 2 * 1024 * 1024;

/// Starts a named thread. Injectable so tests can make thread creation fail.
pub type Spawner = fn(&str, Box<dyn FnOnce() + Send>) -> std::io::Result<()>;

pub fn spawn_thread(name: &str, body: Box<dyn FnOnce() + Send>) -> std::io::Result<()> {
    thread::Builder::new()
        .name(name.into())
        .spawn(body)
        .map(drop)
}

/// A subscription held by a child process; see `subscribe`.
#[derive(Clone)]
pub struct AppSubscription {
    child: Arc<Mutex<Option<Child>>>,
}

impl AppSubscription {
    /// Ends the subscription by killing the child; its reader then reports the end.
    pub fn end(&self) {
        end_child(&self.child);
    }
}

/// Kills and reaps the child exactly once, whichever side gets here first.
fn end_child(child: &Mutex<Option<Child>>) {
    if let Some(mut child) = child.lock().ok().and_then(|mut child| child.take()) {
        let _ = child.kill();
        let _ = child.wait();
    }
}

/// Runs `command`, which prints each reply as one JSON line (as `luna-send -i` does), and returns
/// without waiting for it. Each reply goes to `on_message` on a reader thread. No first reply
/// within `timeout`, a refused first reply, a later `returnValue: false` or the output ending all
/// end the child, and then `on_end` runs once with whether the first reply was accepted.
///
/// The child gets SIGKILL when the calling thread ends (call this from the main thread), so it
/// never outlives Luma. A setup error after the child started kills and reaps it before
/// returning; `on_end` does not run then.
pub fn subscribe(
    mut command: Command,
    timeout: Duration,
    mut on_message: impl FnMut(Value) + Send + 'static,
    on_end: impl FnOnce(bool) + Send + 'static,
    spawn: Spawner,
) -> Result<AppSubscription, String> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    // SAFETY: getpid has no preconditions.
    let parent = unsafe { libc::getpid() };
    // SAFETY: prctl, getppid and _exit are async-signal-safe and only affect the forked child.
    unsafe {
        command.pre_exec(move || {
            if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            // The parent may have ended before the death signal was armed.
            if libc::getppid() != parent {
                libc::_exit(1);
            }
            Ok(())
        });
    }
    let mut child = command
        .spawn()
        .map_err(|error| format!("Luna app subscription: {error}"))?;
    let stdout = child.stdout.take();
    let child = Arc::new(Mutex::new(Some(child)));
    let fail = |error: String| {
        end_child(&child);
        Err(format!("Luna app subscription: {error}"))
    };
    let Some(stdout) = stdout else {
        return fail("no output".into());
    };
    let (answered, handshake) = mpsc::sync_channel::<()>(1);
    let waiting = Arc::clone(&child);
    let started = spawn(
        "luna-app-handshake",
        Box::new(move || {
            // A disconnect means the reader already finished; only silence ends the child here.
            if let Err(mpsc::RecvTimeoutError::Timeout) = handshake.recv_timeout(timeout) {
                eprintln!("Luna app subscription: no reply");
                end_child(&waiting);
            }
        }),
    );
    if let Err(error) = started {
        return fail(error.to_string());
    }
    let reader = Arc::clone(&child);
    let started = spawn(
        "luna-app-subscription",
        Box::new(move || {
            let mut answered = Some(answered);
            let mut registered = false;
            // One compact JSON reply per line. Skip anything else, including invalid UTF-8.
            for line in BufReader::new(stdout).split(b'\n').map_while(Result::ok) {
                if line.len() > MAX_REPLY {
                    continue;
                }
                let Ok(value) = serde_json::from_slice::<Value>(&line) else {
                    continue;
                };
                let refused = value.get("returnValue").and_then(Value::as_bool) == Some(false);
                if let Some(answered) = answered.take() {
                    let _ = answered.send(());
                    registered = !refused;
                }
                if refused {
                    eprintln!("Luna app subscription refused: {value}");
                    break;
                }
                if catch_unwind(AssertUnwindSafe(|| on_message(value))).is_err() {
                    eprintln!("Luna app subscription: handler panicked");
                }
            }
            end_child(&reader);
            eprintln!("Luna app subscription ended (was registered: {registered})");
            if catch_unwind(AssertUnwindSafe(|| on_end(registered))).is_err() {
                eprintln!("Luna app subscription: end handler panicked");
            }
        }),
    );
    if let Err(error) = started {
        // The handshake thread sees its sender dropped and leaves the child to this.
        return fail(error.to_string());
    }
    Ok(AppSubscription { child })
}
