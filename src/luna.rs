//! One bounded LS2 round trip on the platform worker's own GLib context.
//! The hub sees the installed native executable. LSCallFromApplication (the
//! luna-send-pub -a path) instead requires an impersonation privilege we lack.
//! `subscribe_as_app` is the one exception: as root, it holds a subscription
//! through the system bus `luna-send -a`, which may act for an app ID.
use libc::{c_char, c_int, c_ulong, c_void};
use serde_json::Value;
use std::{
    ffi::{CStr, CString},
    io::{BufRead, BufReader},
    os::unix::process::CommandExt,
    panic::{AssertUnwindSafe, catch_unwind},
    process::{Child, Command, Stdio},
    ptr,
    sync::{Arc, Mutex, mpsc},
    thread,
    time::{Duration, Instant},
};

const MAX_REPLY: usize = 2 * 1024 * 1024;

// Layout from the NDK's public luna-service2/lunaservice.h.
#[repr(C)]
struct Error {
    code: c_int,
    message: *mut c_char,
    file: *const c_char,
    line: c_int,
    function: *const c_char,
    padding: *mut c_void,
    magic: c_ulong,
}

#[link(name = "luna-service2")]
unsafe extern "C" {
    fn LSErrorInit(error: *mut Error) -> bool;
    fn LSErrorFree(error: *mut Error);
    fn LSRegister(name: *const c_char, handle: *mut *mut c_void, error: *mut Error) -> bool;
    fn LSUnregister(handle: *mut c_void, error: *mut Error) -> bool;
    fn LSGmainContextAttach(handle: *mut c_void, context: *mut c_void, error: *mut Error) -> bool;
    fn LSCallOneReply(
        handle: *mut c_void,
        uri: *const c_char,
        payload: *const c_char,
        callback: extern "C" fn(*mut c_void, *mut c_void, *mut c_void) -> bool,
        userdata: *mut c_void,
        token: *mut c_ulong,
        error: *mut Error,
    ) -> bool;
    fn LSCall(
        handle: *mut c_void,
        uri: *const c_char,
        payload: *const c_char,
        callback: extern "C" fn(*mut c_void, *mut c_void, *mut c_void) -> bool,
        userdata: *mut c_void,
        token: *mut c_ulong,
        error: *mut Error,
    ) -> bool;
    fn LSCallCancel(handle: *mut c_void, token: c_ulong, error: *mut Error) -> bool;
    fn LSMessageGetPayload(message: *mut c_void) -> *const c_char;
}

#[link(name = "glib-2.0")]
unsafe extern "C" {
    fn g_main_context_new() -> *mut c_void;
    fn g_main_context_iteration(context: *mut c_void, may_block: c_int) -> c_int;
    fn g_main_context_unref(context: *mut c_void);
}

impl Error {
    fn new() -> Self {
        // SAFETY: the public error structure is zero-initializable; LS2 then
        // initializes its cookie before any other error API uses it.
        unsafe {
            let mut error = std::mem::zeroed();
            LSErrorInit(&mut error);
            error
        }
    }

    fn describe(&self, stage: &str) -> String {
        let detail = if self.message.is_null() {
            String::new()
        } else {
            // SAFETY: LS2 owns this NUL-terminated string until LSErrorFree.
            unsafe { CStr::from_ptr(self.message) }
                .to_string_lossy()
                .chars()
                .take(240)
                .collect()
        };
        format!("LS2 {stage} failed ({}): {detail}", self.code)
    }
}

impl Drop for Error {
    fn drop(&mut self) {
        // SAFETY: every Error was initialized and is freed exactly once.
        unsafe { LSErrorFree(self) };
    }
}

#[derive(Default)]
struct Reply {
    result: Option<Result<Value, String>>,
}

extern "C" fn receive(_handle: *mut c_void, message: *mut c_void, userdata: *mut c_void) -> bool {
    if message.is_null() || userdata.is_null() {
        return true;
    }
    // SAFETY: LS2 invokes this only while this connection's context is iterated
    // on the worker. The heap slot outlives cancellation/unregistration.
    unsafe {
        let reply = &mut *userdata.cast::<Reply>();
        let payload = LSMessageGetPayload(message);
        reply.result = Some(if payload.is_null() {
            Err("Luna reply has no payload".into())
        } else {
            let size = libc::strnlen(payload, MAX_REPLY + 1);
            if size > MAX_REPLY {
                Err("TV response is too large".into())
            } else {
                let bytes = std::slice::from_raw_parts(payload.cast::<u8>(), size);
                serde_json::from_slice(bytes).map_err(|error| format!("Invalid Luna JSON: {error}"))
            }
        });
    }
    true
}

struct Connection {
    handle: *mut c_void,
    context: *mut c_void,
    reply: Option<Box<Reply>>,
}

impl Connection {
    fn open() -> Result<Self, String> {
        Self::open_named(None)
    }

