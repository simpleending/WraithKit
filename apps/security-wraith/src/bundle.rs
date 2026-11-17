//! Bounded, untrusted 1-SEC-compatible export envelope. This is not a live connector.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;

pub const MAX_BYTES: u64 = 65_536;
pub const FORMAT: &str = "security-wraith-bundle-v1";

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct Bundle {
    pub format: String,
    pub provenance: String,
    pub sanitized: bool,
    pub source_note: String,
    pub incident_id: String,
    pub correlated_alert_id: String,
    pub asset: Asset,
    pub ingestion: Vec<Receipt>,
    pub events: Vec<SecurityEvent>,
    pub alerts: Vec<Alert>,
    pub context: Vec<String>,
}

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct Asset {
    pub id: String,
    pub kind: String,
    pub owner: String,
    pub criticality: String,
}

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct Receipt {
    pub event_id: String,
    pub http_status: u16,
    pub status: String,
    pub meaning: String,
}

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct SecurityEvent {
    pub id: String,
    pub timestamp: String,
    pub source: String,
    pub module: String,
    #[serde(rename = "type")]
    pub event_type: String,
    pub severity: String,
    pub summary: String,
    #[serde(default)]
    pub details: Value,
    #[serde(default)]
    pub source_ip: String,
    #[serde(default)]
    pub dest_ip: String,
    #[serde(default)]
    pub user_agent: String,
    #[serde(default)]
    pub request_id: String,
    #[serde(default)]
    pub observation_only: bool,
}

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct Alert {
    pub id: String,
    pub timestamp: String,
    pub module: String,
    #[serde(rename = "type")]
    pub alert_type: String,
    pub severity: String,
    pub status: String,
    pub title: String,
    pub description: String,
    pub event_ids: Vec<String>,
    #[serde(default)]
    pub metadata: Value,
    #[serde(default)]
    pub mitigations: Vec<String>,
    #[serde(default)]
    pub observation_only: bool,
}

pub fn valid_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 80
        && s.bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.'))
}

pub fn parse_bundle(text: &str) -> Result<Bundle, String> {
    let b: Bundle =
        serde_json::from_str(text).map_err(|e| format!("invalid incident.json: {e}"))?;
    if b.format != FORMAT
        || !b.sanitized
        || !matches!(
            b.provenance.as_str(),
            "synthetic_fixture" | "user_supplied_export"
        )
    {
        return Err("bundle must use security-wraith-bundle-v1, a declared provenance, and sanitized: true; remove secrets before importing".into());
    }
    if !valid_id(&b.incident_id) || !valid_id(&b.asset.id) || !valid_id(&b.correlated_alert_id) {
        return Err(
            "incident, asset and correlated alert IDs must be bounded ASCII identifiers".into(),
        );
    }
    if b.events.is_empty()
        || b.events.len() > 32
        || b.alerts.is_empty()
        || b.alerts.len() > 16
        || b.ingestion.len() > 32
        || b.context.len() > 16
    {
        return Err(
            "bundle needs 1-32 events, 1-16 alerts, at most 32 receipts and 16 context notes"
                .into(),
        );
    }
    let mut events = BTreeSet::new();
    for e in &b.events {
        if !valid_id(&e.id)
            || !events.insert(e.id.as_str())
            || !severity(&e.severity)
            || e.timestamp.is_empty()
            || e.source.is_empty()
            || e.module.is_empty()
            || e.event_type.is_empty()
            || e.summary.is_empty()
        {
            return Err("events need unique IDs, timestamps, source/module/type, summary and a known severity".into());
        }
    }
    let mut alerts = BTreeSet::new();
    for a in &b.alerts {
        if !valid_id(&a.id)
            || !alerts.insert(a.id.as_str())
            || !severity(&a.severity)
            || !matches!(
                a.status.as_str(),
                "OPEN" | "ACKNOWLEDGED" | "RESOLVED" | "FALSE_POSITIVE"
            )
            || a.timestamp.is_empty()
            || a.module.is_empty()
            || a.alert_type.is_empty()
            || a.title.is_empty()
            || a.event_ids.is_empty()
            || a.event_ids.len() > 32
            || a.event_ids.iter().any(|id| !events.contains(id.as_str()))
        {
            return Err(
                "alerts need unique IDs, valid status/severity and references to included events"
                    .into(),
            );
        }
    }
    let correlated = b
        .alerts
        .iter()
        .find(|a| a.id == b.correlated_alert_id)
        .ok_or("correlated alert is not included")?;
    if correlated.metadata.get("asset_id").and_then(Value::as_str) != Some(&b.asset.id) {
        return Err(
            "correlated alert metadata.asset_id must match the explicitly scoped asset".into(),
        );
    }
    for r in &b.ingestion {
        if !events.contains(r.event_id.as_str()) || r.http_status != 202 || r.status != "accepted" {
            return Err("ingestion entries represent HTTP 202 accepted queue receipts for included events, not detection verdicts".into());
        }
    }
    Ok(b)
}

fn severity(s: &str) -> bool {
    matches!(s, "INFO" | "LOW" | "MEDIUM" | "HIGH" | "CRITICAL")
}

/// Only a local response-plan simulation. No service reads this as enforcement policy.
#[derive(Deserialize, Serialize, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Simulation {
    pub simulation_only: bool,
    pub asset_id: String,
    pub incident_id: String,
    pub revision: u64,
    pub simulated_state: String,
    pub action: String,
    pub reason: String,
    pub rollback_action: String,
    pub bundle_sha256: String,
}

impl Simulation {
    pub fn initial(b: &Bundle) -> Self {
        Self {
            simulation_only: true,
            asset_id: b.asset.id.clone(),
            incident_id: b.incident_id.clone(),
            revision: 0,
            simulated_state: if b.provenance == "synthetic_fixture" {
                "running"
            } else {
                "not_executed"
            }
            .into(),
            action: String::new(),
            reason: String::new(),
            rollback_action: String::new(),
            bundle_sha256: String::new(),
        }
    }
    pub fn validate(&self, b: &Bundle) -> Result<(), String> {
        if !self.simulation_only
            || self.asset_id != b.asset.id
            || self.incident_id != b.incident_id
            || (b.provenance == "synthetic_fixture"
                && !matches!(self.simulated_state.as_str(), "running" | "paused"))
            || (b.provenance != "synthetic_fixture" && self.simulated_state != "not_executed")
        {
            return Err(
                "response-plan.json must be a simulation for exactly this incident and asset"
                    .into(),
            );
        }
        if self.revision == 0 {
            if self != &Self::initial(b) {
                return Err(
                    "revision zero must be the exact unmodified local initial state".into(),
                );
            }
        } else {
            let pause = self.action == "pause_agent";
            let known = pause || self.action == "resume_agent";
            let expected_rollback = if pause { "resume_agent" } else { "pause_agent" };
            let expected_simulated = if pause { "paused" } else { "running" };
            if !known
                || self.rollback_action != expected_rollback
                || (self.revision % 2 == 1) != pause
                || self.reason.len() < 16
                || self.reason.len() > 500
                || self.reason.chars().any(char::is_control)
                || self.bundle_sha256.len() != 64
                || !self
                    .bundle_sha256
                    .bytes()
                    .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
                || (b.provenance == "synthetic_fixture"
                    && self.simulated_state != expected_simulated)
            {
                return Err("response plan action, revision, rollback, reason, evidence hash and simulated state must be consistent".into());
            }
        }
        Ok(())
    }
}
