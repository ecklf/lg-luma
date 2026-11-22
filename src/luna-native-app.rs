//! Use libhelpers' existing app-named handle, which SDL owns. Never register a
//! second handle under the app ID or impersonate an app with a shell process.
use super::{MAX_REPLY, c_char, c_int, c_ulong, c_void};
use serde_json::Value;
use std::{
    ffi::CString,
    ptr,
    sync::{
        OnceLock,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

static READY: AtomicBool = AtomicBool::new(false);
static PENDING: AtomicUsize = AtomicUsize::new(0);
const MAX_PENDING: usize = 8;

// Public ABI in webos-helpers/libhelpers.h and the pinned SDL-webOS
// SDL_webos_helpers_sym.h. The context must remain alive until its callback.
#[repr(C)]
struct Context {
    callback: extern "C" fn(*mut c_void, *mut c_void, *mut Context) -> c_int,
    userdata: *mut c_void,
    unknown: *mut c_void,
    multiple: c_int,
    public: c_int,
    token: c_ulong,
}

type Call = unsafe extern "C" fn(*const c_char, *const c_char, *mut Context) -> c_int;
struct Helpers {
    call: Call,
}

fn helpers() -> Result<&'static Helpers, String> {
    static HELPERS: OnceLock<Result<Helpers, String>> = OnceLock::new();
    HELPERS
        .get_or_init(|| {
            // SAFETY: use only an already-loaded libhelpers. Opening the same DSO
            // reuses SDL's singleton handle, not a fresh library or registration.
            let mut library = unsafe {
                libc::dlopen(
                    c"libhelpers.so.2".as_ptr(),
                    libc::RTLD_NOW | libc::RTLD_NOLOAD,
                )
            };
            if library.is_null() {
                // SAFETY: some firmware exposes only the unversioned soname.
                library = unsafe {
                    libc::dlopen(
                        c"libhelpers.so".as_ptr(),
                        libc::RTLD_NOW | libc::RTLD_NOLOAD,
                    )
                };
            }
            if library.is_null() {
                return Err("SDL's libhelpers is unavailable".into());
            }
            // SAFETY: this is the documented public function with the ABI above.
            let symbol = unsafe { libc::dlsym(library, c"HLunaServiceCall".as_ptr()) };
            if symbol.is_null() {
                return Err("libhelpers has no HLunaServiceCall".into());
            }
            // Retain the DSO reference for the process lifetime so callbacks and
            // the function pointer cannot outlive it.
            Ok(Helpers {
                // SAFETY: the symbol has the signature declared in the public header.
                call: unsafe { std::mem::transmute::<*mut c_void, Call>(symbol) },
            })
        })
        .as_ref()
        .map_err(Clone::clone)
}

struct Pending {
    context: Context,
    sender: mpsc::SyncSender<Result<Value, String>>,
    // libhelpers may keep the submitted strings until delivery.
    _uri: CString,
    _payload: CString,
}

impl Drop for Pending {
    fn drop(&mut self) {
        PENDING.fetch_sub(1, Ordering::AcqRel);
    }
}

extern "C" fn receive(_handle: *mut c_void, message: *mut c_void, context: *mut Context) -> c_int {
    if context.is_null() {
        return 0;
    }
    // SAFETY: one-shot delivery owns this heap allocation. libhelpers permits
    // freeing the HContext in its callback, as SDL's justCallCallback does.
    let pending = unsafe { Box::from_raw((*context).userdata.cast::<Pending>()) };
    let result = if message.is_null() {
        Err("Home Chooser reply has no message".into())
    } else {
        // SAFETY: libhelpers supplies a live LSMessage for this callback only.
        let payload = unsafe { super::LSMessageGetPayload(message) };
        if payload.is_null() {
            Err("Home Chooser reply has no payload".into())
        } else {
            // SAFETY: the LS2 payload is NUL-terminated and borrowed for this callback.
            let size = unsafe { libc::strnlen(payload, MAX_REPLY + 1) };
            if size > MAX_REPLY {
                Err("Home Chooser reply exceeds 2 MiB".into())
            } else {
                // SAFETY: size was checked against the payload's terminating NUL.
                let bytes = unsafe { std::slice::from_raw_parts(payload.cast::<u8>(), size) };
                serde_json::from_slice(bytes).map_err(|e| format!("Invalid Home Chooser JSON: {e}"))
            }
        }
    };
    let _ = pending.sender.send(result);
    0
}

/// SDL has initialized libhelpers and registered the installed app's lifecycle.
pub fn ready() {
    READY.store(true, Ordering::Release);
}

/// Blocking on a platform/artwork worker only. Delivery uses libhelpers' normal
/// event dispatch, not a second connection or a worker driving SDL's context.
pub fn call(uri: &str, payload: &Value) -> Result<Value, String> {
    let deadline = Instant::now() + Duration::from_secs(70);
    // The initial inventory worker starts before video initialization. Do not
    // touch libhelpers until SDL has established the app-named connection.
    while !READY.load(Ordering::Acquire) {
        if Instant::now() >= deadline {
            return Err("SDL native app registration is not ready".into());
        }
        thread::sleep(Duration::from_millis(5));
    }
    let helpers = helpers()?;
    let uri = CString::new(uri).map_err(|_| "Invalid Home Chooser URI")?;
    let payload = CString::new(payload.to_string()).map_err(|_| "Invalid Home Chooser payload")?;
    PENDING
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
            (n < MAX_PENDING).then_some(n + 1)
        })
        .map_err(|_| "Too many pending Home Chooser calls")?;
    let (sender, receiver) = mpsc::sync_channel(1);
    let mut pending = Box::new(Pending {
        context: Context {
            callback: receive,
            userdata: ptr::null_mut(),
            unknown: ptr::null_mut(),
            multiple: 0,
            public: 1,
            token: 0,
        },
        sender,
        _uri: uri,
        _payload: payload,
    });
    pending.context.userdata = ptr::from_mut(pending.as_mut()).cast();
    let raw = Box::into_raw(pending);
    // SAFETY: the heap context and strings survive submission and any delayed
    // reply. libhelpers performs its own dispatch on SDL's existing handle.
    let result = unsafe {
        (helpers.call)(
            (*raw)._uri.as_ptr(),
            (*raw)._payload.as_ptr(),
            &mut (*raw).context,
        )
    };
    if result != 0 {
        // SAFETY: a refused submission registered no callback.
        drop(unsafe { Box::from_raw(raw) });
        return Err(format!("libhelpers refused Home Chooser call ({result})"));
    }
    // On timeout do not free a context the library might still call. A late
    // reply frees it. Permanently lost replies occupy at most MAX_PENDING slots,
    // bounding retained memory and making subsequent callers fall back safely.
    receiver
        .recv_timeout(deadline.saturating_duration_since(Instant::now()))
        .map_err(|_| "Home Chooser call timed out".to_string())?
}
