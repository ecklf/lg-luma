//! Early Home helper for the webOS 5/6 executable Home method.
//!
//! LG's Homebrew init.d hooks run only ~50 s into a cold boot, so `home.sh boot` mounts Luma over
//! LG's Home executable long after the TV showed Live TV and LG's Home. ls-hubd, however, reads
//! the LS2 definitions in /var/luna-service2 at core-boot-done (snapshot-resume-jobs:
//! `ls-control scan-volatile-dirs`, ~17 s) and starts dynamic services as root. `home.sh enable`
//! registers this program there as `com.webos.service.lumahome`; the compositor key filter
//! (`home-filter.js`) calls its one method while the TV boots and opens Home once it succeeded.
//!
//! `/prepare` runs `home.sh early` (mount, marker, stop LG's Home; nothing is launched) and
//! answers `{"returnValue": bool, "exitCode": n}`. It takes no parameters and answers only the
//! key filter. The process exits 15 s after the last request; a new call starts it again.

// The SDK's libc lacks getauxval, which Rust's startup needs (as for the main binary).
#[cfg(all(feature = "webos", target_os = "linux", target_arch = "arm"))]
#[path = "../webos-compat.rs"]
mod webos_compat;

#[cfg(all(feature = "webos", target_os = "linux", target_arch = "arm"))]
mod service {
    use libc::{c_char, c_int, c_uint, c_ulong, c_void};
    use std::{
        cell::Cell,
        ffi::{CStr, CString},
        process::{Command, Stdio},
        ptr,
    };

    const NAME: &CStr = c"com.webos.service.lumahome";
    const CALLER: &[u8] = b"com.webos.starfish.keyfilters";
    const SCRIPT: &str = "/var/lib/luma-home/home.sh";
    const IDLE_SECONDS: c_uint = 15;

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

    type Method = extern "C" fn(*mut c_void, *mut c_void, *mut c_void) -> bool;

    #[repr(C)]
    struct LsMethod {
        name: *const c_char,
        function: Option<Method>,
        flags: c_int,
    }

    #[link(name = "luna-service2")]
    unsafe extern "C" {
        fn LSErrorInit(error: *mut Error) -> bool;
        fn LSErrorFree(error: *mut Error);
        fn LSRegister(name: *const c_char, handle: *mut *mut c_void, error: *mut Error) -> bool;
        fn LSRegisterCategory(
            handle: *mut c_void,
            category: *const c_char,
            methods: *const LsMethod,
            signals: *const c_void,
            properties: *const c_void,
            error: *mut Error,
        ) -> bool;
        fn LSGmainAttach(handle: *mut c_void, main_loop: *mut c_void, error: *mut Error) -> bool;
        fn LSUnregister(handle: *mut c_void, error: *mut Error) -> bool;
        fn LSMessageGetSenderServiceName(message: *mut c_void) -> *const c_char;
        fn LSMessageReply(
            handle: *mut c_void,
            message: *mut c_void,
            payload: *const c_char,
            error: *mut Error,
        ) -> bool;
    }

    #[link(name = "glib-2.0")]
    unsafe extern "C" {
        fn g_main_loop_new(context: *mut c_void, is_running: c_int) -> *mut c_void;
        fn g_main_loop_run(main_loop: *mut c_void);
        fn g_main_loop_quit(main_loop: *mut c_void);
        fn g_main_loop_unref(main_loop: *mut c_void);
        fn g_timeout_add_seconds(
            interval: c_uint,
            function: extern "C" fn(*mut c_void) -> c_int,
            data: *mut c_void,
        ) -> c_uint;
        fn g_source_remove(id: c_uint) -> c_int;
    }

    thread_local! {
        static MAIN_LOOP: Cell<*mut c_void> = const { Cell::new(ptr::null_mut()) };
        static IDLE: Cell<c_uint> = const { Cell::new(0) };
        // Exit code of `home.sh early`; one run per process lifetime, later calls get its result.
        static RESULT: Cell<Option<i32>> = const { Cell::new(None) };
    }

    struct LsError(Error);

    impl LsError {
        fn new() -> Self {
            let mut error = Error {
                code: 0,
                message: ptr::null_mut(),
                file: ptr::null(),
                line: 0,
                function: ptr::null(),
                padding: ptr::null_mut(),
                magic: 0,
            };
            // SAFETY: a valid, writable error struct.
            unsafe { LSErrorInit(&mut error) };
            Self(error)
        }

