//! Test-only app and helpers for driving a `Session` without a network or a UI.

use crate::{
    App, Clean, Demo, EvidenceSink, Kind, Mode, Preview, ToolSpec, ledger::RunStatus, model::Model,
    session::Session,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{cell::Cell, collections::BTreeMap, path::Path};

thread_local! {
    static VALUE: Cell<i64> = const { Cell::new(0) };
    static APPLIED: Cell<usize> = const { Cell::new(0) };
}

/// Resets the counter. Call first in each test.
pub fn reset() {
    VALUE.with(|c| c.set(0));
    APPLIED.with(|c| c.set(0));
}

pub fn set_value(v: i64) {
    VALUE.with(|c| c.set(v));
}

pub fn value() -> i64 {
    VALUE.with(|c| c.get())
}

/// How many times `bump` has actually run.
pub fn applied() -> usize {
    APPLIED.with(|c| c.get())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Empty {}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Bump {
    n: i64,
}

/// A tiny app: probes `peek`, `big`, `blocked`, and effect `bump`.
pub struct Counter;

impl App for Counter {
    const NAME: &'static str = "counter";
    const DEMO_SCRIPT: &'static str = "[]";

    fn open(_target: &Path, _mode: Mode) -> Result<Self, String> {
        Ok(Counter)
    }

    fn system_prompt(&self) -> String {
        "You test WraithKit.".into()
    }

    fn tools(&self) -> Vec<ToolSpec> {
        let empty = json!({"type": "object", "additionalProperties": false, "properties": {}});
        vec![
            ToolSpec {
                name: "peek",
                kind: Kind::Probe,
                description: "Read the counter.",
                parameters: empty.clone(),
            },
            ToolSpec {
                name: "big",
                kind: Kind::Probe,
                description: "Return a long text.",
                parameters: empty.clone(),
            },
            ToolSpec {
                name: "blocked",
                kind: Kind::Probe,
                description: "Always refused.",
                parameters: empty,
            },
            ToolSpec {
                name: "bump",
                kind: Kind::Effect,
                description: "Add n to the counter.",
                parameters: json!({"type": "object", "additionalProperties": false, "required": ["n"],
                    "properties": {"n": {"type": "integer"}}}),
            },
        ]
    }

    fn probe(&self, ev: &mut EvidenceSink<'_>, tool: &str, args: &Value) -> Result<String, String> {
        match tool {
            "peek" => {
                crate::args::<Empty>(args)?;
                let id = ev.add("counter", "hello world text")?;
                Ok(format!("[{id}] value {}", value()))
            }
            "big" => {
                crate::args::<Empty>(args)?;
                let text = "x".repeat(20_000);
                let id = ev.add("big", &text)?;
                Ok(format!("[{id}] {text}"))
            }
            "blocked" => Err("blocked: test refusal".into()),
            other => Err(format!("unknown tool {other}")),
        }
    }

    fn preview(&self, tool: &str, args: &Value) -> Result<Preview, String> {
        if tool != "bump" {
            return Err(format!("unknown effect {tool}"));
        }
        let a: Bump = crate::args(args)?;
        Ok(Preview {
            summary: format!("add {} to the counter", a.n),
            detail: format!("{} -> {}", value(), value() + a.n),
            warnings: vec![],
            inputs: BTreeMap::from([("counter".to_string(), value().to_string())]),
        })
    }

    fn apply(&self, tool: &str, args: Clean<Value>) -> Result<String, String> {
        if tool != "bump" {
            return Err(format!("unknown effect {tool}"));
        }
        let a: Bump = crate::args(args.get())?;
        VALUE.with(|c| c.set(c.get() + a.n));
        APPLIED.with(|c| c.set(c.get() + 1));
        Ok(format!("value {}", value()))
    }

    fn demo() -> Result<Demo, String> {
        Err("the counter app has no demo".into())
    }
}

/// An assistant message that asks for tools.
pub fn asst(calls: Vec<Value>) -> Value {
    json!({"role": "assistant", "content": null, "tool_calls": calls})
}

/// One tool call in the OpenAI shape. The arguments are a JSON string, as the API sends them.
pub fn call(id: &str, name: &str, args: Value) -> Value {
    json!({"id": id, "type": "function", "function": {"name": name, "arguments": args.to_string()}})
}

/// A plain text assistant message, which ends the run.
pub fn text(s: &str) -> Value {
    json!({"role": "assistant", "content": s})
}

pub fn fixture(turns: Vec<Value>) -> Model {
    Model::Fixture { script: turns }
}

/// Starts a Counter run with a scripted model. Resets the counter first.
pub fn start(dir: &Path, turns: Vec<Value>) -> Session<Counter> {
    reset();
    Session::start(dir, dir, "task", Mode::Fixture, fixture(turns)).expect("start a counter run")
}

/// Steps until the run stops being `running`. Stops at once when it waits for a decision.
pub fn run_to_end<A: App>(s: &mut Session<A>) {
    for _ in 0..100 {
        if s.state.status != Some(RunStatus::Running) {
            break;
        }
        s.step().expect("step");
    }
}
