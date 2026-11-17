//! Model adapter: a scripted fixture, or one OpenAI-compatible chat-completions endpoint.

use crate::MODEL_TIMEOUT_SECS;
use serde_json::{Value, json};
use std::time::Duration;

pub const FIXTURE_LABEL: &str = "FIXTURE: scripted, not a live model";

pub enum Model {
    Fixture {
        script: Vec<Value>,
    },
    Live {
        agent: ureq::Agent,
        base: String,
        key: Option<String>,
        model: String,
    },
}

impl Model {
    pub fn fixture(script_json: &str) -> Result<Self, String> {
        let script: Vec<Value> = serde_json::from_str(script_json)
            .map_err(|e| format!("fixture script is not a JSON array: {e}"))?;
        Ok(Model::Fixture { script })
    }

    /// Reads `WRAITH_BASE_URL`, `WRAITH_MODEL` and `WRAITH_API_KEY`. The key is never printed or stored.
    pub fn live_from_env() -> Result<Self, String> {
        let base =
            std::env::var("WRAITH_BASE_URL").unwrap_or_else(|_| "https://api.openai.com/v1".into());
        let base = base.trim_end_matches('/').to_string();
        let model = std::env::var("WRAITH_MODEL").map_err(|_| {
            "set WRAITH_MODEL (for example gpt-5-mini, or llama3.2 with Ollama)".to_string()
        })?;
        let key = std::env::var("WRAITH_API_KEY")
            .ok()
            .filter(|k| !k.is_empty());
        check_base_url(&base)?;
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(MODEL_TIMEOUT_SECS)))
            .http_status_as_error(false)
            .max_redirects(0)
            .build()
            .into();
        Ok(Model::Live {
            agent,
            base,
            key,
            model,
        })
    }

    /// Shown in the UI and written to `RunStarted`. Never includes the key.
    pub fn label(&self) -> String {
        match self {
            Model::Fixture { .. } => FIXTURE_LABEL.into(),
            Model::Live { base, model, .. } => format!("LIVE: {model} at {base}"),
        }
    }

    /// Written to `RunStarted` before the first request, so the egress is on record.
    pub fn disclosure(&self) -> String {
        match self {
            Model::Fixture { .. } => String::new(),
            Model::Live { base, .. } => {
                format!("LIVE: task text and tool results will be sent to {base}")
            }
        }
    }

    /// Returns the assistant message as the server sent it, and its usage when present.
    /// `turn` is the number of model turns so far. The fixture uses it as the script index.
    pub fn next(
        &mut self,
        turn: usize,
        messages: &[Value],
        tools: &[Value],
    ) -> Result<(Value, Option<Value>), String> {
        match self {
            Model::Fixture { script } => script
                .get(turn)
                .cloned()
                .map(|m| (m, None))
                .ok_or_else(|| format!("fixture script has no turn {turn}")),
            Model::Live {
                agent,
                base,
                key,
                model,
            } => {
                let body = request_body(model, messages, tools);
                let mut req = agent.post(format!("{base}/chat/completions"));
                if let Some(k) = key.as_deref() {
                    req = req.header("Authorization", format!("Bearer {k}"));
                }
                let mut resp = req.send_json(&body).map_err(|e| {
                    let message = format!("model request failed: {e}");
                    if key
                        .as_deref()
                        .is_some_and(|k| !k.is_empty() && message.contains(k))
                    {
                        "model request failed (credential omitted)".into()
                    } else {
                        message
                    }
                })?;
                let status = resp.status().as_u16();
                let v: Value = resp
                    .body_mut()
                    .read_json()
                    .map_err(|e| format!("model response is not JSON (HTTP {status}): {e}"))?;
                // Providers can echo credentials in errors or even successful messages/usage.
                // Refuse the whole response before any part reaches the ledger, UI or tools.
                if key
                    .as_deref()
                    .is_some_and(|k| !k.is_empty() && contains_secret(&v, k))
                {
                    return Err(
                        "model response contained WRAITH_API_KEY; it was not recorded".into(),
                    );
                }
                if status != 200 {
                    let msg = v["error"]["message"].as_str().unwrap_or("no message");
                    return Err(format!("model returned HTTP {status}: {msg}"));
                }
                let msg = v["choices"][0]["message"].clone();
                if !msg.is_object() {
                    return Err("model response has no choices[0].message".into());
                }
                Ok((msg, v.get("usage").cloned()))
            }
        }
    }
}

