//! The widget authors' tool (docs/widgets.md §9): check a widget, run it on its fixtures or the
//! network, test it against its expected output, and pack, sign and verify the catalog.
use luma_widgets::catalog::{self, KeyPair};
use luma_widgets::fetch::Http;
use luma_widgets::refresh::{self, Cache, Context};
use luma_widgets::sandbox::{self, Job, Limits};
use luma_widgets::{Error, tree};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const USAGE: &str = "usage:
  luma-widget check <widget folder>
  luma-widget run <widget folder> <case> [--network]
  luma-widget eval '<jq program>' ['<input JSON>']  (Luma's jq, with $settings etc. empty)
  luma-widget test <widget folder>
  luma-widget pack <widgets folder> <out folder> --sequence N [--published RFC3339]
  luma-widget sign <out folder>                 (key in $CATALOG_SIGNING_KEY)
  luma-widget verify <out folder> --key <public key>
  luma-widget keygen --secret-out <file>        (prints the public key)

With --network, secrets come from $LUMA_WIDGET_SECRET_<SETTING> (e.g. LUMA_WIDGET_SECRET_KEY).";

/// Transforms run as they do on the TV: in a limited child process.
fn sandboxed(job: Job) -> Result<Value, Error> {
    let exe = std::env::current_exe().map_err(|e| Error(e.to_string()))?;
    sandbox::run(&exe, "--widget-transform", &job, &Limits::default())
}

fn option(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

fn network_run(dir: &Path, source: &str) -> Result<Value, Error> {
    let (manifest, _) = tree::load(dir)?;
    let context: Value =
        std::fs::read_to_string(dir.join("fixtures").join(source).join("context.json"))
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or(Value::Null);
    let saved = context
        .get("settings")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let settings = manifest.settings_values(&saved);
    let secrets: BTreeMap<String, String> = manifest
        .settings
        .iter()
        .filter_map(|s| {
            let value =
                std::env::var(format!("LUMA_WIDGET_SECRET_{}", s.id.to_uppercase())).ok()?;
            Some((s.id.clone(), value))
        })
        .collect();
    let lang = context.get("lang").and_then(Value::as_str).unwrap_or("en");
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    let cx = Context {
        manifest: &manifest,
        settings: &settings,
        secrets: &secrets,
        lang,
        now,
    };
    let mut output = Value::Null;
    let refreshed = refresh::run(&cx, &Http::new(), &mut Cache::default(), &mut |job| {
        let result = sandboxed(job);
        if let Ok(value) = &result {
            output = value.clone();
        }
        result
    })
    .map_err(|e| Error(e.to_string()))?;
    eprintln!(
        "source {}, {} items, next refresh in {} s",
        refreshed.source,
        refreshed.document.items.len(),
        refreshed.refresh
    );
    Ok(output)
}

fn command(args: &[String]) -> Result<(), Error> {
    let arg = |n: usize| {
        args.get(n)
            .map(PathBuf::from)
            .ok_or_else(|| Error(USAGE.into()))
    };
    match args.first().map(String::as_str) {
        Some("check") => {
            let (manifest, bytes) = tree::load(&arg(1)?)?;
            println!(
                "{} {}: OK ({} bytes, {} sources, origins {})",
                manifest.id,
                manifest.version,
                bytes.len(),
                manifest.sources.len(),
                manifest
                    .origins
                    .iter()
                    .map(|o| o.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
        Some("run") => {
            let dir = arg(1)?;
            let source = args.get(2).ok_or_else(|| Error(USAGE.into()))?;
            let output = if args.iter().any(|a| a == "--network") {
                network_run(&dir, source)?
            } else {
                tree::run_fixture(&dir, source, &mut sandboxed)?
            };
            println!(
                "{}",
                serde_json::to_string_pretty(&output).unwrap_or_default()
            );
        }
        Some("eval") => {
            let program = args.get(1).ok_or_else(|| Error(USAGE.into()))?;
            let input = match args.get(2) {
                Some(text) => serde_json::from_str(text).map_err(|e| Error(e.to_string()))?,
                None => Value::Null,
            };
            let vars = luma_widgets::jq::Vars {
                settings: Value::Object(Default::default()),
                t: Value::Object(Default::default()),
                lang: "en".into(),
                now: 0,
                source: String::new(),
                item: None,
            };
            let output = sandboxed(Job {
                program: program.clone(),
                input,
                vars,
            })?;
            println!(
                "{}",
                serde_json::to_string_pretty(&output).unwrap_or_default()
            );
        }
        Some("test") => {
            let passed = tree::test(&arg(1)?, &mut sandboxed)?;
            println!("passed: {}", passed.join(", "));
        }
        Some("pack") => {
            let sequence = option(args, "--sequence")
                .and_then(|s| s.parse().ok())
                .ok_or_else(|| Error("--sequence N is required".into()))?;
            let published = option(args, "--published").unwrap_or_default();
            let index = tree::pack(&arg(1)?, &arg(2)?, sequence, &published)?;
            for entry in &index.widgets {
                println!("{} {} {}", entry.id, entry.version, entry.sha256);
            }
        }
        Some("sign") => {
            let secret = std::env::var("CATALOG_SIGNING_KEY")
                .map_err(|_| Error("set CATALOG_SIGNING_KEY".into()))?;
            tree::sign(&arg(1)?, &KeyPair::from_secret_text(&secret)?)?;
            println!("signed");
        }
        Some("verify") => {
            let key = catalog::public_key(
                &option(args, "--key").ok_or_else(|| Error("--key is required".into()))?,
            )?;
            let index = tree::verify(&arg(1)?, &key)?;
            println!(
                "catalog {}: {} widgets verified",
                index.sequence,
                index.widgets.len()
            );
        }
        Some("keygen") => {
            let out = option(args, "--secret-out")
                .ok_or_else(|| Error("--secret-out <file> is required".into()))?;
            let keys = KeyPair::generate()?;
            write_secret(Path::new(&out), &keys.secret_text())?;
            println!("{}", catalog::public_text(&keys.public()));
        }
        _ => return Err(Error(USAGE.into())),
    }
    Ok(())
}

/// The secret key, readable only by its owner.
fn write_secret(path: &Path, text: &str) -> Result<(), Error> {
    use std::io::Write;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut file = options
        .open(path)
        .map_err(|e| Error(format!("{}: {e}", path.display())))?;
    file.write_all(text.as_bytes())
        .map_err(|e| Error(e.to_string()))
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("--widget-transform") {
        sandbox::child_main();
    }
    if let Err(error) = command(&args) {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
