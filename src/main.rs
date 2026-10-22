mod diagnostics;
mod font;
mod i18n;
#[cfg(all(feature = "webos", target_os = "linux", target_arch = "arm"))]
mod luna;
mod model;
mod platform;
mod raster;
mod renderer;
#[path = "scroll-preview.rs"]
mod scroll_preview;
mod state;
mod storage;
mod vector;
mod view;
mod weather;
#[path = "weather-service.rs"]
mod weather_service;
#[cfg(all(feature = "webos", any(target_os = "linux", test)))]
#[path = "webos-compat.rs"]
mod webos_compat;

#[cfg(test)]
mod tests;

use platform::{Platform, Request, Response};
use raster::Size;
use sdl2::{
    event::{Event, WindowEvent},
    keyboard::Keycode,
    mouse::MouseButton,
    video::GLProfile,
};
use state::{Direction, Effect, Overlay, State, Target};
use std::{
    collections::VecDeque,
    path::PathBuf,
    time::{Duration, Instant},
};
use time::OffsetDateTime;

#[derive(Default)]
struct Options {
    webos_home: bool,
    assets: Option<PathBuf>,
    preferences: Option<PathBuf>,
    screenshot: Option<PathBuf>,
    dump: Option<PathBuf>,
    frames: Option<u64>,
    script: VecDeque<String>,
    fullscreen: bool,
    time: Option<OffsetDateTime>,
    resolution: Option<Size>,
    capture_size: Option<Size>,
    diagnostics: Option<PathBuf>,
    profile_frames: Option<usize>,
    weather_test_server: Option<String>,
    test_extra_apps: usize,
}

impl Options {
    fn parse() -> Result<Self, String> {
        let mut result = Self::default();
        let mut args = std::env::args().skip(1).peekable();
        // SAM/jailer puts native launch metadata (e.g. @system_native_app and
        // nid) in argv[1]. It is not a Luma CLI option. Keep the original argv
        // intact for SDL/libhelpers, which also read the process launch data.
        if cfg!(feature = "webos")
            && args.peek().is_some_and(|argument| {
                serde_json::from_str::<serde_json::Value>(argument)
                    .is_ok_and(|value| value.is_object())
            })
        {
            if let Some(argument) = args.next() {
                let metadata: serde_json::Value =
                    serde_json::from_str(&argument).map_err(|error| error.to_string())?;
                result.webos_home = metadata.get("nid").and_then(serde_json::Value::as_str)
                    == Some(platform::HOME_ID);
            }
            eprintln!("Luma webOS launch parameters accepted");
        }
        while let Some(arg) = args.next() {
            if arg == "--help" {
                println!(
                    "Luma native\n\n  --assets PATH       Generated artwork directory\n  --preferences PATH  Isolated preferences file\n  --fullscreen        Fullscreen desktop preview\n  --resolution SIZE   Request 1080p, 4k, or WIDTHxHEIGHT; verify actual drawable\n  --frames N          Exit after N event-loop frames\n  --profile-frames N  Force N presentations (20+) and report pacing after warmup\n  --script ACTIONS    Comma-separated home/apps/settings/up/down/left/right/enter/back/menu/hold/type:TEXT/click:X:Y/wait\n  --time RFC3339      Fix the preview's clock and date\n  --screenshot PATH   Save an offscreen PNG\n  --capture-scale N   Capture at 1x/1080p (default) or 2x/4k\n  --diagnostics PATH  Save actual drawable, GPU, font-cache and timing information\n  --dump-state PATH   Save final UI state as JSON\n\nCompile with --features webos for the TV backend. Desktop builds never call TV services."
                );
                println!(
                    "\n  --test-extra-apps N  Explicit scroll preview with up to 300 non-launchable extra cards; separate preferences"
                );
                std::process::exit(0);
            }
            if arg == "--fullscreen" {
                result.fullscreen = true;
                continue;
            }
            let value = args
                .next()
                .ok_or_else(|| format!("Missing value for {arg}"))?;
            match arg.as_str() {
                "--assets" => result.assets = Some(value.into()),
                "--preferences" => result.preferences = Some(value.into()),
                "--screenshot" => result.screenshot = Some(value.into()),
                "--dump-state" => result.dump = Some(value.into()),
                "--resolution" => result.resolution = Some(value.parse()?),
                "--capture-scale" => {
                    result.capture_size = Some(match value.as_str() {
                        "1" => Size::HD,
                        "2" => Size::UHD,
                        _ => return Err("--capture-scale must be 1 or 2".into()),
                    })
                }
                "--diagnostics" => result.diagnostics = Some(value.into()),
                "--weather-test-server" => result.weather_test_server = Some(value),
                "--test-extra-apps" => {
                    result.test_extra_apps = value
                        .parse::<usize>()
                        .ok()
                        .filter(|count| *count <= 300)
                        .ok_or("--test-extra-apps must be between 0 and 300")?;
                }
                "--profile-frames" => {
                    result.profile_frames = Some(
                        value
                            .parse::<usize>()
                            .ok()
                            .filter(|n| (20..=1000000).contains(n))
                            .ok_or("--profile-frames must be between 20 and 1000000")?,
                    )
                }
                "--time" => {
                    result.time = Some(
                        OffsetDateTime::parse(
                            &value,
                            &time::format_description::well_known::Rfc3339,
                        )
                        .map_err(|e| e.to_string())?,
                    )
                }
                "--frames" => {
                    result.frames = Some(
                        value
                            .parse::<u64>()
                            .ok()
                            .filter(|n| *n > 0)
                            .ok_or("--frames must be positive")?,
                    )
                }
                "--script" => result.script = value.split(',').map(str::to_owned).collect(),
                _ => return Err(format!("Unknown option: {arg}")),
            }
        }
        if result.screenshot.is_some() && result.frames.is_none() && result.profile_frames.is_none()
        {
            result.frames = Some(90);
        }
        Ok(result)
    }
}

