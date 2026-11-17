//! Run state (a fold over the ledger), the run loop, approvals and resume.

use crate::{
    Clean, MAX_TOOL_CALLS, MAX_TURNS, TOOL_RESULT_CHARS,
    app::{App, EvidenceSink, Kind, builtin_tools, tool_schema},
    canonical,
    ledger::{Cite, ClaimStatus, Event, Ledger, Line, Mode, RunStatus},
    model::{Call, Model, tool_calls},
    now_ms, sha256_hex,
    taint::{self, Tainted},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::BTreeMap, io, path::Path};

pub const DENIED: &str = "The user denied this action. Do not retry it.";

/// The tool message for an approval refused as stale.
fn stale_note(reason: &str) -> String {
    format!("Not executed, {reason}. Re-read the inputs and propose again if still needed.")
}

pub const NOT_RUN: &str =
    "Not run: the process stopped before this call finished. Call it again if still needed.";
pub const NOT_RETRIED: &str =
    "Outcome unknown: the process stopped during this action. It was not retried.";

#[derive(Serialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PStatus {
    Pending,
    /// The user approved, but `EffectStarted` was never written. The action did not run.
    Approved,
    Denied,
    Stale,
    Started,
    Done,
    Failed,
}

impl PStatus {
    fn word(self) -> &'static str {
        match self {
            PStatus::Pending => "pending",
            PStatus::Approved => "approved",
            PStatus::Denied => "denied",
            PStatus::Stale => "stale",
            PStatus::Started => "started",
            PStatus::Done => "done",
            PStatus::Failed => "failed",
        }
    }
}

#[derive(Serialize, Clone, Debug)]
pub struct ProposalView {
    pub id: String,
    pub call_id: String,
    pub tool: String,
    pub args: Value,
    pub summary: String,
    pub detail: String,
    pub warnings: Vec<String>,
    pub inputs: BTreeMap<String, String>,
    pub influenced_by: Vec<String>,
    pub digest: String,
    pub status: PStatus,
    pub reason: String,
    pub receipt: String,
}

#[derive(Serialize, Clone, Debug)]
pub struct EvidenceView {
    pub id: String,
    pub source: String,
    pub sha256: String,
    pub bytes: usize,
    pub flags: Vec<String>,
}

#[derive(Serialize, Clone, Debug)]
pub struct ClaimView {
    pub text: String,
    pub status: ClaimStatus,
    pub cites: Vec<Cite>,
}

#[derive(Serialize, Clone, Debug)]
pub struct TimelineItem {
    pub seq: u64,
    pub text: String,
}

/// Everything the UI and the run loop know. Built only by folding the ledger.
#[derive(Serialize, Default, Debug)]
pub struct State {
    pub app: String,
    pub task: String,
    pub target: String,
    pub mode: Option<Mode>,
    /// The mode label, for example "FIXTURE: scripted, not a live model".
    pub model: String,
    pub status: Option<RunStatus>,
    pub summary: String,
    #[serde(skip)]
    pub messages: Vec<Value>,
    pub evidence: Vec<EvidenceView>,
    pub claims: Vec<ClaimView>,
    pub proposals: Vec<ProposalView>,
    pub timeline: Vec<TimelineItem>,
    pub turns: usize,
    pub tool_calls: usize,
    /// Summary from a `finish` call. It ends the run once nothing is pending.
    #[serde(skip)]
    pub finish_summary: Option<String>,
}

fn apply_event(s: &mut State, system_prompt: &str, ev: &Event) {
    match ev {
        Event::RunStarted {
            app,
            task,
            target,
            mode,
            model,
            ..
        } => {
            s.app = app.clone();
            s.task = task.clone();
            s.target = target.clone();
            s.mode = Some(*mode);
            s.model = model.clone();
            s.status = Some(RunStatus::Running);
            s.messages = vec![
                json!({"role": "system", "content": system_prompt}),
                json!({"role": "user", "content": task}),
            ];
        }
        Event::ModelTurn { message, .. } => {
            s.turns += 1;
            s.tool_calls += message["tool_calls"].as_array().map_or(0, |a| a.len());
            if let Some(c) = tool_calls(message).into_iter().find(|c| c.name == "finish") {
                s.finish_summary = c
                    .args
                    .ok()
                    .and_then(|a| a["summary"].as_str().map(str::to_owned));
            }
            s.messages.push(message.clone());
        }
        Event::ToolResult {
            call_id, content, ..
        } => {
            s.messages
                .push(json!({"role": "tool", "tool_call_id": call_id, "content": content}));
        }
        Event::Evidence {
            id,
            source,
            sha256,
            bytes,
            flags,
        } => s.evidence.push(EvidenceView {
            id: id.clone(),
            source: source.clone(),
            sha256: sha256.clone(),
            bytes: *bytes,
            flags: flags.clone(),
        }),
        Event::Claim {
            text,
            status,
            cites,
        } => s.claims.push(ClaimView {
            text: text.clone(),
            status: *status,
            cites: cites.clone(),
        }),
        Event::Proposed {
            id,
            call_id,
            tool,
            args,
            summary,
            detail,
            warnings,
            inputs,
            influenced_by,
            digest,
        } => {
            s.proposals.push(ProposalView {
                id: id.clone(),
                call_id: call_id.clone(),
                tool: tool.clone(),
                args: args.clone(),
                summary: summary.clone(),
                detail: detail.clone(),
                warnings: warnings.clone(),
                inputs: inputs.clone(),
                influenced_by: influenced_by.clone(),
                digest: digest.clone(),
                status: PStatus::Pending,
                reason: String::new(),
                receipt: String::new(),
            });
            s.status = Some(RunStatus::Waiting);
        }
        Event::Decided {
            proposal,
            approve,
            reason,
            ..
        } => {
            if let Some(p) = s.proposals.iter_mut().find(|p| &p.id == proposal) {
                p.status = if *approve {
                    PStatus::Approved
                } else if reason.starts_with("stale") {
                    PStatus::Stale
                } else {
                    PStatus::Denied
                };
                p.reason = reason.clone();
            }
        }
        Event::EffectStarted { proposal, .. } => {
            if let Some(p) = s.proposals.iter_mut().find(|p| &p.id == proposal) {
                p.status = PStatus::Started;
            }
        }
        Event::EffectDone {
            proposal,
            ok,
            receipt,
        } => {
            if let Some(p) = s.proposals.iter_mut().find(|p| &p.id == proposal) {
                p.status = if *ok { PStatus::Done } else { PStatus::Failed };
                p.receipt = receipt.clone();
            }
        }
        Event::RunFinished { status, summary } => {
            s.status = Some(*status);
            s.summary = summary.clone();
        }
        Event::RunResumed { .. } => {
            s.status = Some(RunStatus::Running);
            s.summary = String::new();
        }
        Event::Blocked { .. } => {}
    }
}