        fn describe(&self, step: &str) -> String {
            let message = if self.0.message.is_null() {
                "unknown error".into()
            } else {
                // SAFETY: LS2 sets a NUL-terminated message on failure.
                unsafe { CStr::from_ptr(self.0.message) }
                    .to_string_lossy()
                    .into_owned()
            };
            format!("luma-early: {step}: {message}")
        }
    }

    impl Drop for LsError {
        fn drop(&mut self) {
            // SAFETY: initialized by LSErrorInit; freeing an empty error is allowed.
            unsafe { LSErrorFree(&mut self.0) };
        }
    }

    extern "C" fn quit(_: *mut c_void) -> c_int {
        IDLE.with(|idle| idle.set(0));
        // SAFETY: the loop outlives every source attached to its default context.
        MAIN_LOOP.with(|main_loop| unsafe { g_main_loop_quit(main_loop.get()) });
        0
    }

    fn keep_alive() {
        IDLE.with(|idle| {
            if idle.get() != 0 {
                // SAFETY: the id came from g_timeout_add_seconds and has not fired yet.
                unsafe { g_source_remove(idle.get()) };
            }
            // SAFETY: a plain function callback without data.
            idle.set(unsafe { g_timeout_add_seconds(IDLE_SECONDS, quit, ptr::null_mut()) });
        });
    }

    fn reply(handle: *mut c_void, message: *mut c_void, payload: &str) {
        let payload = CString::new(payload).expect("reply payloads contain no NUL");
        let mut error = LsError::new();
        // SAFETY: handle and message are the ones LS2 passed to this method call.
        if !unsafe { LSMessageReply(handle, message, payload.as_ptr(), &mut error.0) } {
            eprintln!("{}", error.describe("reply"));
        }
    }

    fn early() -> i32 {
        Command::new("/bin/sh")
            .args([SCRIPT, "early"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .ok()
            .and_then(|status| status.code())
            .unwrap_or(127)
    }

    extern "C" fn prepare(handle: *mut c_void, message: *mut c_void, _: *mut c_void) -> bool {
        keep_alive();
        // SAFETY: LS2 returns the authenticated sender name of this message, or null.
        let sender = unsafe { LSMessageGetSenderServiceName(message) };
        if sender.is_null() || unsafe { CStr::from_ptr(sender) }.to_bytes() != CALLER {
            reply(
                handle,
                message,
                r#"{"returnValue":false,"errorText":"Caller denied"}"#,
            );
            return true;
        }
        let code = RESULT.with(|result| {
            let code = result.get().unwrap_or_else(early);
            result.set(Some(code));
            code
        });
        reply(
            handle,
            message,
            &format!(r#"{{"returnValue":{},"exitCode":{code}}}"#, code == 0),
        );
        true
    }

    pub fn run() -> i32 {
        let methods = [
            LsMethod {
                name: c"prepare".as_ptr(),
                function: Some(prepare),
                flags: 0,
            },
            LsMethod {
                name: ptr::null(),
                function: None,
                flags: 0,
            },
        ];
        let mut handle = ptr::null_mut();
        let mut error = LsError::new();
        // SAFETY: GLib's default context; the loop is released below.
        let main_loop = unsafe { g_main_loop_new(ptr::null_mut(), 0) };
        MAIN_LOOP.with(|cell| cell.set(main_loop));
        // SAFETY: valid strings, the methods array outlives the registration, and the handle is
        // only used on this thread.
        let registered = unsafe {
            LSRegister(NAME.as_ptr(), &mut handle, &mut error.0)
                && LSRegisterCategory(
                    handle,
                    c"/".as_ptr(),
                    methods.as_ptr(),
                    ptr::null(),
                    ptr::null(),
                    &mut error.0,
                )
                && LSGmainAttach(handle, main_loop, &mut error.0)
        };
        let code = if registered {
            keep_alive();
            // SAFETY: runs until the idle timeout quits it.
            unsafe { g_main_loop_run(main_loop) };
            0
        } else {
            eprintln!("{}", error.describe("registration"));
            1
        };
        if !handle.is_null() {
            let mut error = LsError::new();
            // SAFETY: unregister the handle registered above before releasing the loop.
            if !unsafe { LSUnregister(handle, &mut error.0) } {
                eprintln!("{}", error.describe("unregistration"));
            }
        }
        // SAFETY: created above; no source refers to it any more.
        unsafe { g_main_loop_unref(main_loop) };
        code
    }
}

#[cfg(all(feature = "webos", target_os = "linux", target_arch = "arm"))]
fn main() {
    std::process::exit(service::run());
}

#[cfg(not(all(feature = "webos", target_os = "linux", target_arch = "arm")))]
fn main() {
    eprintln!("luma-early only runs on a webOS TV build");
    std::process::exit(1);
}