struct Press {
    target: Target,
    start: Instant,
    held: bool,
    pointer: bool,
}

const SCRIPT_POINTER: u32 = u32::MAX - 1;
// SDL-webOS's Back scancode is outside rust-sdl2's standard Scancode enum.
// Keycode retains the raw value: SDL_SCANCODE_TO_KEYCODE(SDL_SCANCODE_WEBOS_BACK).
const WEBOS_BACK_KEYCODE: i32 = (1 << 30) | 482;

fn effects(
    effects: Vec<Effect>,
    state: &mut State,
    platform: &Platform,
    preferences: &std::path::Path,
    weather: &weather_service::Service,
) -> bool {
    for effect in effects {
        match effect {
            Effect::Save => {
                if let Err(error) = storage::save(preferences, &state.preferences) {
                    eprintln!("{error}");
                    state.notify(state.preferences.language.t("savedError"));
                }
            }
            Effect::Refresh => {
                if !state.loading {
                    state.loading = true;
                    state.dirty = true;
                    platform.send(Request::Refresh);
                }
            }
            Effect::Launch(id) => {
                if scroll_preview::is_placeholder(&id) {
                    state.notify(state.preferences.language.t("scrollTestOnly"));
                    continue;
                }
                if !state.launching {
                    if platform.webos {
                        state.launching = true;
                        state.notify(state.preferences.language.t("opening"));
                        platform.send(Request::Launch(id));
                    } else {
                        let title = state
                            .inventory
                            .apps
                            .iter()
                            .find(|a| a.id == id)
                            .map_or(id.as_str(), |app| app.title.as_str());
                        state.notify(
                            state
                                .preferences
                                .language
                                .format("desktopPreview", &[("app", title)]),
                        );
                    }
                }
            }
            Effect::Quit => return false,
            Effect::Weather(command) => weather.send(command),
        }
    }
    true
}

fn script(
    action: &str,
    state: &mut State,
    controls: &[state::Control],
) -> Result<Vec<Effect>, String> {
    Ok(match action {
        "home" => {
            state.open(0);
            vec![]
        }
        "apps" => {
            state.open(1);
            vec![]
        }
        "settings" => {
            state.open(2);
            vec![]
        }
        "up" => state.direction(Direction::Up, controls),
        "down" => state.direction(Direction::Down, controls),
        "left" => state.direction(Direction::Left, controls),
        "right" => state.direction(Direction::Right, controls),
        "enter" => state.activate(),
        "back" => state.back(),
        "menu" => {
            state.options();
            vec![]
        }
        "hold" => {
            state.hold();
            vec![]
        }
        "wait" => vec![],
        action if action.starts_with("type:") => {
            let field = if state.subpage == Some(state::Subpage::WeatherLocation) {
                state::TextField::Location
            } else {
                state::TextField::Apps
            };
            state.open_keyboard(field);
            state.text(&action[5..]);
            vec![]
        }
        "await-search" => vec![],
        "await-weather" => vec![],
        _ => return Err(format!("Unknown scripted action: {action}")),
    })
}