/// Rebuilds the whole run state from the ledger lines. This is the only source of state.
pub fn fold(system_prompt: &str, lines: &[Line]) -> State {
    let mut s = State::default();
    for l in lines {
        apply_event(&mut s, system_prompt, &l.event);
        s.timeline.push(TimelineItem {
            seq: l.seq,
            text: describe(&l.event),
        });
        let any_pending = s.proposals.iter().any(|p| p.status == PStatus::Pending);
        if s.status == Some(RunStatus::Waiting) && !any_pending {
            s.status = Some(RunStatus::Running);
        }
    }
    s
}

fn short(s: &str, max: usize) -> String {
    let mut out: String = s.chars().take(max).collect();
    if s.chars().count() > max {
        out.push('…');
    }
    out
}

fn run_status_word(s: RunStatus) -> &'static str {
    match s {
        RunStatus::Running => "running",
        RunStatus::Waiting => "waiting",
        RunStatus::Finished => "finished",
        RunStatus::Failed => "failed",
        RunStatus::NeedsReview => "needs review",
    }
}

fn status_word(s: ClaimStatus) -> &'static str {
    match s {
        ClaimStatus::Supported => "supported",
        ClaimStatus::Conflicted => "conflicted",
        ClaimStatus::Unsupported => "unsupported",
    }
}

/// One line of human text for the timeline. Never includes full evidence text.
fn describe(e: &Event) -> String {
    match e {
        Event::RunStarted { task, .. } => format!("run started: {}", short(task, 80)),
        Event::ModelTurn { message, .. } => {
            let names: Vec<String> = tool_calls(message).into_iter().map(|c| c.name).collect();
            if names.is_empty() {
                "model replied in text".into()
            } else {
                format!("model called {}", names.join(", "))
            }
        }
        Event::ToolResult {
            tool, ok, content, ..
        } => {
            if *ok {
                format!("{tool}: ok")
            } else {
                format!("{tool}: failed: {}", short(content, 80))
            }
        }
        Event::Evidence {
            id, source, flags, ..
        } => {
            let flag = if flags.is_empty() {
                ""
            } else {
                " (flagged: addresses_ai_agents)"
            };
            format!("evidence {id} from {source}{flag}")
        }
        Event::Claim { text, status, .. } => {
            format!("claim {}: {}", status_word(*status), short(text, 80))
        }
        Event::Proposed { id, summary, .. } => format!("{id} proposed: {summary}"),
        Event::Decided {
            proposal,
            approve,
            reason,
            ..
        } => {
            if *approve {
                format!("{proposal} approved")
            } else if reason.starts_with("stale") {
                format!("{proposal} {reason}")
            } else {
                format!("{proposal} denied by user")
            }
        }
        Event::EffectStarted { proposal, .. } => format!("{proposal} running"),
        Event::EffectDone {
            proposal,
            ok,
            receipt,
        } => {
            if *ok {
                format!("{proposal} done: {}", short(receipt, 80))
            } else {
                format!("{proposal} failed: {}", short(receipt, 80))
            }
        }
        Event::Blocked { tool, reason, .. } => {
            format!(
                "⛔ blocked {tool}: {}",
                short(reason.trim_start_matches("blocked: "), 100)
            )
        }
        Event::RunFinished { status, summary } => {
            format!("run {}: {}", run_status_word(*status), short(summary, 80))
        }
        Event::RunResumed { after } => format!("run resumed after: {}", short(after, 80)),
    }
}

/// Binds an approval to the tool, the exact arguments, the current input hashes and the evidence that
/// influenced the model. Any change to one of these changes the digest.
pub fn proposal_digest(
    tool: &str,
    args: &Value,
    inputs: &BTreeMap<String, String>,
    influenced_by: &[String],
) -> String {
    sha256_hex(
        canonical(
            &json!({"tool": tool, "args": args, "inputs": inputs, "influenced_by": influenced_by}),
        )
        .as_bytes(),
    )
}