    /// `name`: a service name the executable's LS2 role allows (sam identifies a registerApp
    /// caller by it); None registers anonymously.
    fn open_named(name: Option<&CStr>) -> Result<Self, String> {
        let mut error = Error::new();
        let mut connection = Self {
            handle: ptr::null_mut(),
            // SAFETY: GLib returns a new context owned by this connection.
            context: unsafe { g_main_context_new() },
            reply: None,
        };
        if connection.context.is_null() {
            return Err("Could not allocate a Luna event context".into());
        }
        // Anonymous registration still authenticates the executable and its LS2
        // role, and avoids colliding with SDL's own app-named registration.
        // SAFETY: output/error pointers are valid; no fake application ID is sent.
        let name = name.map_or(ptr::null(), CStr::as_ptr);
        if !unsafe { LSRegister(name, &mut connection.handle, &mut error) } {
            return Err(error.describe("registration"));
        }
        // SAFETY: the registered handle and private context stay alive together.
        if !unsafe { LSGmainContextAttach(connection.handle, connection.context, &mut error) } {
            return Err(error.describe("context attachment"));
        }
        Ok(connection)
    }

    fn request(&mut self, uri: &str, payload: &Value) -> Result<Value, String> {
        let uri = CString::new(uri).map_err(|_| "Invalid Luna URI")?;
        let payload = CString::new(payload.to_string()).map_err(|_| "Invalid Luna payload")?;
        let mut error = Error::new();
        let mut reply = Box::<Reply>::default();
        let userdata = ptr::from_mut(reply.as_mut()).cast();
        self.reply = Some(reply);
        let mut token = 0;
        // SAFETY: strings and the stable reply allocation outlive this request.
        if !unsafe {
            LSCallOneReply(
                self.handle,
                uri.as_ptr(),
                payload.as_ptr(),
                receive,
                userdata,
                &mut token,
                &mut error,
            )
        } {
            return Err(error.describe("call submission"));
        }
        let deadline = Instant::now() + Duration::from_millis(6500);
        while Instant::now() < deadline {
            // SAFETY: only this worker drives this private context. Iteration is
            // nonblocking, so a missing service cannot hold the UI or this loop.
            unsafe { g_main_context_iteration(self.context, 0) };
            if let Some(result) = self.reply.as_mut().and_then(|reply| reply.result.take()) {
                return result;
            }
            thread::sleep(Duration::from_millis(2));
        }
        let mut cancel_error = Error::new();
        // SAFETY: handle/token belong to this live request. Even a refused cancel
        // leaves its callback allocation alive until unregistering on drop.
        if !unsafe { LSCallCancel(self.handle, token, &mut cancel_error) } {
            eprintln!("{}", cancel_error.describe("timeout cancellation"));
        }
        Err("The TV service timed out".into())
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            let mut error = Error::new();
            // SAFETY: unregister before freeing the context or callback storage.
            if !unsafe { LSUnregister(self.handle, &mut error) } {
                eprintln!("{}", error.describe("unregistration"));
                // A refused unregister can retain the callback. Preserve its
                // context and heap slot rather than leave dangling pointers.
                std::mem::forget(self.reply.take());
                return;
            }
        }
        if !self.context.is_null() {
            // SAFETY: no live LS2 connection can now dispatch on this context.
            unsafe { g_main_context_unref(self.context) };
        }
    }
}

pub fn call(uri: &str, payload: &Value) -> Result<Value, String> {
    Connection::open()?.request(uri, payload)
}

type Sink = Box<dyn FnMut(Value) + Send>;

extern "C" fn deliver(_handle: *mut c_void, message: *mut c_void, userdata: *mut c_void) -> bool {
    if message.is_null() || userdata.is_null() {
        return true;
    }
    // SAFETY: userdata is the leaked sink of a subscription whose connection lives for the
    // rest of the process, and LS2 calls this only on that subscription's worker thread.
    unsafe {
        let payload = LSMessageGetPayload(message);
        if payload.is_null() {
            return true;
        }
        let size = libc::strnlen(payload, MAX_REPLY + 1);
        if size > MAX_REPLY {
            return true;
        }
        let bytes = std::slice::from_raw_parts(payload.cast::<u8>(), size);
        if let Ok(value) = serde_json::from_slice(bytes) {
            let sink = &mut *userdata.cast::<Sink>();
            // A panic must not unwind into LS2's C frames, which would abort the process.
            if catch_unwind(AssertUnwindSafe(|| sink(value))).is_err() {
                eprintln!("Luna subscription: handler panicked");
            }
        }
    }
    true
}