fn main() {
    if let Err(error) = run() {
        eprintln!("Luma native: {error}");
        std::process::exit(1);
    }
}

fn compose(
    state: &mut State,
    assets: &view::Assets,
    pill: f32,
    width: f32,
    now: OffsetDateTime,
) -> view::Scene {
    let scene = view::scene_with_pill(state, assets, pill, width, now);
    if !state.pointer && state.overlay.is_none() && state.reveal_focus(&scene.controls) {
        view::scene_with_pill(state, assets, pill, width, now)
    } else {
        scene
    }
}

fn weather_tick(state: &mut State, service: &weather_service::Service, visible: bool) {
    let now = OffsetDateTime::now_utc().unix_timestamp();
    let (command, changed) = state.weather.sync(
        &state.preferences,
        visible && state.page == state::Page::Home,
        now,
    );
    state.dirty |= changed;
    service.generations(state.weather.generations());
    if let Some(command) = command {
        service.send(command);
    }
    while let Ok(response) = service.receiver.try_recv() {
        match response {
            weather_service::Response::Search { id, result } => {
                state.dirty |= state.weather.accept_search(id, result)
            }
            weather_service::Response::Forecast { id, key, result } => {
                let (changed, command) = state.weather.accept_forecast(id, &key, result, now);
                state.dirty |= changed;
                if let Some(command) = command {
                    service.send(command);
                }
            }
        }
    }
}

fn scripted_click(
    sdl: &sdl2::Sdl,
    window: &sdl2::video::Window,
    position: &str,
) -> Result<(), String> {
    let coordinates: Vec<f32> = position
        .split(':')
        .map(|value| value.parse::<f32>().map_err(|e| e.to_string()))
        .collect::<Result<_, _>>()?;
    let [x, y] = coordinates.as_slice() else {
        return Err("click: expects logical x:y coordinates".into());
    };
    if !x.is_finite() || !y.is_finite() {
        return Err("Pointer coordinates must be finite".into());
    }
    let (w, h) = window.size();
    let scale = (w as f32 / 1920.).min(h as f32 / 1080.);
    let x = (x * scale + (w as f32 - 1920. * scale) / 2.).round() as i32;
    let y = (y * scale + (h as f32 - 1080. * scale) / 2.).round() as i32;
    let events = sdl.event()?;
    for event in [
        Event::MouseButtonDown {
            timestamp: 0,
            window_id: window.id(),
            which: SCRIPT_POINTER,
            mouse_btn: MouseButton::Left,
            clicks: 1,
            x,
            y,
        },
        Event::MouseButtonUp {
            timestamp: 0,
            window_id: window.id(),
            which: SCRIPT_POINTER,
            mouse_btn: MouseButton::Left,
            clicks: 1,
            x,
            y,
        },
    ] {
        events.push_event(event)?;
    }
    Ok(())
}

