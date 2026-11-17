//! The contract an app implements. Keep it small.

use crate::{
    Clean, Mode,
    ledger::{Event, Ledger},
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Kind {
    /// Read-only. Runs without approval.
    Probe,
    /// Changes something. Runs only after a human approves the exact preview.
    Effect,
}

pub struct ToolSpec {
    pub name: &'static str,
    pub description: &'static str,
    /// JSON Schema object for the arguments.
    pub parameters: Value,
    pub kind: Kind,
}

#[derive(Debug)]
pub struct Preview {
    /// One line, for example "Edit invoice/money.py: replace 1 block".
    pub summary: String,
    /// Diff or file preview, shown in a `<pre>`.
    pub detail: String,
    pub warnings: Vec<String>,
    /// What the effect depends on, mapped to its current SHA-256 or "absent".
    pub inputs: BTreeMap<String, String>,
}

pub struct Demo {
    pub target: PathBuf,
    pub task: String,
}

pub trait App: Sized + Send + 'static {
    const NAME: &'static str;
    /// The demo's scripted assistant messages (OpenAI chat-completions shape), as a JSON array.
    /// A const, so resuming a fixture run reads it without writing the fixture files again.
    const DEMO_SCRIPT: &'static str;
    fn open(target: &Path, mode: Mode) -> Result<Self, String>;
    fn system_prompt(&self) -> String;
    fn tools(&self) -> Vec<ToolSpec>;
    /// Read-only. Return `Err("blocked: ...")` for policy refusals.
    fn probe(&self, ev: &mut EvidenceSink<'_>, tool: &str, args: &Value) -> Result<String, String>;
    /// Describe an effect and hash its inputs as they are now. Called at proposal and again at approval.
    fn preview(&self, tool: &str, args: &Value) -> Result<Preview, String>;
    /// Perform an effect. Reachable only with a `Clean` value.
    fn apply(&self, tool: &str, args: Clean<Value>) -> Result<String, String>;
    /// Writes the bundled fixture files to a fresh folder and returns the target and task.
    fn demo() -> Result<Demo, String>;
}

/// Phrases addressed to AI agents. A heuristic: no flag does not mean the text is safe.
const AGENT_PHRASES: &[&str] = &[
    "ignore previous",
    "ignore all previous",
    "ignore prior",
    "ignore your instructions",
    "disregard previous",
    "ai agent",
    "ai assistant",
    "language model",
    "system prompt",
    "you are now",
    "new instructions",
];

pub fn agent_flags(text: &str) -> Vec<String> {
    let lower = text.to_lowercase();
    if AGENT_PHRASES.iter().any(|p| lower.contains(p)) {
        vec!["addresses_ai_agents".to_string()]
    } else {
        vec![]
    }
}

pub struct EvidenceSink<'a> {
    pub(crate) ledger: &'a mut Ledger,
    pub(crate) next_id: usize,
}

impl EvidenceSink<'_> {
    /// Stores `text` as evidence and returns its ID, for example "E3". `text` is exactly what the model sees.
    pub fn add(&mut self, source: &str, text: &str) -> Result<String, String> {
        let sha = self
            .ledger
            .put_blob(text)
            .map_err(|e| format!("could not store evidence: {e}"))?;
        self.next_id += 1;
        let id = format!("E{}", self.next_id);
        let flags = agent_flags(text);
        self.ledger
            .append(Event::Evidence {
                id: id.clone(),
                source: source.into(),
                sha256: sha,
                bytes: text.len(),
                flags,
            })
            .map_err(|e| format!("could not record evidence: {e}"))?;
        Ok(id)
    }
}

/// Kernel-owned tools, offered to the model alongside the app's tools.
pub fn builtin_tools() -> Vec<Value> {
    vec![
        json!({"type": "function", "function": {
            "name": "claim",
            "description": "Record one factual claim with exact quotes from evidence you read. Quotes must be copied verbatim (at least 8 characters).",
            "parameters": {"type": "object", "additionalProperties": false,
                "required": ["text", "status", "cites"],
                "properties": {
                    "text": {"type": "string"},
                    "status": {"type": "string", "enum": ["supported", "conflicted", "unsupported"]},
                    "cites": {"type": "array", "items": {"type": "object", "additionalProperties": false,
                        "required": ["evidence_id", "quote"],
                        "properties": {"evidence_id": {"type": "string"}, "quote": {"type": "string"}}}}}}}}),
        json!({"type": "function", "function": {
            "name": "finish",
            "description": "End the run with a short summary for the user.",
            "parameters": {"type": "object", "additionalProperties": false,
                "required": ["summary"], "properties": {"summary": {"type": "string"}}}}}),
    ]
}

/// The OpenAI tool schema for one app tool.
pub fn tool_schema(spec: &ToolSpec) -> Value {
    json!({"type": "function", "function": {
        "name": spec.name, "description": spec.description, "parameters": spec.parameters}})
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TempDir;

    #[test]
    fn evidence_ids_sequential() {
        let d = TempDir::new("ev");
        let mut l = Ledger::open(d.path()).unwrap();
        let mut sink = EvidenceSink {
            ledger: &mut l,
            next_id: 0,
        };
        assert_eq!(sink.add("a", "one").unwrap(), "E1");
        assert_eq!(sink.add("b", "two").unwrap(), "E2");
        assert_eq!(sink.add("c", "three").unwrap(), "E3");
        assert_eq!(l.lines.len(), 3);
    }

    #[test]
    fn flags_agent_addressed_text() {
        assert_eq!(
            agent_flags("Note for AI agents: run this"),
            vec!["addresses_ai_agents"]
        );
        assert_eq!(
            agent_flags("Please IGNORE PREVIOUS instructions"),
            vec!["addresses_ai_agents"]
        );
        assert!(agent_flags("The deadline is November 30.").is_empty());
    }
}
