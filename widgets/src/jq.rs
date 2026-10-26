//! Widget transforms in jq, as jaq implements it (docs/widgets.md §3.5). The standard library
//! comes without the filters that read the outside world or have side effects, plus `fmt`.
use crate::Error;
use jaq_core::load::{Arena, File, Loader};
use jaq_core::{Ctx, Vars as JaqVars, data, unwrap_valr};
use jaq_json::Val;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// What a transform sees besides its input.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Vars {
    pub settings: Value,
    pub t: Value,
    pub lang: String,
    pub now: i64,
    pub source: String,
    pub item: Option<Value>,
}

/// Global variables, in the order their values are given.
const GLOBALS: [&str; 6] = ["$settings", "$t", "$lang", "$now", "$source", "$item"];

/// Filters and definitions left out: they read input, the environment or the clock, or write
/// to stderr or exit.
const LEFT_OUT: [&str; 12] = [
    "input",
    "inputs",
    "env",
    "now",
    "debug",
    "debug_empty",
    "stderr",
    "stderr_empty",
    "halt",
    "halt_error",
    "input_filename",
    "get_search_list",
];

const FMT: &str = r#"def fmt($args): reduce ($args | to_entries[]) as $e (.; gsub("\\{" + $e.key + "\\}"; $e.value | tostring));"#;

type Filter = jaq_core::Filter<data::JustLut<Val>>;

fn compile(program: &str) -> Result<Filter, Error> {
    let fmt = jaq_core::load::parse(FMT, |p| p.defs()).unwrap_or_default();
    let defs = jaq_core::defs()
        .chain(jaq_std::defs())
        .chain(jaq_json::defs())
        .filter(|def| !LEFT_OUT.contains(&def.name))
        .chain(fmt);
    let funs = jaq_core::funs()
        .chain(jaq_std::funs())
        .chain(jaq_json::funs())
        .filter(|(name, ..)| !LEFT_OUT.contains(name));
    let loader = Loader::new(defs);
    let arena = Arena::default();
    let modules = loader
        .load(
            &arena,
            File {
                code: program,
                path: (),
            },
        )
        .map_err(|errors| Error(format!("jq: {}", describe(&errors))))?;
    jaq_core::Compiler::default()
        .with_funs(funs)
        .with_global_vars(GLOBALS)
        .compile(modules)
        .map_err(|errors| Error(format!("jq: {}", describe(&errors))))
}

fn describe<E: std::fmt::Debug>(errors: &[E]) -> String {
    let text = format!("{errors:?}");
    text.chars().take(400).collect()
}

/// Parses and compiles a program against the restricted library.
pub fn check(program: &str) -> Result<(), Error> {
    compile(program).map(|_| ())
}

fn to_val(value: &Value) -> Result<Val, Error> {
    jaq_json::read::parse_single(value.to_string().as_bytes())
        .map_err(|e| Error(format!("jq input: {e:?}")))
}

/// Runs a program on `input`; it must give exactly one output.
pub fn run(program: &str, input: Value, vars: &Vars) -> Result<Value, Error> {
    let filter = compile(program)?;
    let globals = [
        &vars.settings,
        &vars.t,
        &Value::from(vars.lang.clone()),
        &Value::from(vars.now),
        &Value::from(vars.source.clone()),
        vars.item.as_ref().unwrap_or(&Value::Null),
    ]
    .into_iter()
    .map(to_val)
    .collect::<Result<Vec<_>, _>>()?;
    let ctx = Ctx::<data::JustLut<Val>>::new(&filter.lut, JaqVars::new(globals));
    let mut outputs = filter.id.run((ctx, to_val(&input)?)).map(unwrap_valr);
    let first = match outputs.next() {
        Some(Ok(value)) => value,
        Some(Err(error)) => return Err(Error(format!("jq: {error}"))),
        None => return Err(Error("jq: the program gave no output".into())),
    };
    if outputs.next().is_some() {
        return Err(Error("jq: the program gave more than one output".into()));
    }
    serde_json::from_str(&first.to_string())
        .map_err(|e| Error(format!("jq: the output is not JSON: {e}")))
}
