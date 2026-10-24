//! Buffer age and damage regions for partial redraw on EGL window surfaces.
//! EGL_KHR_partial_update (webOS Mali) also tells the driver which tiles to preserve, so a
//! small change costs only those tiles; EGL_EXT_buffer_age alone still allows redrawing less.
//! Without either, or outside Wayland (X11 may be GLX), every frame is drawn in full.
use std::ffi::{CStr, c_char, c_void};

type Display = *mut c_void;
type Surface = *mut c_void;
const EGL_EXTENSIONS: i32 = 0x3055;
const EGL_DRAW: i32 = 0x3059;
const EGL_BUFFER_AGE: i32 = 0x313D;
type QuerySurface = unsafe extern "C" fn(Display, Surface, i32, *mut i32) -> u32;
type SetDamage = unsafe extern "C" fn(Display, Surface, *const i32, i32) -> u32;

pub struct PartialUpdate {
    display: Display,
    surface: Surface,
    query_surface: QuerySurface,
    set_damage: Option<SetDamage>,
}

impl PartialUpdate {
    /// For the current context's draw surface; call after the GL context is current.
    pub fn load(video: &sdl2::VideoSubsystem) -> Option<Self> {
        // Only Wayland contexts are EGL here (webOS too). Under X11 the context may be GLX,
        // whose glXGetProcAddress returns non-null stubs even for EGL names.
        if video.current_video_driver() != "wayland" {
            return None;
        }
        let proc = |name: &str| {
            let address = video.gl_get_proc_address(name);
            (!address.is_null()).then_some(address)
        };
        // SAFETY: the addresses are EGL entry points with these C signatures, resolved by SDL
        // from the EGL library it loaded for the current context.
        unsafe {
            let current_display: unsafe extern "C" fn() -> Display =
                std::mem::transmute(proc("eglGetCurrentDisplay")?);
            let current_surface: unsafe extern "C" fn(i32) -> Surface =
                std::mem::transmute(proc("eglGetCurrentSurface")?);
            let query_string: unsafe extern "C" fn(Display, i32) -> *const c_char =
                std::mem::transmute(proc("eglQueryString")?);
            let query_surface: QuerySurface = std::mem::transmute(proc("eglQuerySurface")?);
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
                proc("eglSetDamageRegionKHR")
                    .map(|address| std::mem::transmute::<*const (), SetDamage>(address))
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