fn run() -> Result<(), String> {
    let startup = Instant::now();
    let mut options = Options::parse()?;
    let webos = cfg!(feature = "webos");
    let startup_stage = |stage: &str| {
        if webos {
            eprintln!(
                "Luma startup: {stage} after {} ms",
                startup.elapsed().as_millis()
            );
        }
    };
    if !webos && std::path::Path::new("/etc/starfish-release").exists() {
        return Err("This desktop build cannot run on a TV. Build with --features webos.".into());
    }
    if webos {
        if !options.webos_home && platform::redirect_to_home()? {
            return Ok(());
        }
        let surface_id = if options.webos_home {
            platform::HOME_ID
        } else {
            platform::APP_ID
        };
        // SDL-webOS reads APPID for its shell surface and getenv (not a hint)
        // for WMCLASS. Set these before video init and before starting workers.
        // SAFETY: startup is still single-threaded here.
        unsafe {
            std::env::set_var("APPID", surface_id);
            std::env::set_var("SDL_VIDEO_WAYLAND_WMCLASS", surface_id);
        }
        eprintln!("Luma webOS surface: {surface_id}");
        sdl2::hint::set("SDL_WEBOS_ACCESS_POLICY_KEYS_BACK", "true");
    }
    let root = options.assets.clone().unwrap_or_else(|| {
        if webos {
            std::env::current_exe()
                .unwrap_or_default()
                .with_file_name("assets")
        } else {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".build/assets")
        }
    });
    let assets = view::Assets::load(root)?;
    startup_stage("asset metadata");
    let default_preferences = options
        .preferences
        .clone()
        .map(Ok)
        .unwrap_or_else(|| storage::default_path(webos))?;
    let preferences_path = if options.test_extra_apps > 0 && options.preferences.is_none() {
        default_preferences.with_file_name("scroll-preview-preferences.json")
    } else {
        default_preferences.clone()
    };
    // Start the preview with the user's current choices, but keep test favorites
    // and hidden placeholders out of their normal preferences file.
    let initial_preferences = if options.test_extra_apps > 0
        && options.preferences.is_none()
        && !preferences_path.exists()
    {
        storage::load(&default_preferences)?
    } else {
        storage::load(&preferences_path)?
    };
    let mut state = State::new(initial_preferences);
    if options.test_extra_apps > 0 {
        eprintln!(
            "Luma scroll preview: explicit {}-app expansion; preferences {}",
            options.test_extra_apps,
            preferences_path.display()
        );
    }
    // Discover the local offset before any HTTP/platform workers start.
    let local_offset = OffsetDateTime::now_local()
        .map(|now| now.offset())
        .unwrap_or(time::UtcOffset::UTC);
    if webos && options.weather_test_server.is_some() {
        return Err("Weather test servers are only available in desktop builds".into());
    }
    let weather_cache = weather_service::cache_path(&preferences_path);
    state.weather.cache = weather_service::load_cache(&weather_cache);
    let weather_service = weather_service::Service::new(
        weather_service::Client::new(options.weather_test_server.as_deref())?,
        weather_cache,
    );
    let platform = Platform::new(webos);
    effects(
        vec![Effect::Refresh],
        &mut state,
        &platform,
        &preferences_path,
        &weather_service,
    );
    startup_stage("preferences and workers");

    let sdl = sdl2::init()?;
    let drivers: Vec<_> = sdl2::video::drivers().collect();
    if webos && !drivers.contains(&"wayland") {
        return Err(format!(
            "SDL has no Wayland backend (drivers: {}). Rebuild the package with the SDL-webOS runtime.",
            drivers.join(", ")
        ));
    }
    let video = sdl.video().map_err(|error| {
        format!(
            "SDL video initialization: {error}; drivers: {}; XDG_RUNTIME_DIR={:?}; WAYLAND_DISPLAY={:?}",
            drivers.join(", "),
            std::env::var_os("XDG_RUNTIME_DIR"),
            std::env::var_os("WAYLAND_DISPLAY"),
        )
    })?;
    eprintln!("Luma SDL video driver: {}", video.current_video_driver());
    startup_stage("SDL video");
    let attributes = video.gl_attr();
    if webos {
        attributes.set_context_profile(GLProfile::GLES);
        attributes.set_context_version(2, 0);
    } else {
        attributes.set_context_profile(GLProfile::Core);
        attributes.set_context_version(3, 2);
    }
    attributes.set_double_buffer(true);
    attributes.set_alpha_size(8);
    attributes.set_depth_size(0);
    attributes.set_stencil_size(0);
    let requested = options.resolution.unwrap_or(if webos {
        Size::HD
    } else {
        Size::new(1280, 720)
    });
    let create_window = || {
        let mut builder = video.window("Luma", requested.width, requested.height);
        builder.opengl().allow_highdpi().position_centered();
        if options.fullscreen || (webos && options.resolution.is_none()) {
            builder.fullscreen_desktop();
        } else if webos {
            builder.borderless();
        } else {
            builder.resizable();
        }
        let window = builder.build().map_err(|e| e.to_string())?;
        let context = window.gl_create_context()?;
        Ok::<_, String>((window, context))
    };
    let mut opened = None;
    let mut failures = Vec::new();
    // Prefer true sample coverage of intact glyph outlines. Some TV EGL stacks
    // have fewer configurations, so retain a usable non-MSAA fallback.
    for samples in [4, 2, 0] {
        attributes.set_multisample_buffers(u8::from(samples > 0));
        attributes.set_multisample_samples(samples);
        match create_window() {
            Ok(window) => {
                opened = Some(window);
                break;
            }
            Err(error) => failures.push(format!("{samples}x MSAA: {error}")),
        }
    }
    let (window, _context) = opened.ok_or_else(|| failures.join("; "))?;
    gl::load_with(|name| video.gl_get_proc_address(name) as *const _);
    if !webos {
        unsafe { gl::Enable(gl::MULTISAMPLE) };
    }
    let mut samples = 0;
    unsafe { gl::GetIntegerv(gl::SAMPLES, &mut samples) };
    eprintln!("Luma GL multisampling: {samples} samples");
    startup_stage("window and GL context");
    if let Err(error) = video.gl_set_swap_interval(1) {
        eprintln!("VSync: {error}");
    }
    let mut renderer = renderer::Renderer::new(&assets.root.join("font.ttf"), webos)?;
    startup_stage("renderer");
    let mut profile = diagnostics::Profile::default();
    let mut presented = 0usize;
    let mut captured = None;
    let mut last_drawable = None;
    let mut events = sdl.event_pump()?;
    let mut press: Option<Press> = None;
    let initial_nav = view::navigation(&assets, state.preferences.language);
    let mut pill = initial_nav.items[0].x;
    let mut pill_width = initial_nav.items[0].w;
    let mut last_frame = Instant::now();
    let mut last_minute = None;
    let mut frame = 0u64;
    let mut visible = true;
    let mut running = true;
    let mut queued_event = None;
    let mut text_active = false;
    let mut initial_focus = options.script.is_empty();
    let scripted_input = !options.script.is_empty();
    let mut scene = compose(
        &mut state,
        &assets,
        pill,
        pill_width,
        options.time.unwrap_or_else(OffsetDateTime::now_utc),
    );
    while running {
        let now = options
            .time
            .unwrap_or_else(|| OffsetDateTime::now_utc().to_offset(local_offset));
        if last_minute != Some(now.unix_timestamp() / 60) {
            last_minute = Some(now.unix_timestamp() / 60);
            state.dirty = true;
        }
        state.tick();
        while let Ok(response) = platform.receiver.try_recv() {
            match response {
                Response::Inventory(Ok((mut inventory, warning))) => {
                    scroll_preview::append(
                        &mut inventory,
                        options.test_extra_apps,
                        state.preferences.language,
                    );
                    state.set_inventory(inventory, warning);
                    if initial_focus
                        && state.focus == Target::Nav(0)
                        && state.page == state::Page::Home
                    {
                        state.focus = state.first_content();
                    }
                    initial_focus = false;
                }
                Response::Inventory(Err(error)) => {
                    state.loading = false;
                    state.error = Some(error);
                    state.dirty = true;
                    if initial_focus {
                        state.focus = state.first_content();
                        initial_focus = false;
                    }
                }
                Response::Launched(result) => {
                    state.launching = false;
                    match result {
                        Err(error) => {
                            eprintln!("{error}");
                            state.notify(state.preferences.language.t("appOpenError"));
                        }
                        Ok(()) => {
                            state.notice = None;
                            state.dirty = true;
                        }
                    }
                }
            }
        }
        weather_tick(&mut state, &weather_service, visible);
        // Rebuild hit regions before input whenever inventory/layout changed.
        if state.dirty {
            scene = compose(&mut state, &assets, pill, pill_width, now);
        }
        for event in queued_event.take().into_iter().chain(events.poll_iter()) {
            // Window-entry pointer events can arrive after a slow first upload.
            // Bounded scripted captures own input until their final frame; only
            // their explicitly injected SDL clicks may alter pointer focus.
            let script_owns_input = scripted_input
                && (options.frames.is_some()
                    || options.profile_frames.is_some()
                    || !options.script.is_empty());
            let external_input = match &event {
                Event::MouseButtonDown { which, .. } | Event::MouseButtonUp { which, .. } => {
                    *which != SCRIPT_POINTER
                }
                Event::MouseMotion { .. }
                | Event::MouseWheel { .. }
                | Event::KeyDown { .. }
                | Event::KeyUp { .. }
                | Event::TextEditing { .. }
                | Event::TextInput { .. } => true,
                _ => false,
            };
            if script_owns_input && external_input {
                continue;
            }
            let mut pending = vec![];
            match event {
                Event::Quit { .. } => running = false,
                Event::Window {
                    window_id,
                    win_event,
                    ..
                } if window_id == window.id() => match win_event {
                    WindowEvent::Hidden | WindowEvent::Minimized => {
                        visible = false;
                        press = None;
                        if matches!(state.overlay, Some(Overlay::SystemKeyboard)) {
                            state.finish_text_input();
                        }
                        profile.pause();
                    }
                    WindowEvent::FocusLost => press = None,
                    WindowEvent::Shown | WindowEvent::Restored | WindowEvent::FocusGained => {
                        visible = true;
                        state.dirty = true;
                        if webos {
                            pending.push(Effect::Refresh);
                        }
                    }
                    WindowEvent::Resized(..)
                    | WindowEvent::SizeChanged(..)
                    | WindowEvent::Exposed => state.dirty = true,
                    _ => {}
                },
                Event::TextInput { text, .. } => state.text(&text),
                Event::TextEditing {
                    text,
                    start,
                    length,
                    ..
                } if text.is_empty() && matches!(state.overlay, Some(Overlay::SystemKeyboard)) => {
                    state.delete_surrounding_text(start, length);
                }
                Event::KeyDown {
                    keycode: Some(key),
                    repeat,
                    ..
                } => match key {
                    Keycode::Up => pending = state.direction(Direction::Up, &scene.controls),
                    Keycode::Down => pending = state.direction(Direction::Down, &scene.controls),
                    Keycode::Left => pending = state.direction(Direction::Left, &scene.controls),
                    Keycode::Right => pending = state.direction(Direction::Right, &scene.controls),
                    Keycode::Return | Keycode::KpEnter
                        if matches!(state.overlay, Some(Overlay::SystemKeyboard)) =>
                    {
                        press = None;
                        state.finish_text_input();
                    }
                    Keycode::Return | Keycode::KpEnter if !repeat => {
                        press = Some(Press {
                            target: state.focus.clone(),
                            start: Instant::now(),
                            held: false,
                            pointer: false,
                        });
                    }
                    Keycode::Escape | Keycode::AcBack if !repeat => {
                        press = None;
                        pending = state.back();
                    }
                    Keycode::Backspace if state.text_input_active() => state.delete(),
                    Keycode::Backspace if !repeat => {
                        press = None;
                        pending = state.back();
                    }
                    Keycode::Clear if matches!(state.overlay, Some(Overlay::SystemKeyboard)) => {
                        let length = state.editing_text().len() as i32;
                        state.delete_surrounding_text(-length, length);
                    }
                    Keycode::M | Keycode::Menu if !repeat && state.overlay.is_none() => {
                        press = None;
                        state.options();
                    }
                    Keycode::F5 if !repeat => pending.push(Effect::Refresh),
                    key if !repeat
                        && (i32::from(key) == 461
                            || (webos && i32::from(key) == WEBOS_BACK_KEYCODE)) =>
                    {
                        press = None;
                        pending = state.back();
                    }
                    _ => {}
                },
                Event::KeyUp {
                    keycode: Some(Keycode::Return | Keycode::KpEnter),
                    ..
                } => {
                    if let Some(held) = press.take()
                        && !held.pointer
                        && !held.held
                        && held.target == state.focus
                    {
                        pending = state.activate();
                    }
                }
                Event::MouseMotion { x, y, .. }
                    if state.moving.is_none()
                        && !matches!(state.overlay, Some(Overlay::SystemKeyboard)) =>
                {
                    let hovered = hit(&scene.controls, &window, x, y, &state);
                    if let Some(target) = hovered.clone() {
                        state.pointer_focus(target);
                    }
                    if press
                        .as_ref()
                        .is_some_and(|p| p.pointer && hovered.as_ref() != Some(&p.target))
                    {
                        press = None;
                    }
                }
                Event::MouseButtonDown {
                    mouse_btn, x, y, ..
                } => {
                    if let Some(target) = hit(&scene.controls, &window, x, y, &state)
                        && state.moving.is_none()
                    {
                        if matches!(state.overlay, Some(Overlay::SystemKeyboard)) {
                            state.finish_text_input();
                        }
                        // Pointer-down must not normalize scroll and move the clicked control.
                        state.pointer_focus(target);
                        if mouse_btn == MouseButton::Right {
                            state.options();
                            press = None;
                        } else if mouse_btn == MouseButton::Left {
                            press = Some(Press {
                                target: state.focus.clone(),
                                start: Instant::now(),
                                held: false,
                                pointer: true,
                            });
                        }
                    } else if mouse_btn == MouseButton::Left {
                        press = None;
                    }
                }
                Event::MouseButtonUp {
                    mouse_btn: MouseButton::Left,
                    x,
                    y,
                    ..
                } => match press.take() {
                    Some(held)
                        if held.pointer
                            && !held.held
                            && hit(&scene.controls, &window, x, y, &state).as_ref()
                                == Some(&held.target) =>
                    {
                        pending = state.activate()
                    }
                    None if scene.modal.is_some_and(|rect| {
                        pointer_position(&window, x, y).is_some_and(|(x, y)| !rect.contains(x, y))
                    }) =>
                    {
                        pending = state.back()
                    }
                    _ => {}
                },
                Event::MouseWheel {
                    x,
                    y,
                    mouse_x,
                    mouse_y,
                    ..
                } if state.overlay.is_none() => {
                    let over_favorites = state.page == state::Page::Home
                        && pointer_position(&window, mouse_x, mouse_y).is_some_and(|(x, y)| {
                            (64. ..1856.).contains(&x)
                                && scene.controls.iter().any(|control| {
                                    matches!(control.target, Target::App(_))
                                        && control.rect.contains(x, y)
                                })
                        });
                    if state.page == state::Page::Home && (x != 0 || over_favorites) {
                        state.scroll_favorites(
                            (if x != 0 { x } else { -y }) as f32 * state::HOME_TILE_PITCH,
                        );
                        press = None;
                    } else {
                        let max = scene
                            .controls
                            .iter()
                            .filter(|c| c.scrollable)
                            .map(|c| {
                                c.rect.y + c.rect.h + state.scroll
                                    - if state.page == state::Page::Home {
                                        1041.6
                                    } else {
                                        982.4
                                    }
                            })
                            .fold(0., f32::max);
                        state.scroll = (state.scroll - y as f32 * 90.).clamp(0., max.max(0.));
                    }
                    state.pointer = true;
                    state.dirty = true;
                }
                _ => {}
            }
            if state.take_greeting_save() {
                pending.push(Effect::Save);
            }
            running &= effects(
                pending,
                &mut state,
                &platform,
                &preferences_path,
                &weather_service,
            );
            if press
                .as_ref()
                .is_some_and(|p| !p.held && p.target != state.focus)
            {
                press = None;
            }
            let pressed = press.as_ref().filter(|p| !p.held).map(|p| p.target.clone());
            if state.pressed != pressed {
                state.pressed = pressed;
                state.dirty = true;
            }
            // SDL may deliver several navigation events in a single poll.
            if state.dirty {
                scene = compose(&mut state, &assets, pill, pill_width, now);
            }
        }
        if let Some(held) = &mut press
            && !held.held
            && held.start.elapsed() >= Duration::from_millis(600)
        {
            held.held = true;
            state.pressed = None;
            state.hold();
        }
        if !state.loading
            && frame.is_multiple_of(3)
            && !options.script.front().is_some_and(|action| {
                (action == "await-search" && state.weather.searching)
                    || (action == "await-weather" && state.weather.fetching())
            })
            && let Some(action) = options.script.pop_front()
        {
            if let Some(position) = action.strip_prefix("click:") {
                scripted_click(&sdl, &window, position)?;
            } else {
                let pending = script(&action, &mut state, &scene.controls)?;
                running &= effects(
                    pending,
                    &mut state,
                    &platform,
                    &preferences_path,
                    &weather_service,
                );
            }
        }
        weather_tick(&mut state, &weather_service, visible);
        // SDL-webOS disables text input when its own panel is dismissed, even
        // if it did not forward a Back/Return key to the application.
        if text_active
            && matches!(state.overlay, Some(Overlay::SystemKeyboard))
            && !video.text_input().is_active()
        {
            press = None;
            state.finish_text_input();
        }
        let keyboard = state.text_input_active();
        if keyboard != text_active {
            if keyboard {
                video.text_input().start();
            } else {
                video.text_input().stop();
            }
            text_active = keyboard;
        }
        let dt = last_frame.elapsed().as_secs_f32().min(0.1);
        last_frame = Instant::now();
        let navigation = view::navigation(&assets, state.preferences.language);
        let target = navigation.items[state.page_index()].x;
        let target_width = navigation.items[state.page_index()].w;
        if state.preferences.reduced_motion
            || ((pill - target).abs() < 0.2 && (pill_width - target_width).abs() < 0.2)
        {
            pill = target;
            pill_width = target_width;
        } else {
            pill += (target - pill) * (1. - (-20. * dt).exp());
            pill_width += (target_width - pill_width) * (1. - (-20. * dt).exp());
            state.dirty = true;
        }
        let final_frame = options.frames.is_some_and(|limit| frame + 1 >= limit)
            || (visible
                && options
                    .profile_frames
                    .is_some_and(|limit| presented + 1 >= limit));
        if ((state.dirty || options.profile_frames.is_some()) && visible) || final_frame {
            scene = compose(&mut state, &assets, pill, pill_width, now);
            let (width, height) = window.drawable_size();
            if width > 0 && height > 0 {
                if final_frame && let Some(path) = &options.screenshot {
                    captured = Some(renderer.capture(
                        &scene.draw,
                        path,
                        options.capture_size.unwrap_or(Size::HD),
                    )?);
                }
                renderer.render(&scene.draw, width, height);
                window.gl_swap_window();
                if presented == 0 {
                    startup_stage("first frame");
                    if webos {
                        let stats = renderer.stats();
                        eprintln!(
                            "Luma vector text: {} glyphs, {} text runs, {} KiB glyph geometry, {} KiB GPU text",
                            stats.cached_glyphs,
                            stats.cached_text_runs,
                            stats.glyph_geometry_bytes / 1024,
                            stats.text_geometry_bytes / 1024,
                        );
                    }
                }
                presented += 1;
                if options.profile_frames.is_some() {
                    profile.record(
                        Instant::now(),
                        renderer.stats().cpu_submission_ms,
                        final_frame && options.screenshot.is_some(),
                    );
                }
                let drawable = Size::new(width, height);
                if last_drawable != Some(drawable) {
                    if options.diagnostics.is_some() || options.resolution.is_some() {
                        eprintln!(
                            "Luma drawable: {}x{}, density {:.3}x, 4K buffer: {}",
                            width,
                            height,
                            drawable.scale(),
                            drawable.is_4k()
                        );
                    }
                    last_drawable = Some(drawable);
                }
            }
            state.dirty = false;
        }
        frame += 1;
        if final_frame {
            running = false;
        }
        // Block until input instead of polling at frame rate on idle screens.
        // VSync paces animation; holds and scripted checks need short timed wakes.
        let timeout = if !visible {
            250
        } else if pill != target
            || pill_width != target_width
            || state.animating()
            || options.profile_frames.is_some()
        {
            0
        } else if press.is_some() || options.frames.is_some() {
            8
        } else {
            100
        };
        queued_event = events.wait_event_timeout(timeout);
    }
    if text_active {
        video.text_input().stop();
    }
    if !options.script.is_empty() {
        return Err("Frame budget ended before the input script completed".into());
    }
    if let Some(path) = options.dump {
        let state = serde_json::json!({ "page": state.page, "focus": state.focus, "query": state.query,
            "filter": state.filter, "preferences": state.preferences, "apps": state.apps().iter().map(|a| &a.id).collect::<Vec<_>>(),
            "error": state.error, "scroll": state.scroll, "home_scroll": state.home_scroll, "subpage": state.subpage, "modal": state.overlay.is_some(), "weather": state.weather,
            "ui_text": scene.draw.iter().filter_map(|draw| match draw { renderer::Draw::Text(value, ..) => Some(value), _ => None }).collect::<Vec<_>>() });
        std::fs::write(
            path,
            serde_json::to_vec_pretty(&state).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
    }
    if let Some(path) = options.diagnostics {
        let (width, height) = window.size();
        let (dw, dh) = window.drawable_size();
        let display_mode = window.display_index().ok().and_then(|index| video.current_display_mode(index).ok())
            .map(|mode| serde_json::json!({ "width": mode.w, "height": mode.h, "refresh_hz": mode.refresh_rate }));
        let report = serde_json::json!({
            "logical_layout": Size::HD, "requested_window": requested,
            "window": Size::new(width, height), "drawable": Size::new(dw, dh),
            "display_mode": display_mode,
            "drawable_is_4k": Size::new(dw, dh).is_4k(), "presented_frames": presented,
            "graphics": renderer.info, "last_presented": renderer.stats(), "capture": captured,
            "profile": options.profile_frames.map(|_| profile.summary()),
        });
        std::fs::write(
            path,
            serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn pointer_position(window: &sdl2::video::Window, x: i32, y: i32) -> Option<(f32, f32)> {
    let (width, height) = window.size();
    let scale = (width as f32 / 1920.).min(height as f32 / 1080.);
    if scale <= 0. {
        return None;
    }
    Some((
        (x as f32 - (width as f32 - 1920. * scale) / 2.) / scale,
        (y as f32 - (height as f32 - 1080. * scale) / 2.) / scale,
    ))
}

fn hit(
    controls: &[state::Control],
    window: &sdl2::video::Window,
    x: i32,
    y: i32,
    state: &State,
) -> Option<Target> {
    let (x, y) = pointer_position(window, x, y)?;
    controls
        .iter()
        .rev()
        .find(|control| {
            control.rect.contains(x, y)
                && (!control.scrollable
                    || ((64. ..1856.).contains(&x)
                        && y >= 108.8
                        && y < if state.page == state::Page::Home && state.moving.is_none() {
                            1053.6
                        } else {
                            994.4
                        }))
        })
        .map(|control| control.target.clone())
}
