#[cfg(all(feature = "webos", target_os = "linux", target_arch = "arm"))]
#[path = "../../src/webos-compat.rs"]
mod webos_compat;
use luma_tv_helper::{
    files, home, input,
    system::{System, Tv},
    uninstall,
};
use serde_json::Value;
fn run() -> luma_tv_helper::Result<Value> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|s| s == "--help") {
        println!(
            "luma-helper: home COMMAND [APP_DIR] [HOME_ID] | read-app-icon JSON | sync-splash JSON | uninstall-app ID | tv-input COMMAND [APP_DIR]"
        );
        return Ok(serde_json::json!({"returnValue":true}));
    }
    // Host builds compile all operations for fixtures, but cannot mutate a developer machine.
    if !cfg!(all(
        feature = "webos",
        target_os = "linux",
        target_arch = "arm"
    )) {
        return Err("TV helper commands require an ARM webOS build".into());
    }
    let arg = |i| {
        args.get(i)
            .map(String::as_str)
            .ok_or_else(|| "Missing helper argument".to_string())
    };
    let sys = Tv;
    match arg(0)? {
        "read-app-icon" => {
            luma_tv_helper::system::root(&sys)?;
            let source = serde_json::from_str(arg(1)?).map_err(|e| e.to_string())?;
            files::read_icons(&sys, &source)
        }
        "sync-splash" => {
            luma_tv_helper::system::root(&sys)?;
            let request: Value = serde_json::from_str(arg(1)?).map_err(|e| e.to_string())?;
            files::sync_splash(
                &sys,
                std::path::Path::new(request["directory"].as_str().ok_or("Missing directory")?),
                request["background"].as_str().ok_or("Missing background")?,
            )
        }
        "uninstall-app" => Ok(uninstall::remove(arg(1)?, sys.uid())),
        "home" => home::dispatch(
            &sys,
            arg(1)?,
            args.get(2).map(String::as_str),
            args.get(3).map(String::as_str),
        ),
        "tv-input" => {
            let directory = args
                .get(2)
                .map(std::path::PathBuf::from)
                .or_else(|| {
                    std::env::current_exe()
                        .ok()
                        .and_then(|p| p.parent().map(std::path::Path::to_path_buf))
                })
                .ok_or("Cannot determine installation")?;
            input::configure(&sys, arg(1)?, &directory)
        }
        _ => Err("Unknown TV helper command".into()),
    }
}
fn main() {
    match run() {
        Ok(reply) => println!("{reply}"),
        Err(error) => {
            eprintln!("Luma TV helper: {error}");
            std::process::exit(1);
        }
    }
}