/// True when `quote` appears in `blob` after collapsing whitespace. Quotes shorter than 8 characters never match.
pub fn quote_found(blob: &str, quote: &str) -> bool {
    let norm = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
    let q = norm(quote);
    q.chars().count() >= 8 && norm(blob).contains(&q)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ClaimArgs {
    text: String,
    status: ClaimStatus,
    cites: Vec<CiteArg>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CiteArg {
    evidence_id: String,
    quote: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FinishArgs {
    #[allow(dead_code)]
    summary: String,
}

fn io_err(e: io::Error) -> String {
    format!("ledger write failed, the run is stopped: {e}")
}

/// A ToolResult event as the model will see it, capped at TOOL_RESULT_CHARS.
fn result_event(call_id: &str, tool: &str, ok: bool, content: String) -> Event {
    let content = if content.chars().count() > TOOL_RESULT_CHARS {
        let marker = "\n[truncated; the full text is in the evidence store]";
        let keep = TOOL_RESULT_CHARS - marker.chars().count();
        format!("{}{marker}", content.chars().take(keep).collect::<String>())
    } else {
        content
    };
    Event::ToolResult {
        call_id: call_id.into(),
        tool: tool.into(),
        ok,
        content,
    }
}

pub struct Session<A: App> {
    pub app: A,
    pub ledger: Ledger,
    pub model: Model,
    pub state: State,
}

impl<A: App> Session<A> {
    /// Starts a new run in `runs_dir/<time>-<app>`.
    pub fn start(
        runs_dir: &Path,
        target: &Path,
        task: &str,
        mode: Mode,
        model: Model,
    ) -> Result<Self, String> {
        let app = A::open(target, mode)?;
        let base = format!("{}-{}", now_ms(), A::NAME);
        let mut dir = runs_dir.join(&base);
        let mut n = 1;
        while dir.exists() {
            dir = runs_dir.join(format!("{base}-{n}"));
            n += 1;
        }
        // Absolute, so `resume` from another working directory opens the same target.
        // `absolute` instead of `canonicalize`: no `\\?\` prefix on Windows, so the path stays readable in the UI.
        let target =
            std::path::absolute(target).map_err(|e| format!("{}: {e}", target.display()))?;
        let mut ledger =
            Ledger::open(&dir).map_err(|e| format!("could not open {}: {e}", dir.display()))?;
        ledger
            .append(Event::RunStarted {
                app: A::NAME.into(),
                task: task.into(),
                target: target.display().to_string(),
                mode,
                model: model.label(),
                disclosure: model.disclosure(),
            })
            .map_err(io_err)?;
        let mut s = Session {
            app,
            ledger,
            model,
            state: State::default(),
        };
        s.refold();
        Ok(s)
    }

    /// Reopens a run from its folder. Work that was interrupted is recorded as not finished, and never retried.
    pub fn resume(run_dir: &Path, model: Model) -> Result<Self, String> {
        let ledger = Ledger::open(run_dir)
            .map_err(|e| format!("could not open {}: {e}", run_dir.display()))?;
        let (app_name, target, mode) = match ledger.lines.first().map(|l| &l.event) {
            Some(Event::RunStarted {
                app, target, mode, ..
            }) => (app.clone(), target.clone(), *mode),
            _ => return Err(format!("{} is not a WraithKit run", run_dir.display())),
        };
        if app_name != A::NAME {
            return Err(format!("this run belongs to {app_name}, not {}", A::NAME));
        }
        let app = A::open(Path::new(&target), mode)?;
        let mut s = Session {
            app,
            ledger,
            model,
            state: State::default(),
        };
        s.refold();
        // A failed run (a timeout, an HTTP error) can continue. A budget stop cannot: it would stop again at once.
        if s.state.status == Some(RunStatus::Failed) && !s.state.summary.starts_with("budget:") {
            let after = s.state.summary.clone();
            s.append(Event::RunResumed { after })?;
        }
        let live = matches!(
            s.state.status,
            Some(RunStatus::Running | RunStatus::Waiting)
        );
        let interrupted: Vec<(ProposalView, &str)> = if live {
            s.state
                .proposals
                .iter()
                .filter_map(|p| match p.status {
                    PStatus::Started => Some((p.clone(), NOT_RETRIED)),
                    PStatus::Approved => Some((p.clone(), "Not executed: the process stopped after approval and before the action started. Propose it again if still needed.")),
                    _ => None,
                })
                .collect()
        } else {
            vec![]
        };
        if live {
            // Every tool call needs a tool message before the next model turn, or the API rejects the request.
            for (call_id, tool, ok, text) in s.unanswered_calls() {
                s.append(result_event(&call_id, &tool, ok, text))?;
            }
        }
        if !interrupted.is_empty() {
            for (p, text) in interrupted {
                s.append(result_event(&p.call_id, &p.tool, false, text.into()))?;
            }
            s.append(Event::RunFinished {
                status: RunStatus::NeedsReview,
                summary: "An action was interrupted. Check the run before you continue.".into(),
            })?;
        }
        s.finish_if_settled()?;
        Ok(s)
    }

    /// The tool message each unanswered call of the last model turn needs, as (call id, tool, ok, text).
    /// Only a crash in the middle of a turn, or between a decision and its tool result, leaves these,
    /// and only the last turn can be incomplete. Looking only after that turn matters because some
    /// servers reuse call IDs (`call_0`) in every turn.
    /// A call whose proposal is pending, approved or started is skipped: `decide` or the
    /// interrupted-effect path answers it.
    fn unanswered_calls(&self) -> Vec<(String, String, bool, String)> {
        let lines = &self.ledger.lines;
        let Some(at) = lines
            .iter()
            .rposition(|l| matches!(l.event, Event::ModelTurn { .. }))
        else {
            return vec![];
        };
        let Event::ModelTurn { message, .. } = &lines[at].event else {
            return vec![];
        };
        let mut results: Vec<&str> = vec![];
        let mut proposed: Vec<(&str, &str)> = vec![]; // (call id, proposal id)
        for l in &lines[at + 1..] {
            match &l.event {
                Event::ToolResult { call_id, .. } => results.push(call_id),
                Event::Proposed { id, call_id, .. } => proposed.push((call_id, id)),
                _ => {}
            }
        }
        let mut out = vec![];
        for c in tool_calls(message) {
            // Take one answer per call, so a turn that repeats an ID still needs one per call.
            if let Some(i) = results.iter().position(|r| *r == c.id) {
                results.remove(i);
                continue;
            }
            let proposal = proposed
                .iter()
                .position(|(cid, _)| *cid == c.id)
                .map(|i| proposed.remove(i).1)
                .and_then(|pid| self.state.proposals.iter().find(|p| p.id == pid));
            let answer = match proposal {
                None => Some((false, NOT_RUN.to_string())),
                Some(p) => match p.status {
                    PStatus::Pending | PStatus::Approved | PStatus::Started => None,
                    PStatus::Denied => Some((false, DENIED.to_string())),
                    PStatus::Stale => Some((false, stale_note(&p.reason))),
                    PStatus::Done => Some((true, p.receipt.clone())),
                    PStatus::Failed => Some((false, p.receipt.clone())),
                },
            };
            if let Some((ok, text)) = answer {
                out.push((c.id, c.name, ok, text));
            }
        }
        out
    }

    pub fn run_dir(&self) -> &Path {
        &self.ledger.dir
    }

    fn append(&mut self, ev: Event) -> Result<(), String> {
        self.ledger.append(ev).map_err(io_err)?;
        self.refold();
        Ok(())
    }

    fn refold(&mut self) {
        self.state = fold(&self.app.system_prompt(), &self.ledger.lines);
    }

    fn end(&mut self, status: RunStatus, summary: String) -> Result<(), String> {
        self.append(Event::RunFinished { status, summary })
    }

    /// Finishes the run once the model has called `finish` and nothing is waiting for a decision.
    fn finish_if_settled(&mut self) -> Result<(), String> {
        if self.state.status != Some(RunStatus::Running) {
            return Ok(());
        }
        match self.state.finish_summary.clone() {
            Some(summary) => self.end(RunStatus::Finished, summary),
            None => Ok(()),
        }
    }

    fn tool_schemas(&self) -> Vec<Value> {
        let mut tools = builtin_tools();
        tools.extend(self.app.tools().iter().map(tool_schema));
        tools
    }

    /// One model turn: call the model, then handle each tool call in order.
    pub fn step(&mut self) -> Result<(), String> {
        if self.state.status != Some(RunStatus::Running) {
            return Ok(());
        }
        if self.state.turns >= MAX_TURNS {
            return self.end(
                RunStatus::Failed,
                format!("budget: stopped after {MAX_TURNS} model turns"),
            );
        }
        if self.state.tool_calls >= MAX_TOOL_CALLS {
            return self.end(
                RunStatus::Failed,
                format!("budget: stopped after {MAX_TOOL_CALLS} tool calls"),
            );
        }
        let tools = self.tool_schemas();
        let (message, usage) = match self
            .model
            .next(self.state.turns, &self.state.messages, &tools)
        {
            Ok(r) => r,
            Err(e) => return self.end(RunStatus::Failed, e),
        };
        self.append(Event::ModelTurn {
            message: message.clone(),
            usage,
        })?;
        let calls = tool_calls(&message);
        if calls.is_empty() {
            let text = message["content"].as_str().unwrap_or_default().to_string();
            return self.end(RunStatus::Finished, text);
        }
        // The budget is checked once per turn, so a turn with many calls must not run past it.
        // `state.tool_calls` already counts this whole turn, so subtract it to find the room left before it.
        let room = MAX_TOOL_CALLS.saturating_sub(self.state.tool_calls - calls.len());
        for (i, c) in calls.into_iter().enumerate() {
            if i < room {
                self.handle(c)?;
            } else {
                let note = "Not run: the tool call budget is used up.".to_string();
                self.tool_result(&c, false, note)?;
            }
        }
        self.finish_if_settled()
    }

    fn handle(&mut self, c: Call) -> Result<(), String> {
        let args = match &c.args {
            Ok(a) => a.clone(),
            Err(e) => return self.tool_result(&c, false, e.clone()),
        };
        match c.name.as_str() {
            "claim" => match self.record_claim(&args) {
                Ok(note) => self.tool_result(&c, true, note),
                Err(e) => self.tool_result(&c, false, e),
            },
            "finish" => match crate::args::<FinishArgs>(&args) {
                Ok(_) => self.tool_result(
                    &c,
                    true,
                    "Recorded. The run ends when nothing is waiting for a decision.".into(),
                ),
                Err(e) => self.tool_result(&c, false, e),
            },
            name => match self
                .app
                .tools()
                .iter()
                .find(|t| t.name == name)
                .map(|t| t.kind)
            {
                Some(Kind::Probe) => self.probe(&c, &args),
                Some(Kind::Effect) => self.propose(&c, &args),
                None => self.tool_result(&c, false, format!("unknown tool {name}")),
            },
        }
    }

    fn tool_result(&mut self, c: &Call, ok: bool, content: String) -> Result<(), String> {
        self.append(result_event(&c.id, &c.name, ok, content))
    }

    fn probe(&mut self, c: &Call, args: &Value) -> Result<(), String> {
        let result = {
            let mut sink = EvidenceSink {
                ledger: &mut self.ledger,
                next_id: self.state.evidence.len(),
            };
            self.app.probe(&mut sink, &c.name, args)
        };
        self.refold();
        match result {
            Ok(text) => self.tool_result(c, true, text),
            Err(e) if e.starts_with("blocked: ") => {
                self.append(Event::Blocked {
                    call_id: c.id.clone(),
                    tool: c.name.clone(),
                    reason: e.clone(),
                })?;
                self.tool_result(c, false, e)
            }
            Err(e) => self.tool_result(c, false, e),
        }
    }

    /// Effects are not run here. Their preview and digest are recorded, and the run waits for a decision.
    fn propose(&mut self, c: &Call, args: &Value) -> Result<(), String> {
        let p = match self.app.preview(&c.name, args) {
            Ok(p) => p,
            Err(e) => return self.tool_result(c, false, e),
        };
        let influenced_by: Vec<String> = self.state.evidence.iter().map(|e| e.id.clone()).collect();
        let digest = proposal_digest(&c.name, args, &p.inputs, &influenced_by);
        let id = format!("P{}", self.state.proposals.len() + 1);
        let (summary, hidden_s) = reveal_hidden(&p.summary);
        let (detail, hidden_d) = reveal_hidden(&p.detail);
        let mut warnings = p.warnings;
        if hidden_s || hidden_d {
            warnings.push("The change contains invisible or direction-changing characters. They are shown as <U+....> here; the approved bytes are exactly the ones in the arguments.".into());
        }
        self.append(Event::Proposed {
            id,
            call_id: c.id.clone(),
            tool: c.name.clone(),
            args: args.clone(),
            summary,
            detail,
            warnings,
            inputs: p.inputs,
            influenced_by,
            digest,
        })
    }

    fn quote_in_evidence(&self, id: &str, quote: &str) -> bool {
        let Some(ev) = self.state.evidence.iter().find(|e| e.id == id) else {
            return false;
        };
        self.ledger
            .blob(&ev.sha256)
            .is_ok_and(|text| quote_found(&text, quote))
    }

    /// Checks each cite against the stored evidence. Any missing quote makes the claim unsupported.
    fn record_claim(&mut self, args: &Value) -> Result<String, String> {
        let a: ClaimArgs = crate::args(args)?;
        let cites: Vec<Cite> = a
            .cites
            .into_iter()
            .map(|c| {
                let found = self.quote_in_evidence(&c.evidence_id, &c.quote);
                Cite {
                    evidence_id: c.evidence_id,
                    quote: c.quote,
                    found,
                }
            })
            .collect();
        let missing: Vec<&str> = cites
            .iter()
            .filter(|c| !c.found)
            .map(|c| c.evidence_id.as_str())
            .collect();
        let status = if cites.is_empty() || !missing.is_empty() {
            ClaimStatus::Unsupported
        } else {
            a.status
        };
        let note = if cites.is_empty() {
            "; no evidence cited".to_string()
        } else if missing.is_empty() {
            String::new()
        } else {
            format!("; quote not found in {}", missing.join(", "))
        };
        let msg = format!("recorded as {}{note}", status_word(status));
        self.append(Event::Claim {
            text: a.text,
            status,
            cites,
        })?;
        Ok(msg)
    }

    /// Human decision on a proposal. Returns the outcome: "approved", "denied", or the stale reason.
    pub fn decide(&mut self, id: &str, approve: bool, seen_digest: &str) -> Result<String, String> {
        if self.state.status != Some(RunStatus::Waiting) {
            return Err("this run is not waiting for a decision".into());
        }
        let p = self
            .state
            .proposals
            .iter()
            .find(|p| p.id == id)
            .cloned()
            .ok_or_else(|| format!("no proposal {id}"))?;
        if p.status != PStatus::Pending {
            return Err(format!("{id} is already {}", p.status.word()));
        }
        if seen_digest != p.digest {
            return Err("this card is out of date; reload the page".into());
        }
        if !approve {
            self.append(Event::Decided {
                proposal: id.into(),
                approve: false,
                digest: p.digest.clone(),
                reason: "denied by user".into(),
            })?;
            self.append(result_event(&p.call_id, &p.tool, false, DENIED.into()))?;
            self.finish_if_settled()?;
            return Ok("denied".into());
        }
        let fresh = match self.app.preview(&p.tool, &p.args) {
            Ok(f) => f,
            Err(e) => return self.refuse_stale(&p, format!("stale: {e}")),
        };
        let digest_now = proposal_digest(&p.tool, &p.args, &fresh.inputs, &p.influenced_by);
        if digest_now != p.digest {
            let changed = changed_inputs(&p.inputs, &fresh.inputs);
            return self.refuse_stale(&p, format!("stale: {changed} changed since the proposal"));
        }
        self.append(Event::Decided {
            proposal: id.into(),
            approve: true,
            digest: p.digest.clone(),
            reason: String::new(),
        })?;
        self.execute(&p, &digest_now)?;
        Ok("approved".into())
    }

    fn refuse_stale(&mut self, p: &ProposalView, reason: String) -> Result<String, String> {
        self.append(Event::Decided {
            proposal: p.id.clone(),
            approve: false,
            digest: p.digest.clone(),
            reason: reason.clone(),
        })?;
        self.append(result_event(
            &p.call_id,
            &p.tool,
            false,
            stale_note(&reason),
        ))?;
        self.finish_if_settled()?;
        Ok(reason)
    }

    /// Runs an approved effect once. `EffectStarted` is synced before `apply` is called.
    // shortcut: an input can change between the stale check in `decide` and `apply()` (milliseconds). Acceptable for one local user; upgrade by making apply re-hash and compare inside the write if multi-writer use appears.
    fn execute(&mut self, p: &ProposalView, digest_now: &str) -> Result<(), String> {
        let clean: Clean<Value> =
            taint::release(Tainted::new(p.args.clone()), &p.digest, digest_now)?;
        self.append(Event::EffectStarted {
            proposal: p.id.clone(),
            digest: p.digest.clone(),
        })?;
        let (ok, receipt) = match self.app.apply(&p.tool, clean) {
            Ok(r) => (true, r),
            Err(e) => (false, e),
        };
        self.append(Event::EffectDone {
            proposal: p.id.clone(),
            ok,
            receipt: receipt.clone(),
        })?;
        self.append(result_event(&p.call_id, &p.tool, ok, receipt))?;
        self.finish_if_settled()
    }
}

/// Makes invisible and direction-changing characters visible, so an approval card cannot read
/// differently from the bytes it approves ("Trojan Source"). Returns the text and whether any were found.
pub fn reveal_hidden(s: &str) -> (String, bool) {
    let hidden = |c: char| matches!(c, '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}' | '\u{FEFF}');
    if !s.chars().any(hidden) {
        return (s.to_string(), false);
    }
    let out = s
        .chars()
        .map(|c| {
            if hidden(c) {
                format!("<U+{:04X}>", c as u32)
            } else {
                c.to_string()
            }
        })
        .collect();
    (out, true)
}

/// Names the inputs whose hashes differ from the proposal, for the stale reason.
fn changed_inputs(before: &BTreeMap<String, String>, now: &BTreeMap<String, String>) -> String {
    let mut keys: Vec<&String> = before.keys().chain(now.keys()).collect();
    keys.sort();
    keys.dedup();
    let changed: Vec<&str> = keys
        .into_iter()
        .filter(|k| before.get(*k) != now.get(*k))
        .map(|k| k.as_str())
        .collect();
    if changed.is_empty() {
        "the inputs".into()
    } else {
        changed.join(", ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ledger::{Cite, ClaimStatus},
        testkit::{
            Counter, applied, asst, call, fixture, run_to_end, set_value, start, text, value,
        },
        testutil::TempDir,
    };

    fn line(seq: u64, event: Event) -> Line {
        Line {
            seq,
            prev: String::new(),
            at_ms: 0,
            event,
            hash: String::new(),
        }
    }

    fn proposed() -> Event {
        Event::Proposed {
            id: "P1".into(),
            call_id: "c1".into(),
            tool: "bump".into(),
            args: json!({"n": 1}),
            summary: "add 1".into(),
            detail: String::new(),
            warnings: vec!["careful".into()],
            inputs: BTreeMap::from([("counter".to_string(), "0".to_string())]),
            influenced_by: vec!["E1".into()],
            digest: "d1".into(),
        }
    }

    fn kind_of(e: &Event) -> String {
        serde_json::to_value(e).unwrap()["kind"]
            .as_str()
            .unwrap()
            .to_string()
    }

    fn result(s: &Session<Counter>, call_id: &str) -> (bool, String) {
        s.ledger
            .lines
            .iter()
            .find_map(|l| match &l.event {
                Event::ToolResult {
                    call_id: c,
                    ok,
                    content,
                    ..
                } if c == call_id => Some((*ok, content.clone())),
                _ => None,
            })
            .unwrap_or_else(|| panic!("no ToolResult for {call_id}"))
    }

    fn run_started() -> Event {
        Event::RunStarted {
            app: "counter".into(),
            task: "t".into(),
            target: "x".into(),
            mode: Mode::Fixture,
            model: "m".into(),
            disclosure: String::new(),
        }
    }

    #[test]
    fn fold_status_transitions() {
        let lines = vec![line(0, run_started()), line(1, proposed())];
        let s = fold("sys", &lines);
        assert_eq!(s.status, Some(RunStatus::Waiting));
        assert_eq!(s.proposals[0].status, PStatus::Pending);
        assert_eq!(
            s.messages[0]["content"], "sys",
            "messages start with the system prompt"
        );

        let mut more = lines.clone();
        more.push(line(
            2,
            Event::Decided {
                proposal: "P1".into(),
                approve: true,
                digest: "d1".into(),
                reason: String::new(),
            },
        ));
        let s = fold("sys", &more);
        assert_eq!(
            s.status,
            Some(RunStatus::Running),
            "nothing pending any more"
        );
        assert_eq!(s.proposals[0].status, PStatus::Approved);

        more.push(line(
            3,
            Event::EffectStarted {
                proposal: "P1".into(),
                digest: "d1".into(),
            },
        ));
        more.push(line(
            4,
            Event::EffectDone {
                proposal: "P1".into(),
                ok: true,
                receipt: "value 1".into(),
            },
        ));
        more.push(line(
            5,
            Event::RunFinished {
                status: RunStatus::Finished,
                summary: "ok".into(),
            },
        ));
        let s = fold("sys", &more);
        assert_eq!(s.proposals[0].status, PStatus::Done);
        assert_eq!(s.proposals[0].receipt, "value 1");
        assert_eq!(s.status, Some(RunStatus::Finished));
        assert_eq!(s.summary, "ok");
        assert_eq!(s.timeline.len(), 6);
    }

    #[test]
    fn fold_denied_is_not_pending() {
        let lines = vec![
            line(0, run_started()),
            line(1, proposed()),
            line(
                2,
                Event::Decided {
                    proposal: "P1".into(),
                    approve: false,
                    digest: "d1".into(),
                    reason: "denied by user".into(),
                },
            ),
        ];
        let s = fold("", &lines);
        assert_eq!(s.proposals[0].status, PStatus::Denied);
        assert_eq!(s.status, Some(RunStatus::Running));
    }

    #[test]
    fn digest_changes_with_any_input() {
        let args = json!({"n": 1});
        let inputs = BTreeMap::from([("f".to_string(), "aa".to_string())]);
        let infl = vec!["E1".to_string()];
        let base = proposal_digest("bump", &args, &inputs, &infl);
        assert_ne!(
            base,
            proposal_digest("other", &args, &inputs, &infl),
            "tool"
        );
        assert_ne!(
            base,
            proposal_digest("bump", &json!({"n": 2}), &inputs, &infl),
            "args"
        );
        let changed = BTreeMap::from([("f".to_string(), "bb".to_string())]);
        assert_ne!(
            base,
            proposal_digest("bump", &args, &changed, &infl),
            "input hash"
        );
        assert_ne!(
            base,
            proposal_digest("bump", &args, &inputs, &["E2".to_string()]),
            "influenced_by"
        );
        assert_eq!(
            base,
            proposal_digest("bump", &args, &inputs, &infl),
            "stable"
        );
    }

    #[test]
    fn quotes() {
        let blob = "Submissions close\n  December 4, 2026   at 2:00 PM PST.";
        assert!(
            quote_found(blob, "December 4, 2026 at 2:00 PM PST"),
            "whitespace collapses"
        );
        assert!(
            !quote_found(blob, "December 5, 2026 at 2:00 PM PST"),
            "a changed quote is not found"
        );
        assert!(
            !quote_found(blob, "close"),
            "under 8 characters never matches"
        );
    }

    #[test]
    fn effect_waits_for_approval() {
        let d = TempDir::new("s-wait");
        let mut s = start(
            d.path(),
            vec![asst(vec![call("c1", "bump", json!({"n": 1}))])],
        );
        s.step().unwrap();
        assert_eq!(s.state.status, Some(RunStatus::Waiting));
        assert_eq!(s.state.proposals[0].status, PStatus::Pending);
        assert_eq!(applied(), 0, "nothing ran before approval");
    }

    #[test]
    fn deny_never_executes() {
        let d = TempDir::new("s-deny");
        let mut s = start(
            d.path(),
            vec![asst(vec![call("c1", "bump", json!({"n": 1}))]), text("ok")],
        );
        s.step().unwrap();
        let digest = s.state.proposals[0].digest.clone();
        assert_eq!(s.decide("P1", false, &digest).unwrap(), "denied");
        assert_eq!(s.state.status, Some(RunStatus::Running));
        run_to_end(&mut s);
        assert_eq!(applied(), 0, "denied effect ran 0 times");
        assert_eq!(value(), 0);
        assert_eq!(
            result(&s, "c1"),
            (
                false,
                "The user denied this action. Do not retry it.".into()
            )
        );
        assert_eq!(s.state.status, Some(RunStatus::Finished));
    }

    #[test]
    fn decide_rejects_wrong_seen_digest() {
        let d = TempDir::new("s-wrongdigest");
        let mut s = start(
            d.path(),
            vec![asst(vec![call("c1", "bump", json!({"n": 1}))])],
        );
        s.step().unwrap();
        let before = s.ledger.lines.len();
        assert!(s.decide("P1", true, "not-the-digest").is_err());
        assert_eq!(s.ledger.lines.len(), before, "nothing was written");
        assert_eq!(applied(), 0);
    }

    #[test]
    fn stale_input_blocks_effect() {
        let d = TempDir::new("s-stale");
        let mut s = start(
            d.path(),
            vec![asst(vec![call("c1", "bump", json!({"n": 1}))]), text("ok")],
        );
        s.step().unwrap();
        let digest = s.state.proposals[0].digest.clone();
        set_value(5); // the input changed after the proposal
        let outcome = s.decide("P1", true, &digest).unwrap();
        assert!(outcome.starts_with("stale:"), "{outcome}");
        assert!(
            outcome.contains("counter"),
            "names the changed input: {outcome}"
        );
        assert_eq!(applied(), 0, "stale effect ran 0 times");
        assert_eq!(s.state.proposals[0].status, PStatus::Stale);
    }

    #[test]
    fn double_decide_executes_once() {
        let d = TempDir::new("s-double");
        let mut s = start(
            d.path(),
            vec![asst(vec![call("c1", "bump", json!({"n": 1}))]), text("ok")],
        );
        s.step().unwrap();
        let digest = s.state.proposals[0].digest.clone();
        s.decide("P1", true, &digest).unwrap();
        assert!(
            s.decide("P1", true, &digest).is_err(),
            "second approval is refused"
        );
        assert!(
            s.decide("P1", false, &digest).is_err(),
            "and so is a deny after approval"
        );
        run_to_end(&mut s);
        assert_eq!(applied(), 1, "effect ran 1 time");
        assert_eq!(value(), 1);
    }

    #[test]
    fn effect_events_bracket_apply() {
        let d = TempDir::new("s-bracket");
        let mut s = start(
            d.path(),
            vec![asst(vec![call("c1", "bump", json!({"n": 1}))])],
        );
        s.step().unwrap();
        let digest = s.state.proposals[0].digest.clone();
        s.decide("P1", true, &digest).unwrap();
        let tail: Vec<String> = s
            .ledger
            .lines
            .iter()
            .rev()
            .take(4)
            .rev()
            .map(|l| kind_of(&l.event))
            .collect();
        assert_eq!(
            tail,
            ["decided", "effect_started", "effect_done", "tool_result"]
        );
    }

    #[test]
    fn resume_restores_pending() {
        let d = TempDir::new("s-resume");
        let script = vec![asst(vec![call("c1", "bump", json!({"n": 1}))])];
        let mut s = start(d.path(), script.clone());
        s.step().unwrap();
        let digest = s.state.proposals[0].digest.clone();
        let run = s.run_dir().to_path_buf();
        drop(s); // the process goes away after the proposal
        let mut r = Session::<Counter>::resume(&run, fixture(script)).unwrap();
        assert_eq!(r.state.status, Some(RunStatus::Waiting));
        assert_eq!(r.state.proposals[0].status, PStatus::Pending);
        r.decide("P1", true, &digest).unwrap();
        assert_eq!(applied(), 1, "effect ran once after resume");
    }

    #[test]
    fn crash_mid_effect_is_outcome_unknown() {
        let d = TempDir::new("s-crash");
        let script = vec![asst(vec![call("c1", "bump", json!({"n": 1}))]), text("x")];
        let mut s = start(d.path(), script.clone());
        s.step().unwrap();
        let digest = s.state.proposals[0].digest.clone();
        // Simulate a crash right after EffectStarted was synced.
        s.ledger
            .append(Event::Decided {
                proposal: "P1".into(),
                approve: true,
                digest: digest.clone(),
                reason: String::new(),
            })
            .unwrap();
        s.ledger
            .append(Event::EffectStarted {
                proposal: "P1".into(),
                digest,
            })
            .unwrap();
        let run = s.run_dir().to_path_buf();
        drop(s);
        let r = Session::<Counter>::resume(&run, fixture(script)).unwrap();
        assert_eq!(applied(), 0, "resume never re-executes");
        assert_eq!(r.state.status, Some(RunStatus::NeedsReview));
        assert_eq!(r.state.proposals[0].status, PStatus::Started);
        assert_eq!(result(&r, "c1"), (false, NOT_RETRIED.into()));
    }

    #[test]
    fn approved_but_not_started_is_not_executed() {
        let d = TempDir::new("s-approved");
        let script = vec![asst(vec![call("c1", "bump", json!({"n": 1}))])];
        let mut s = start(d.path(), script.clone());
        s.step().unwrap();
        let digest = s.state.proposals[0].digest.clone();
        s.ledger
            .append(Event::Decided {
                proposal: "P1".into(),
                approve: true,
                digest,
                reason: String::new(),
            })
            .unwrap();
        let run = s.run_dir().to_path_buf();
        drop(s);
        let r = Session::<Counter>::resume(&run, fixture(script)).unwrap();
        assert_eq!(applied(), 0);
        assert_eq!(r.state.status, Some(RunStatus::NeedsReview));
        assert!(result(&r, "c1").1.starts_with("Not executed:"));
    }

    #[test]
    fn crash_mid_turn_answers_every_call() {
        let d = TempDir::new("s-midturn");
        let script = vec![
            asst(vec![
                call("c1", "peek", json!({})),
                call("c2", "peek", json!({})),
            ]),
            text("done"),
        ];
        let mut s = start(d.path(), script.clone());
        // Simulate a crash after the model turn and the first result, before the second.
        s.ledger
            .append(Event::ModelTurn {
                message: script[0].clone(),
                usage: None,
            })
            .unwrap();
        s.ledger
            .append(result_event("c1", "peek", true, "ok".into()))
            .unwrap();
        let run = s.run_dir().to_path_buf();
        drop(s);
        let mut r = Session::<Counter>::resume(&run, fixture(script)).unwrap();
        assert_eq!(
            result(&r, "c2"),
            (false, NOT_RUN.into()),
            "the missing call gets a result"
        );
        assert_eq!(result(&r, "c1").1, "ok", "the answered call is left alone");
        let tool_msgs = r
            .state
            .messages
            .iter()
            .filter(|m| m["role"] == "tool")
            .count();
        assert_eq!(
            tool_msgs, 2,
            "the next request has a tool message for every call"
        );
        run_to_end(&mut r);
        assert_eq!(r.state.status, Some(RunStatus::Finished));
    }

    #[test]
    fn failed_run_continues_after_resume() {
        let d = TempDir::new("s-failresume");
        // The script ends after one turn, like a model that timed out on turn 2.
        let first = vec![asst(vec![call("c1", "peek", json!({}))])];
        let mut s = start(d.path(), first.clone());
        run_to_end(&mut s);
        assert_eq!(s.state.status, Some(RunStatus::Failed));
        let run = s.run_dir().to_path_buf();
        drop(s);
        let mut more = first;
        more.push(text("all done"));
        let mut r = Session::<Counter>::resume(&run, fixture(more)).unwrap();
        assert_eq!(r.state.status, Some(RunStatus::Running));
        assert!(
            r.ledger
                .lines
                .iter()
                .any(|l| matches!(l.event, Event::RunResumed { .. }))
        );
        run_to_end(&mut r);
        assert_eq!(r.state.status, Some(RunStatus::Finished));
        assert_eq!(r.state.summary, "all done");
    }

    #[test]
    fn budget_stop_stays_stopped_after_resume() {
        let d = TempDir::new("s-budgetresume");
        let turns: Vec<Value> = (0..30)
            .map(|i| asst(vec![call(&format!("c{i}"), "peek", json!({}))]))
            .collect();
        let mut s = start(d.path(), turns.clone());
        run_to_end(&mut s);
        assert!(s.state.summary.starts_with("budget:"));
        let run = s.run_dir().to_path_buf();
        drop(s);
        let r = Session::<Counter>::resume(&run, fixture(turns)).unwrap();
        assert_eq!(
            r.state.status,
            Some(RunStatus::Failed),
            "a budget stop is final"
        );
        assert!(
            !r.ledger
                .lines
                .iter()
                .any(|l| matches!(l.event, Event::RunResumed { .. }))
        );
    }
    #[test]
    fn crash_mid_turn_with_reused_call_ids() {
        // Some servers number calls call_0, call_1 in every turn.
        let d = TempDir::new("s-reuse");
        let turn = asst(vec![call("call_0", "peek", json!({}))]);
        let script = vec![turn.clone(), turn.clone(), text("done")];
        let mut s = start(d.path(), script.clone());
        s.step().unwrap(); // turn 1 completes: call_0 has a result
        // Turn 2 reuses call_0, and the process stops before its result.
        s.ledger
            .append(Event::ModelTurn {
                message: turn,
                usage: None,
            })
            .unwrap();
        let run = s.run_dir().to_path_buf();
        drop(s);
        let mut r = Session::<Counter>::resume(&run, fixture(script)).unwrap();
        let results = r
            .ledger
            .lines
            .iter()
            .filter(
                |l| matches!(&l.event, Event::ToolResult { call_id, .. } if call_id == "call_0"),
            )
            .count();
        assert_eq!(results, 2, "the second turn's call_0 gets its own result");
        run_to_end(&mut r);
        assert_eq!(r.state.status, Some(RunStatus::Finished));
    }
    /// Runs a bump proposal, writes the given events by hand as if the process then stopped before the
    /// proposal's tool result, and resumes.
    fn resume_after(d: &TempDir, settle: impl Fn(&str) -> Vec<Event>) -> Session<Counter> {
        let script = vec![
            asst(vec![call("c1", "bump", json!({"n": 1}))]),
            text("done"),
        ];
        let mut s = start(d.path(), script.clone());
        s.step().unwrap();
        let digest = s.state.proposals[0].digest.clone();
        for e in settle(&digest) {
            s.ledger.append(e).unwrap();
        }
        let run = s.run_dir().to_path_buf();
        drop(s);
        Session::<Counter>::resume(&run, fixture(script)).unwrap()
    }

    #[test]
    fn crash_after_deny_answers_with_the_denial() {
        let d = TempDir::new("s-settled-deny");
        let r = resume_after(&d, |dg| {
            vec![Event::Decided {
                proposal: "P1".into(),
                approve: false,
                digest: dg.into(),
                reason: "denied by user".into(),
            }]
        });
        assert_eq!(
            result(&r, "c1"),
            (false, DENIED.into()),
            "the model is told it was denied, not 'not run'"
        );
        assert_eq!(
            r.state.status,
            Some(RunStatus::Running),
            "the run can continue"
        );
    }

    #[test]
    fn crash_after_effect_done_answers_with_the_receipt() {
        let d = TempDir::new("s-settled-done");
        let r = resume_after(&d, |dg| {
            vec![
                Event::Decided {
                    proposal: "P1".into(),
                    approve: true,
                    digest: dg.into(),
                    reason: String::new(),
                },
                Event::EffectStarted {
                    proposal: "P1".into(),
                    digest: dg.into(),
                },
                Event::EffectDone {
                    proposal: "P1".into(),
                    ok: true,
                    receipt: "value 1".into(),
                },
            ]
        });
        assert_eq!(
            result(&r, "c1"),
            (true, "value 1".into()),
            "a done effect is reported done, so it is not proposed again"
        );
        assert_eq!(applied(), 0, "resume never runs an effect");
    }

    #[test]
    fn card_reveals_hidden_characters() {
        let (shown, found) = reveal_hidden("total = a\u{202E} // b");
        assert!(found);
        assert_eq!(shown, "total = a<U+202E> // b");
        assert_eq!(
            reveal_hidden("plain text"),
            ("plain text".to_string(), false)
        );
    }
    #[test]
    fn resume_finishes_when_finish_was_already_called() {
        let d = TempDir::new("s-finish-resume");
        let script = vec![asst(vec![call(
            "c1",
            "finish",
            json!({"summary": "all done"}),
        )])];
        let mut s = start(d.path(), script.clone());
        // Crash after the finish call's result, before RunFinished.
        s.ledger
            .append(Event::ModelTurn {
                message: script[0].clone(),
                usage: None,
            })
            .unwrap();
        s.ledger
            .append(result_event("c1", "finish", true, "Recorded.".into()))
            .unwrap();
        let run = s.run_dir().to_path_buf();
        drop(s);
        let r = Session::<Counter>::resume(&run, fixture(script)).unwrap();
        assert_eq!(r.state.status, Some(RunStatus::Finished));
        assert_eq!(r.state.summary, "all done");
    }

    #[test]
    fn start_records_absolute_target() {
        let d = TempDir::new("s-target");
        let s = Session::<Counter>::start(
            d.path(),
            Path::new("."),
            "t",
            Mode::Fixture,
            fixture(vec![]),
        )
        .unwrap();
        assert!(
            Path::new(&s.state.target).is_absolute(),
            "a relative target is stored absolute: {}",
            s.state.target
        );
    }

    #[test]
    fn claim_with_missing_quote_is_unsupported() {
        let d = TempDir::new("s-claim");
        let mut s = start(
            d.path(),
            vec![
                asst(vec![call("c1", "peek", json!({}))]),
                asst(vec![
                    call(
                        "c2",
                        "claim",
                        json!({"text": "ok", "status": "supported",
                    "cites": [{"evidence_id": "E1", "quote": "hello world text"}]}),
                    ),
                    call(
                        "c3",
                        "claim",
                        json!({"text": "bad", "status": "supported",
                    "cites": [{"evidence_id": "E1", "quote": "this sentence is nowhere"}]}),
                    ),
                    call(
                        "c4",
                        "claim",
                        json!({"text": "unknown id", "status": "supported",
                    "cites": [{"evidence_id": "E9", "quote": "hello world text"}]}),
                    ),
                ]),
                text("done"),
            ],
        );
        run_to_end(&mut s);
        let st: Vec<ClaimStatus> = s.state.claims.iter().map(|c| c.status).collect();
        assert_eq!(
            st,
            [
                ClaimStatus::Supported,
                ClaimStatus::Unsupported,
                ClaimStatus::Unsupported
            ]
        );
        assert!(!s.state.claims[1].cites[0].found);
        assert_eq!(
            s.state.claims[0].cites,
            vec![Cite {
                evidence_id: "E1".into(),
                quote: "hello world text".into(),
                found: true
            }]
        );
        assert!(result(&s, "c3").1.contains("quote not found in E1"));
    }

    #[test]
    fn unknown_tool_is_error() {
        let d = TempDir::new("s-unknown");
        let mut s = start(
            d.path(),
            vec![asst(vec![call("c1", "nope", json!({}))]), text("done")],
        );
        run_to_end(&mut s);
        assert_eq!(result(&s, "c1"), (false, "unknown tool nope".into()));
        assert_eq!(s.state.status, Some(RunStatus::Finished));
    }

    #[test]
    fn extra_field_is_error() {
        let d = TempDir::new("s-extra");
        let mut s = start(
            d.path(),
            vec![
                asst(vec![call("c1", "peek", json!({"x": 1}))]),
                asst(vec![call("c2", "bump", json!({"n": 1, "extra": 2}))]),
                text("done"),
            ],
        );
        run_to_end(&mut s);
        assert!(!result(&s, "c1").0, "peek refuses an unknown field");
        assert!(!result(&s, "c2").0, "bump refuses an unknown field");
        assert!(
            s.state.proposals.is_empty(),
            "no proposal for the refused effect"
        );
        assert_eq!(applied(), 0);
    }

    #[test]
    fn budget_stops_run() {
        let d = TempDir::new("s-budget");
        let turns: Vec<Value> = (0..30)
            .map(|i| asst(vec![call(&format!("c{i}"), "peek", json!({}))]))
            .collect();
        let mut s = start(d.path(), turns);
        run_to_end(&mut s);
        assert_eq!(s.state.status, Some(RunStatus::Failed));
        assert!(
            s.state.summary.starts_with("budget:"),
            "{}",
            s.state.summary
        );
        assert_eq!(s.state.turns, crate::MAX_TURNS);
    }

    #[test]
    fn budget_caps_calls_in_one_turn() {
        let d = TempDir::new("s-onceturn");
        let calls: Vec<Value> = (0..50)
            .map(|i| call(&format!("c{i}"), "peek", json!({})))
            .collect();
        let mut s = start(d.path(), vec![asst(calls)]);
        s.step().unwrap();
        let results: Vec<(bool, String)> = (0..50).map(|i| result(&s, &format!("c{i}"))).collect();
        let ran = results.iter().filter(|r| r.0).count();
        assert_eq!(ran, crate::MAX_TOOL_CALLS, "only the budget ran");
        assert_eq!(
            s.state.evidence.len(),
            crate::MAX_TOOL_CALLS,
            "no probe ran past the budget"
        );
        assert!(
            results[crate::MAX_TOOL_CALLS..]
                .iter()
                .all(|r| !r.0 && r.1.contains("budget"))
        );
        assert_eq!(
            results.len(),
            50,
            "every call got a result, so the next request is valid"
        );
    }
    #[test]
    fn tool_result_truncated() {
        let d = TempDir::new("s-trunc");
        let mut s = start(
            d.path(),
            vec![asst(vec![call("c1", "big", json!({}))]), text("done")],
        );
        run_to_end(&mut s);
        let (ok, content) = result(&s, "c1");
        assert!(ok);
        assert!(
            content.chars().count() <= crate::TOOL_RESULT_CHARS,
            "{} chars",
            content.chars().count()
        );
        assert!(content.contains("truncated"));
        let sha = s.state.evidence[0].sha256.clone();
        assert_eq!(
            s.ledger.blob(&sha).unwrap().len(),
            20_000,
            "the blob keeps the full text"
        );
    }

    #[test]
    fn blocked_probe_records_blocked() {
        let d = TempDir::new("s-blocked");
        let mut s = start(
            d.path(),
            vec![asst(vec![call("c1", "blocked", json!({}))]), text("done")],
        );
        run_to_end(&mut s);
        assert!(s.ledger.lines.iter().any(
            |l| matches!(&l.event, Event::Blocked { reason, .. } if reason.contains("test refusal"))
        ));
        assert_eq!(result(&s, "c1"), (false, "blocked: test refusal".into()));
        assert!(
            s.state
                .timeline
                .iter()
                .any(|t| t.text.contains("⛔ blocked"))
        );
    }

    #[test]
    fn finish_waits_for_pending() {
        let d = TempDir::new("s-finish");
        let mut s = start(
            d.path(),
            vec![asst(vec![
                call("c1", "bump", json!({"n": 1})),
                call("c2", "finish", json!({"summary": "done"})),
            ])],
        );
        s.step().unwrap();
        assert_eq!(
            s.state.status,
            Some(RunStatus::Waiting),
            "finish does not end a waiting run"
        );
        let digest = s.state.proposals[0].digest.clone();
        s.decide("P1", true, &digest).unwrap();
        assert_eq!(s.state.status, Some(RunStatus::Finished));
        assert_eq!(s.state.summary, "done");
    }

    #[test]
    fn approved_result_names_the_effect() {
        let d = TempDir::new("s-result");
        let mut s = start(
            d.path(),
            vec![asst(vec![call("c1", "bump", json!({"n": 2}))]), text("ok")],
        );
        s.step().unwrap();
        let digest = s.state.proposals[0].digest.clone();
        s.decide("P1", true, &digest).unwrap();
        assert_eq!(result(&s, "c1"), (true, "value 2".into()));
        assert_eq!(s.state.proposals[0].receipt, "value 2");
    }
}
