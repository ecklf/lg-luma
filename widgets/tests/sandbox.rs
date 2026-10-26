use luma_widgets::jq::Vars;
use luma_widgets::sandbox::{self, Job, Limits};
use serde_json::json;
use std::{path::Path, time::Instant};

fn job(program: &str) -> Job {
    Job {
        program: program.into(),
        input: json!({ "a": [1, 2, 3] }),
        vars: Vars {
            settings: json!({}),
            t: json!({}),
            lang: "en".into(),
            now: 0,
            source: "shared".into(),
            item: None,
        },
    }
}

fn run(program: &str) -> Result<serde_json::Value, luma_widgets::Error> {
    let exe = Path::new(env!("CARGO_BIN_EXE_luma-widget"));
    sandbox::run(exe, "--widget-transform", &job(program), &Limits::default())
}

#[test]
fn a_transform_runs_in_the_child() {
    assert_eq!(run(".a | add").unwrap(), json!(6));
    let error = run("error(\"broken\")").unwrap_err();
    assert!(error.0.contains("broken"), "{error}");
}

#[test]
fn runaway_transforms_are_stopped_without_harming_the_parent() {
    for program in [
        "def f: 1 + f; f",
        "last(range(1e15))",
        "[range(1e10)] | length",
        "[limit(1e9; repeat(\"xxxxxxxxxxxxxxxx\"))] | length",
    ] {
        let start = Instant::now();
        let error = run(program).expect_err(program);
        assert!(
            start.elapsed().as_secs_f32() < 4.5,
            "{program} took too long"
        );
        assert!(!error.0.is_empty());
    }
    // The parent is fine and the next transform works.
    assert_eq!(run(".a | length").unwrap(), json!(3));
}

#[test]
fn oversized_output_is_refused() {
    let error = run("[range(200000) | \"xxxx\"]").unwrap_err();
    assert!(error.0.contains("512 KiB"), "{error}");
}
