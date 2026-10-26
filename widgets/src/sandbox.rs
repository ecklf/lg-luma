//! Transforms run in a child process with hard limits (docs/widgets.md §3.5): a jq program can
//! loop forever, recurse until its stack overflows or build huge values, and none of that may
//! reach the launcher. The parent starts an executable (Luma itself, or `luma-widget`) with a
//! flag, writes the job to its stdin and reads one JSON reply from its stdout.
use crate::Error;
use crate::jq::{self, Vars};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    io::{Read, Write},
    path::Path,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

/// Everything a transform needs, sent to the child.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Job {
    pub program: String,
    pub input: Value,
    pub vars: Vars,
}

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub cpu_seconds: u64,
    pub memory: u64,
    pub wall: Duration,
    pub output: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            cpu_seconds: 2,
            memory: 256 * 1024 * 1024,
            wall: Duration::from_secs(3),
            output: 512 * 1024,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Reply {
    Ok(Value),
    Error(String),
}

/// Runs `job` in `exe flag`, killed after the wall-clock limit.
pub fn run(exe: &Path, flag: &str, job: &Job, limits: &Limits) -> Result<Value, Error> {
    let input = serde_json::to_vec(job).map_err(|e| Error(e.to_string()))?;
    let mut command = Command::new(exe);
    // The child sees none of the parent's environment (which may hold a developer's secret),
    // only what finding its libraries needs.
    command.env_clear();
    for name in [
        "PATH",
        "LD_LIBRARY_PATH",
        "DYLD_LIBRARY_PATH",
        "DYLD_FALLBACK_LIBRARY_PATH",
    ] {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    let mut child = command
        .arg(flag)
        .env("LUMA_TRANSFORM_CPU", limits.cpu_seconds.to_string())
        .env("LUMA_TRANSFORM_MEMORY", limits.memory.to_string())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| Error(format!("the transform could not start: {e}")))?;
    let mut stdin = child.stdin.take().ok_or(Error("no stdin".into()))?;
    let writer = thread::spawn(move || {
        let _ = stdin.write_all(&input);
    });
    let mut stdout = child.stdout.take().ok_or(Error("no stdout".into()))?;
    let limit = limits.output;
    let reader = thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = (&mut stdout).take(limit as u64 + 1).read_to_end(&mut bytes);
        bytes
    });
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if start.elapsed() >= limits.wall => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
            Ok(None) => thread::sleep(Duration::from_millis(5)),
            Err(e) => return Err(Error(format!("the transform: {e}"))),
        }
    };
    let _ = writer.join();
    let bytes = reader.join().unwrap_or_default();
    let Some(status) = status else {
        return Err(Error(format!(
            "the transform took longer than {} s and was stopped",
            limits.wall.as_secs()
        )));
    };
    if bytes.len() > limits.output {
        return Err(Error("the transform's output is over 512 KiB".into()));
    }
    match serde_json::from_slice::<Reply>(&bytes) {
        Ok(Reply::Ok(value)) => Ok(value),
        Ok(Reply::Error(error)) => Err(Error(error)),
        Err(_) if status.success() => Err(Error("the transform gave no reply".into())),
        Err(_) => Err(Error(format!(
            "the transform stopped ({status}): it ran out of time or memory, or crashed"
        ))),
    }
}

#[cfg(unix)]
fn limit_self() {
    let get = |name: &str, default: u64| {
        std::env::var(name)
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(default)
    };
    let defaults = Limits::default();
    let cpu = get("LUMA_TRANSFORM_CPU", defaults.cpu_seconds);
    let memory = get("LUMA_TRANSFORM_MEMORY", defaults.memory);
    for (resource, value) in [
        (libc::RLIMIT_CPU, cpu),
        (libc::RLIMIT_AS, memory),
        (libc::RLIMIT_CORE, 0),
    ] {
        let limit = libc::rlimit {
            rlim_cur: value as libc::rlim_t,
            rlim_max: value as libc::rlim_t,
        };
        // macOS does not enforce RLIMIT_AS; the parent's wall clock still applies.
        unsafe {
            libc::setrlimit(resource, &limit);
        }
    }
}

#[cfg(not(unix))]
fn limit_self() {}

/// The child: limits itself, reads a job from stdin, writes one reply to stdout and exits.
pub fn child_main() -> ! {
    limit_self();
    let mut input = Vec::new();
    let reply = match std::io::stdin()
        .take(8 * 1024 * 1024)
        .read_to_end(&mut input)
        .map_err(|e| Error(e.to_string()))
        .and_then(|_| serde_json::from_slice::<Job>(&input).map_err(|e| Error(e.to_string())))
        .and_then(|job| jq::run(&job.program, job.input, &job.vars))
    {
        Ok(value) => Reply::Ok(value),
        Err(error) => Reply::Error(error.0),
    };
    let failed = matches!(reply, Reply::Error(_));
    let mut stdout = std::io::stdout().lock();
    let _ = serde_json::to_writer(&mut stdout, &reply);
    let _ = stdout.flush();
    std::process::exit(i32::from(failed));
}