fn contains_secret(value: &Value, key: &str) -> bool {
    match value {
        Value::String(s) => s.contains(key),
        Value::Array(values) => values.iter().any(|v| contains_secret(v, key)),
        Value::Object(values) => values
            .iter()
            .any(|(name, v)| name.contains(key) || contains_secret(v, key)),
        _ => false,
    }
}

pub fn request_body(model: &str, messages: &[Value], tools: &[Value]) -> Value {
    json!({"model": model, "messages": messages, "tools": tools, "tool_choice": "auto"})
}

/// HTTPS is required, except for a model server on this machine.
pub fn check_base_url(base: &str) -> Result<(), String> {
    let uri: ureq::http::Uri = base
        .parse()
        .map_err(|_| "WRAITH_BASE_URL is not a valid URL".to_string())?;
    // The base URL is printed, shown in the UI and written to the ledger, so it must not carry a key.
    if uri.authority().is_some_and(|a| a.as_str().contains('@')) {
        return Err("WRAITH_BASE_URL must not contain a user name or password; put the key in WRAITH_API_KEY".into());
    }
    if uri.query().is_some() {
        return Err(
            "WRAITH_BASE_URL must not contain a query; put credentials in WRAITH_API_KEY".into(),
        );
    }
    let local = matches!(uri.host(), Some("localhost" | "127.0.0.1"));
    match uri.scheme_str() {
        Some("https") => Ok(()),
        Some("http") if local => Ok(()),
        _ => Err(
            "WRAITH_BASE_URL must be https (http is allowed only for localhost or 127.0.0.1)"
                .into(),
        ),
    }
}

pub struct Call {
    pub id: String,
    pub name: String,
    pub args: Result<Value, String>,
}

/// Parses the OpenAI `tool_calls` of an assistant message. Bad arguments stay as `Err` for the caller to report.
pub fn tool_calls(message: &Value) -> Vec<Call> {
    message["tool_calls"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|c| Call {
            id: c["id"].as_str().unwrap_or_default().to_owned(),
            name: c["function"]["name"]
                .as_str()
                .unwrap_or_default()
                .to_owned(),
            args: parse_arguments(&c["function"]["arguments"]),
        })
        .collect()
}

