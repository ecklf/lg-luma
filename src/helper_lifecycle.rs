//! Ordinary app lifecycle on the app-named connection already owned by SDL/libhelpers.
//! Never LSRegister the same name a second time. The context lives until process exit,
//! like SDL's own lifecycle context, and its callback only queues work for the SDL thread.
use std::{
    ffi::{CStr, c_char, c_int, c_ulong, c_void},
    panic::{AssertUnwindSafe, catch_unwind},
    ptr,
};

use serde_json::Value;

// ABI used by the pinned SDL-webOS src/core/webos/SDL_webos_helpers_sym.h.
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
type Message = unsafe extern "C" fn(*mut c_void) -> *const c_char;

struct Sink {
    message: Message,
    on_message: Box<dyn FnMut(Value) + Send>,
}

extern "C" fn receive(_handle: *mut c_void, reply: *mut c_void, context: *mut Context) -> c_int {
    if reply.is_null() || context.is_null() {
        return 1;
    }
    // SAFETY: libhelpers supplies our live context on its callback thread. Both it and
    // the sink remain allocated for the process lifetime after successful submission.
    unsafe {
        if (*context).userdata.is_null() {
            return 1;
        }
        let sink = &mut *(*context).userdata.cast::<Sink>();
        let payload = (sink.message)(reply);
        if payload.is_null() {
            return 1;
        }
        let bytes = CStr::from_ptr(payload).to_bytes();
        if bytes.len() <= 2 * 1024 * 1024
            && let Ok(value) = serde_json::from_slice(bytes)
            && catch_unwind(AssertUnwindSafe(|| (sink.on_message)(value))).is_err()
        {
            eprintln!("Luma lifecycle: libhelpers callback panicked");
        }
    }
    1
}

fn submit(
    call: Call,
    message: Message,
    on_message: impl FnMut(Value) + Send + 'static,
) -> Result<(), String> {
    let sink = Box::into_raw(Box::new(Sink {
        message,
        on_message: Box::new(on_message),
    }));
    let context = Box::into_raw(Box::new(Context {
        callback: receive,
        userdata: sink.cast(),
        unknown: ptr::null_mut(),
        multiple: 1,
        public: 1,
        token: 0,
    }));
    let payload =
        std::ffi::CString::new(serde_json::json!({"id": crate::platform::APP_ID}).to_string())
            .expect("app ID JSON has no NUL");
    // SAFETY: the resolved function uses the pinned helper ABI. Strings live through
    // submission and the context/sink stay alive for every later asynchronous reply.
    let result = unsafe {
        call(
            c"luna://com.webos.applicationManager/registerApp".as_ptr(),
            payload.as_ptr(),
            context,
        )
    };
    if result != 0 {
        // SAFETY: a refused submission retains no callback, matching SDL's helper calls.
        unsafe {
            drop(Box::from_raw(context));
            drop(Box::from_raw(sink));
        }
        return Err(format!(
            "libhelpers registerApp submission failed ({result})"
        ));
    }
    Ok(())
}

/// Submit without waiting for SAM, so a missing acknowledgement cannot block first paint.
#[cfg(all(feature = "webos", target_os = "linux", target_arch = "arm"))]
pub fn subscribe(on_message: impl FnMut(Value) + Send + 'static) -> Result<(), String> {
    // SAFETY: use the same library name as pinned SDL. Loading it again acquires a
    // reference to that library, not a second helper instance or LS2 connection.
    let library = unsafe { sdl2::sys::SDL_LoadObject(c"libhelpers.so.2".as_ptr()) };
    if library.is_null() {
        return Err(format!("load libhelpers: {}", sdl2::get_error()));
    }
    // SAFETY: symbols are resolved from libhelpers using their documented ABI.
    let call = unsafe { sdl2::sys::SDL_LoadFunction(library, c"HLunaServiceCall".as_ptr()) };
    let message = unsafe { sdl2::sys::SDL_LoadFunction(library, c"HLunaServiceMessage".as_ptr()) };
    if call.is_null() || message.is_null() {
        let error = sdl2::get_error();
        // SAFETY: no subscription has been submitted using this library reference.
        unsafe { sdl2::sys::SDL_UnloadObject(library) };
        return Err(format!("resolve libhelpers lifecycle API: {error}"));
    }
    // Keep this library reference alive with the process-lifetime subscription, even
    // during SDL shutdown. A callback must never point into an unloaded library.
    // SAFETY: the concrete signatures match the pinned SDL helper declarations.
    let call = unsafe { std::mem::transmute::<*mut c_void, Call>(call) };
    let message = unsafe { std::mem::transmute::<*mut c_void, Message>(message) };
    submit(call, message, on_message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    unsafe extern "C" fn message(reply: *mut c_void) -> *const c_char {
        reply.cast()
    }

    unsafe extern "C" fn call(
        uri: *const c_char,
        payload: *const c_char,
        context: *mut Context,
    ) -> c_int {
        // SAFETY: submit passes live NUL-terminated strings and an initialized context.
        unsafe {
            assert_eq!(
                CStr::from_ptr(uri),
                c"luna://com.webos.applicationManager/registerApp"
            );
            let payload: Value =
                serde_json::from_slice(CStr::from_ptr(payload).to_bytes()).unwrap();
            assert_eq!(payload["id"], crate::platform::APP_ID);
            assert_eq!((*context).multiple, 1);
            assert_eq!((*context).public, 1);
            for payload in [
                c"{\"event\":\"registered\"}",
                c"not json",
                c"{\"event\":\"relaunch\"}",
                c"{\"event\":\"close\"}",
            ] {
                assert_eq!(
                    ((*context).callback)(
                        ptr::null_mut(),
                        payload.as_ptr().cast_mut().cast(),
                        context
                    ),
                    1
                );
            }
        }
        0
    }

    #[test]
    fn shared_helper_submission_uses_app_identity_and_delivers_lifecycle_replies() {
        let replies = Arc::new(Mutex::new(Vec::new()));
        let received = replies.clone();
        submit(call, message, move |reply| {
            received.lock().unwrap().push(reply)
        })
        .unwrap();
        let replies = replies.lock().unwrap();
        assert_eq!(replies.len(), 3);
        assert_eq!(replies[0]["event"], "registered");
        assert_eq!(replies[1]["event"], "relaunch");
        assert_eq!(replies[2]["event"], "close");
    }

    #[test]
    fn refused_submission_is_an_error_without_invoking_the_sink() {
        unsafe extern "C" fn refuse(_: *const c_char, _: *const c_char, _: *mut Context) -> c_int {
            -1
        }
        assert!(submit(refuse, message, |_| panic!("no callback expected")).is_err());
    }

    #[test]
    fn null_callback_arguments_are_ignored() {
        assert_eq!(
            receive(ptr::null_mut(), ptr::null_mut(), ptr::null_mut()),
            1
        );
    }
}
