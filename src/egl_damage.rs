//! Buffer age and damage regions for partial redraw on EGL window surfaces.
//! EGL_KHR_partial_update (webOS Mali) also tells the driver which tiles to preserve, so a
//! small change costs only those tiles; EGL_EXT_buffer_age alone still allows redrawing less.
//! Without either, or without a current EGL context (GLX, CGL, WGL), every frame is drawn in
//! full.
use std::ffi::{CStr, c_char, c_void};

type Display = *mut c_void;
type Surface = *mut c_void;
const EGL_EXTENSIONS: i32 = 0x3055;
const EGL_DRAW: i32 = 0x3059;
const EGL_BUFFER_AGE: i32 = 0x313D;
type QuerySurface = unsafe extern "C" fn(Display, Surface, i32, *mut i32) -> u32;
type SetDamage = unsafe extern "C" fn(Display, Surface, *const i32, i32) -> u32;

#[cfg(target_os = "linux")]
unsafe extern "C" {
    fn dlopen(filename: *const c_char, flags: std::ffi::c_int) -> *mut c_void;
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
}

/// The EGL library SDL already loaded, without loading one. Not loaded: the context isn't EGL.
#[cfg(target_os = "linux")]
fn loaded_egl() -> Option<*mut c_void> {
    const RTLD_LAZY: std::ffi::c_int = 0x1;
    const RTLD_NOLOAD: std::ffi::c_int = 0x4;
    [c"libEGL.so.1", c"libEGL.so"].into_iter().find_map(|name| {
        // SAFETY: a NUL-terminated name; RTLD_NOLOAD only looks up an already loaded library.
        let handle = unsafe { dlopen(name.as_ptr(), RTLD_LAZY | RTLD_NOLOAD) };
        (!handle.is_null()).then_some(handle)
    })
}

pub struct PartialUpdate {
    display: Display,
    surface: Surface,
    query_surface: QuerySurface,
    set_damage: Option<SetDamage>,
}

impl PartialUpdate {
    /// For the current context's draw surface; call after the GL context is current.
    #[cfg(not(target_os = "linux"))]
    pub fn load() -> Option<Self> {
        None
    }

    /// For the current context's draw surface; call after the GL context is current.
    #[cfg(target_os = "linux")]
    pub fn load() -> Option<Self> {
        let library = loaded_egl()?;
        // Symbols come from the EGL library itself, never from the GL backend's resolver:
        // glXGetProcAddress returns non-null dispatch stubs even for unknown egl* names.
        let symbol = |name: &CStr| {
            // SAFETY: a loaded library handle and a NUL-terminated symbol name.
            let address = unsafe { dlsym(library, name.as_ptr()) };
            (!address.is_null()).then_some(address)
        };
        // SAFETY: the addresses are EGL 1.4 entry points with these C signatures, from libEGL.
        unsafe {
            let current_context: unsafe extern "C" fn() -> *mut c_void =
                std::mem::transmute(symbol(c"eglGetCurrentContext")?);
            let current_display: unsafe extern "C" fn() -> Display =
                std::mem::transmute(symbol(c"eglGetCurrentDisplay")?);
            let current_surface: unsafe extern "C" fn(i32) -> Surface =
                std::mem::transmute(symbol(c"eglGetCurrentSurface")?);
            let query_string: unsafe extern "C" fn(Display, i32) -> *const c_char =
                std::mem::transmute(symbol(c"eglQueryString")?);
            let query_surface: QuerySurface = std::mem::transmute(symbol(c"eglQuerySurface")?);
            let get_proc_address: unsafe extern "C" fn(*const c_char) -> *mut c_void =
                std::mem::transmute(symbol(c"eglGetProcAddress")?);
            // libEGL can be loaded beside a GLX context (libglvnd); then no EGL context is current.
            if current_context().is_null() {
                return None;
            }
            let display = current_display();
            let surface = current_surface(EGL_DRAW);
            if display.is_null() || surface.is_null() {
                return None;
            }
            let extensions = query_string(display, EGL_EXTENSIONS);
            if extensions.is_null() {
                return None;
            }
            let extensions = CStr::from_ptr(extensions).to_string_lossy();
            let has = |name: &str| extensions.split_whitespace().any(|item| item == name);
            let set_damage = if has("EGL_KHR_partial_update") {
                let address = get_proc_address(c"eglSetDamageRegionKHR".as_ptr());
                (!address.is_null()).then(|| std::mem::transmute::<*mut c_void, SetDamage>(address))
            } else {
                None
            };
            if set_damage.is_none() && !has("EGL_EXT_buffer_age") {
                return None;
            }
            Some(Self {
                display,
                surface,
                query_surface,
                set_damage,
            })
        }
    }

    pub fn describe(&self) -> &'static str {
        if self.set_damage.is_some() {
            "EGL_KHR_partial_update"
        } else {
            "EGL_EXT_buffer_age"
        }
    }

    /// Frames since the next back buffer was presented; 0 when its contents are undefined.
    /// Query before drawing the frame (and before setting its damage).
    pub fn buffer_age(&self) -> u32 {
        let mut age = 0;
        // SAFETY: display and surface are the current context's, alive for the window.
        let ok =
            unsafe { (self.query_surface)(self.display, self.surface, EGL_BUFFER_AGE, &mut age) };
        if ok == 0 { 0 } else { age.max(0) as u32 }
    }

    /// Pixels the next frame redraws ([x, y, width, height], origin bottom-left). Everything
    /// outside keeps the back buffer's contents. Must precede the frame's first GL command.
    pub fn set_damage(&self, rect: [i32; 4]) {
        if let Some(set_damage) = self.set_damage {
            // An unchanged frame still names one pixel rather than relying on an empty list.
            let rect = [rect[0], rect[1], rect[2].max(1), rect[3].max(1)];
            // SAFETY: one rectangle of four ints, as EGL_KHR_partial_update specifies.
            unsafe { set_damage(self.display, self.surface, rect.as_ptr(), 1) };
        }
    }
}