/// OpenAI sends arguments as a JSON string. Some compatible servers send `""` for a tool with no
/// parameters, or an object instead of a string; both are accepted rather than failing every call.
fn parse_arguments(v: &Value) -> Result<Value, String> {
    match v {
        Value::String(s) if s.trim().is_empty() => Ok(json!({})),
        Value::String(s) => {
            serde_json::from_str(s).map_err(|e| format!("arguments are not valid JSON: {e}"))
        }
        Value::Object(_) => Ok(v.clone()),
        _ => Err("arguments missing".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_openai_tool_calls() {
        let msg = json!({"role": "assistant", "tool_calls": [
            {"id": "c1", "type": "function", "function": {"name": "read_file", "arguments": "{\"path\":\"a\"}"}},
            {"id": "c2", "type": "function", "function": {"name": "search", "arguments": "{bad"}}
        ]});
        let calls = tool_calls(&msg);
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].name, "read_file");
        assert_eq!(calls[0].args.as_ref().unwrap(), &json!({"path": "a"}));
        assert!(
            calls[1]
                .args
                .as_ref()
                .err()
                .unwrap()
                .contains("not valid JSON")
        );
    }

    #[test]
    fn tolerates_compatible_server_argument_shapes() {
        let msg = json!({"role": "assistant", "tool_calls": [
            {"id": "a", "type": "function", "function": {"name": "list_files", "arguments": ""}},
            {"id": "b", "type": "function", "function": {"name": "read_file", "arguments": {"path": "x"}}},
            {"id": "c", "type": "function", "function": {"name": "read_file"}},
            {"id": "d", "type": "function", "function": {"name": "read_file", "arguments": "[1]"}}
        ]});
        let calls = tool_calls(&msg);
        assert_eq!(
            calls[0].args.as_ref().unwrap(),
            &json!({}),
            "empty string means no arguments"
        );
        assert_eq!(
            calls[1].args.as_ref().unwrap(),
            &json!({"path": "x"}),
            "an object is accepted"
        );
        assert!(calls[2].args.is_err(), "missing arguments are an error");
        assert!(
            calls[3].args.is_ok(),
            "valid JSON parses; the tool's own schema then rejects it"
        );
    }

    #[test]
    fn live_request_shape() {
        let tools = vec![json!({"type": "function"})];
        let body = request_body("m1", &[json!({"role": "user", "content": "hi"})], &tools);
        assert_eq!(body["model"], "m1");
        assert_eq!(body["messages"][0]["content"], "hi");
        assert_eq!(body["tools"], json!(tools));
        assert_eq!(body["tool_choice"], "auto");
    }

    #[test]
    fn model_base_url_rules() {
        assert!(check_base_url("https://api.openai.com/v1").is_ok());
        assert!(check_base_url("http://localhost:11434/v1").is_ok());
        assert!(check_base_url("http://127.0.0.1:8000/v1").is_ok());
        assert!(check_base_url("http://example.com/v1").is_err());
        assert!(check_base_url("not a url at all").is_err());
        assert!(check_base_url("ftp://example.com").is_err());
        let err = check_base_url("https://user:secret@proxy.example/v1").unwrap_err();
        assert!(err.contains("WRAITH_API_KEY"), "{err}");
        assert!(
            !err.contains("secret"),
            "the error must not echo the credential"
        );
        assert!(
            check_base_url("https://proxy.example:8443/v1").is_ok(),
            "a port is fine"
        );
    }

    #[test]
    fn provider_echoed_key_is_not_recorded() {
        use crate::{
            Mode, ledger::RunStatus, session::Session, testkit::Counter, testutil::TempDir,
        };
        let key = "synthetic-secret-for-provider-echo-test";
        for (status, body) in [
            (
                401,
                json!({"error": {"message": format!("invalid key: {key}")}}),
            ),
            (
                200,
                json!({"choices": [{"message": {"role": "assistant", "content": key}}]}),
            ),
            (
                200,
                json!({"choices": [{"message": {"role": "assistant", "content": "done"}}], "usage": {"echo": key}}),
            ),
        ] {
            let server = tiny_http::Server::http(("127.0.0.1", 0)).unwrap();
            let port = server.server_addr().to_ip().unwrap().port();
            let worker = std::thread::spawn(move || {
                let req = server
                    .recv_timeout(Duration::from_secs(5))
                    .unwrap()
                    .expect("model request");
                req.respond(
                    tiny_http::Response::from_string(body.to_string()).with_status_code(status),
                )
                .unwrap();
            });
            let dir = TempDir::new("provider-echo");
            let model = Model::Live {
                agent: ureq::Agent::config_builder()
                    .http_status_as_error(false)
                    .build()
                    .into(),
                base: format!("http://127.0.0.1:{port}/v1"),
                key: Some(key.into()),
                model: "test".into(),
            };
            let mut session =
                Session::<Counter>::start(dir.path(), dir.path(), "test", Mode::Live, model)
                    .unwrap();
            session.step().unwrap();
            worker.join().unwrap();
            let ledger = std::fs::read_to_string(session.run_dir().join("ledger.jsonl")).unwrap();
            assert!(
                !ledger.contains(key),
                "an echoed credential must not reach the ledger"
            );
            assert!(
                !serde_json::to_string(&session.state).unwrap().contains(key),
                "the UI must not receive the key"
            );
            assert_eq!(session.state.status, Some(RunStatus::Failed));
            let (checks, _) = crate::verify::verify_with(session.run_dir(), Some(key)).unwrap();
            assert!(crate::verify::all_ok(&checks));
        }
    }

    #[test]
    fn model_base_url_does_not_accept_query_credentials() {
        let err = check_base_url("https://proxy.example/v1?api_key=synthetic-secret").unwrap_err();
        assert!(!err.contains("synthetic-secret"));
        assert!(check_base_url("https://proxy.example/v1").is_ok());
    }

    #[test]
    fn fixture_script_end_is_error() {
        let mut m = Model::fixture(r#"[{"role":"assistant","content":"done"}]"#).unwrap();
        assert!(m.next(0, &[], &[]).is_ok());
        let err = m.next(1, &[], &[]).err().unwrap();
        assert!(err.contains("no turn 1"));
        assert_eq!(m.label(), FIXTURE_LABEL);
        assert_eq!(m.disclosure(), "");
    }
}