/// A subscription kept for the life of the process on its own thread and LS2 connection,
/// e.g. sam's registerApp lifecycle events. Each reply goes to `on_message` on that thread.
/// Returns once the service has answered the call: an error if the connection or call fails,
/// the first reply has `returnValue: false`, or no reply comes within `timeout`.
pub fn subscribe(
    name: Option<&str>,
    uri: &str,
    payload: &Value,
    timeout: Duration,
    mut on_message: impl FnMut(Value) + Send + 'static,
) -> Result<(), String> {
    let name = name
        .map(CString::new)
        .transpose()
        .map_err(|_| "Invalid Luna service name")?;
    let uri = CString::new(uri).map_err(|_| "Invalid Luna URI")?;
    let payload = CString::new(payload.to_string()).map_err(|_| "Invalid Luna payload")?;
    let (started, handshake) = mpsc::sync_channel::<Result<(), String>>(1);
    let mut first = Some(started.clone());
    let sink: Sink = Box::new(move |value: Value| {
        if let Some(started) = first.take() {
            let result = if value.get("returnValue").and_then(Value::as_bool) == Some(false) {
                Err(format!("Luna subscription refused: {value}"))
            } else {
                Ok(())
            };
            let _ = started.send(result);
        }
        on_message(value);
    });
    thread::Builder::new()
        .name("luna-subscription".into())
        .spawn(move || {
            let connection = match Connection::open_named(name.as_deref()) {
                Ok(connection) => connection,
                Err(error) => {
                    return drop(started.send(Err(format!("Luna subscription: {error}"))));
                }
            };
            let userdata = Box::into_raw(Box::new(sink));
            let mut error = Error::new();
            let mut token = 0;
            // SAFETY: strings outlive the call; once submitted, the sink is never freed.
            if !unsafe {
                LSCall(
                    connection.handle,
                    uri.as_ptr(),
                    payload.as_ptr(),
                    deliver,
                    userdata.cast(),
                    &mut token,
                    &mut error,
                )
            } {
                // SAFETY: a refused call never registered the callback, so nothing else holds it.
                drop(unsafe { Box::from_raw(userdata) });
                let message = format!("Luna subscription: {}", error.describe("call submission"));
                return drop(started.send(Err(message)));
            }
            drop(started);
            loop {
                // SAFETY: this thread alone drives the connection's private context.
                unsafe { g_main_context_iteration(connection.context, 1) };
            }
        })
        .map_err(|error| error.to_string())?;
    match handshake.recv_timeout(timeout) {
        Ok(result) => result,
        Err(mpsc::RecvTimeoutError::Timeout) => Err("Luna subscription: no reply".into()),
        Err(mpsc::RecvTimeoutError::Disconnected) => Err("Luna subscription ended".into()),
    }
}

/// A subscription held by a root `luna-send -a` child; see `subscribe_as_app`.
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

/// Like `subscribe`, but the call is made as `app_id` by a root `luna-send -a` child, which the
/// system bus lets act for an app. For a Home manifest staged under LG's app-ID based role
/// (webOS 10/11), where Luma's own connection cannot be that app. Requires root.
///
/// Does not wait for the service: each reply goes to `on_message` on a reader thread. No first
/// reply within `timeout`, a refused first reply, a later `returnValue: false` or the child's
/// output ending all end the child, and then `on_end` runs once with whether the first reply was
/// accepted. The child gets SIGKILL when the calling thread ends (call this from the main
/// thread), so the subscription never outlives Luma.
pub fn subscribe_as_app(
    app_id: &str,
    uri: &str,
    payload: &Value,
    timeout: Duration,
    mut on_message: impl FnMut(Value) + Send + 'static,
    on_end: impl FnOnce(bool) + Send + 'static,
) -> Result<AppSubscription, String> {
    // SAFETY: getpid has no preconditions.
    let parent = unsafe { libc::getpid() };
    let mut command = Command::new("/usr/bin/luna-send");
    command
        .args(["-a", app_id, "-i", uri, &payload.to_string()])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
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
    let stdout = child
        .stdout
        .take()
        .ok_or("Luna app subscription: no output")?;
    let child = Arc::new(Mutex::new(Some(child)));
    let (answered, handshake) = mpsc::sync_channel::<()>(1);
    let waiting = Arc::clone(&child);
    thread::Builder::new()
        .name("luna-app-handshake".into())
        .spawn(move || {
            // A disconnect means the reader already finished; only silence ends the child here.
            if let Err(mpsc::RecvTimeoutError::Timeout) = handshake.recv_timeout(timeout) {
                eprintln!("Luna app subscription: no reply");
                end_child(&waiting);
            }
        })
        .map_err(|error| error.to_string())?;
    let reader = Arc::clone(&child);
    thread::Builder::new()
        .name("luna-app-subscription".into())
        .spawn(move || {
            let mut answered = Some(answered);
            let mut registered = false;
            // luna-send -i prints each reply as one compact JSON line. Skip anything else.
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
        })
        .map_err(|error| error.to_string())?;
    Ok(AppSubscription { child })
}
