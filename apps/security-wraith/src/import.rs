//! Explicit local file handoff, not a live API connector or response executor.
use crate::{
    bundle::{self, Asset},
    storage,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{fs, io::Write, path::Path};
use wraithkit::canonical;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Mapping {
    sanitized: bool,
    incident_id: String,
    selected_alert_id: String,
    asset: Asset,
    source_note: String,
    context: Vec<String>,
}

const EVENT_FIELDS: &[&str] = &[
    "id",
    "timestamp",
    "source",
    "module",
    "type",
    "severity",
    "summary",
    "source_ip",
    "dest_ip",
    "user_agent",
    "request_id",
    "observation_only",
];
const ALERT_FIELDS: &[&str] = &[
    "id",
    "timestamp",
    "module",
    "type",
    "severity",
    "status",
    "title",
    "description",
    "event_ids",
    "mitigations",
    "observation_only",
];

fn select(value: &Value, fields: &[&str]) -> Result<Value, String> {
    let object = value
        .as_object()
        .ok_or("each exported event/alert must be an object")?;
    Ok(Value::Object(
        fields
            .iter()
            .filter_map(|k| object.get(*k).map(|v| ((*k).into(), v.clone())))
            .collect(),
    ))
}

/// Map already-sanitized exports; labels and asset association are operator assertions.
/// Dropping opaque fields is NOT redaction of retained strings.
pub fn convert(events: &str, alerts: &str, mapping: &str) -> Result<String, String> {
    if [events, alerts, mapping]
        .iter()
        .any(|s| s.len() > bundle::MAX_BYTES as usize)
    {
        return Err("each export file must fit 64 KiB".into());
    }
    let map: Mapping = serde_json::from_str(mapping).map_err(|_| "invalid mapping.json schema")?;
    if !map.sanitized || map.source_note.trim().is_empty() || map.source_note.len() > 1000 {
        return Err("mapping requires sanitized:true and a nonempty source_note of at most 1000 bytes; redact retained text before importing".into());
    }
    let source_events: Vec<Value> = serde_json::from_str(events)
        .map_err(|_| "events.json must be an array of exported SecurityEvent objects")?;
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct AlertExport {
        alerts: Vec<Value>,
        total: usize,
    }
    let source_alerts: AlertExport = serde_json::from_str(alerts)
        .map_err(|_| "alerts.json must be the {alerts:[...],total:N} API envelope")?;
    if source_alerts.total != source_alerts.alerts.len() {
        return Err("alerts.json total must equal the included alert count".into());
    }
    if source_events.is_empty()
        || source_events.len() > 32
        || source_alerts.alerts.is_empty()
        || source_alerts.alerts.len() > 16
    {
        return Err("handoff requires 1-32 events and 1-16 alerts".into());
    }
    let mut events = source_events
        .iter()
        .map(|v| select(v, EVENT_FIELDS))
        .collect::<Result<Vec<_>, _>>()?;
    for e in &mut events {
        // Upstream NewSecurityEvent/raised module events may have an empty source.
        // This labels the transport origin; it does not invent authenticated provenance.
        if e.get("source").is_none_or(|s| s == "") {
            e["source"] = json!("operator-supplied-1sec-export");
        }
    }
    let mut alerts = source_alerts
        .alerts
        .iter()
        .map(|v| select(v, ALERT_FIELDS))
        .collect::<Result<Vec<_>, _>>()?;
    let selected = alerts
        .iter_mut()
        .find(|a| a.get("id").and_then(Value::as_str) == Some(&map.selected_alert_id))
        .ok_or("selected_alert_id must name an included alert")?;
    selected["metadata"] =
        json!({"asset_id":map.asset.id,"asset_mapping":"operator_assertion_not_verified"});
    let value = json!({
        "format":bundle::FORMAT,"provenance":"user_supplied_export","sanitized":true,
        "incident_id":map.incident_id,"correlated_alert_id":map.selected_alert_id,
        "source_note":format!("Local 1-SEC export handoff. Source and asset mapping are operator assertions. Opaque details, metadata, raw_data and sensor fields were omitted; retained text still requires redaction. {}",map.source_note),
        "asset":map.asset,"ingestion":[],"events":events,"alerts":alerts,"context":map.context
    });
    let text = canonical(&value) + "\n";
    if text.len() > bundle::MAX_BYTES as usize {
        return Err("mapped incident exceeds 64 KiB".into());
    }
    let bundle = bundle::parse_bundle(&text)?;
    let inputs = crate::Inputs {
        plan: bundle::Simulation::initial(&bundle),
        bundle,
        bundle_sha: wraithkit::sha256_hex(text.as_bytes()),
        plan_sha: "absent".into(),
    };
    // Validate the exact labeled/pretty-printed payloads used by the app, not
    // compact approximations that miss wrappers or the asset context.
    for tool in [
        "read_ingestion",
        "read_incident",
        "read_context",
        "read_activity",
        "read_response_plan",
    ] {
        crate::evidence_text(&inputs, tool)?;
    }
    Ok(text)
}

pub fn import(source: &Path, output: &Path) -> Result<String, String> {
    let source = storage::root(source)?;
    let read = |name| storage::read(&source, name, false)?.ok_or_else(|| format!("missing {name}"));
    let text = convert(
        &read("events.json")?,
        &read("alerts.json")?,
        &read("mapping.json")?,
    )?;
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let parent = parent
        .canonicalize()
        .map_err(|_| "output parent must already exist")?;
    let name = output
        .file_name()
        .ok_or("output must name a new directory")?;
    let destination = parent.join(name);
    let builder = fs::DirBuilder::new();
    #[cfg(unix)]
    let builder = {
        use std::os::unix::fs::DirBuilderExt;
        let mut builder = builder;
        builder.mode(0o700);
        builder
    };
    builder
        .create(&destination)
        .map_err(|_| "output must be a new directory; existing paths are never overwritten")?;
    if destination
        .canonicalize()
        .map_err(|_| "cannot resolve new output")?
        != destination
    {
        return Err("new output was retargeted; refusing write".into());
    }
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(destination.join("incident.json"))
        .map_err(|_| "cannot exclusively create incident.json")?;
    file.write_all(text.as_bytes())
        .and_then(|_| file.sync_all())
        .map_err(|_| "could not finish incident.json; inspect the new output directory")?;
    Ok("Imported local export to a new incident.json. No model, network, response plan or enforcement was run. Retained strings are not automatically redacted; inspect before optional --live use.".into())
}
