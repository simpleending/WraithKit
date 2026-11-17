//! Local UI server. It binds 127.0.0.1 only, requires a random token as the first path segment,
//! checks the Host header, and accepts POST only as application/json. One loop serves requests
//! and advances the run, so no lock is shared between threads.

use crate::{
    App,
    ledger::{RunStatus, read_blob},
    now_ms,
    session::Session,
    verify::{all_ok, verify},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{io::Read, time::Duration};
use tiny_http::{Header, Method, Request, Response, Server};

const INDEX: &str = include_str!("../ui/index.html");
const MAX_BODY: u64 = 64 * 1024;
const CSP: &str =
    "default-src 'self'; style-src 'self' 'unsafe-inline'; script-src 'self' 'unsafe-inline'";

pub fn host_ok(host: Option<&str>, port: u16) -> bool {
    host.is_some_and(|h| h == format!("127.0.0.1:{port}") || h == format!("localhost:{port}"))
}

/// Returns the path after the token, or None when the token does not match.
pub fn strip_token<'a>(url: &'a str, token: &str) -> Option<&'a str> {
    let rest = url.strip_prefix('/')?.strip_prefix(token)?;
    (rest.is_empty() || rest.starts_with('/')).then_some(rest)
}

/// A per-run URL token, 32 hex characters.
pub fn new_token() -> String {
    use std::hash::{BuildHasher, Hasher};
    // shortcut: RandomState is OS-seeded but not a documented CSPRNG. That is fine for a per-run token on
    // loopback. Upgrade to a getrandom-based token if the server ever binds beyond 127.0.0.1.
    let h = |n: u64| {
        let mut x = std::collections::hash_map::RandomState::new().build_hasher();
        x.write_u64(n);
        x.write_u64(now_ms());
        x.finish()
    };
    format!("{:016x}{:016x}", h(1), h(2))
}

/// Binds 127.0.0.1. Port 0 asks the OS for a free port. Returns the port actually bound.
pub fn bind(port: u16) -> Result<(Server, u16), String> {
    let server = Server::http(("127.0.0.1", port))
        .map_err(|e| format!("could not listen on 127.0.0.1:{port}: {e}"))?;
    let actual = server.server_addr().to_ip().map_or(port, |a| a.port());
    Ok((server, actual))
}

/// Serves the UI and advances the run until the process stops. Returns only on a ledger or server error.
pub fn run<A: App>(
    server: Server,
    port: u16,
    token: &str,
    mut session: Session<A>,
) -> Result<(), String> {
    loop {
        if session.state.status == Some(RunStatus::Running) {
            // shortcut: a live model call blocks this loop for up to MODEL_TIMEOUT_SECS (90 s), so the page and the Approve button stall meanwhile. Fine for one local user; upgrade by calling the model on a worker thread and folding its reply in here.
            session.step()?;
        }
        match server.recv_timeout(Duration::from_millis(10)) {
            Ok(Some(req)) => handle(&mut session, port, token, req)?,
            Ok(None) => {}
            Err(e) => return Err(format!("server error: {e}")),
        }
    }
}

fn header(name: &str, value: &str) -> Option<Header> {
    Header::from_bytes(name.as_bytes(), value.as_bytes()).ok()
}

fn send(req: Request, code: u16, ctype: &str, body: String) {
    let mut resp = Response::from_string(body).with_status_code(code);
    let headers = [
        header("Content-Type", ctype),
        header("Content-Security-Policy", CSP),
        header("X-Content-Type-Options", "nosniff"),
        header("Cache-Control", "no-store"),
    ];
    for h in headers.into_iter().flatten() {
        resp = resp.with_header(h);
    }
    // A client that hung up is not a run error. The run continues.
    let _ = req.respond(resp);
}

fn send_json(req: Request, code: u16, v: Value) {
    send(req, code, "application/json; charset=utf-8", v.to_string());
}

fn is_json(req: &Request) -> bool {
    req.headers()
        .iter()
        .find(|h| h.field.equiv("Content-Type"))
        .is_some_and(|h| {
            h.value
                .as_str()
                .split(';')
                .next()
                .is_some_and(|t| t.trim() == "application/json")
        })
}

/// Reads a JSON body up to MAX_BODY. Returns None when it is too large or unreadable.
fn read_body(req: &mut Request) -> Option<String> {
    let mut body = String::new();
    req.as_reader()
        .take(MAX_BODY + 1)
        .read_to_string(&mut body)
        .ok()?;
    (body.len() as u64 <= MAX_BODY).then_some(body)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DecideBody {
    proposal: String,
    approve: bool,
    digest: String,
}

fn state_json<A: App>(s: &Session<A>) -> Value {
    let mut v = serde_json::to_value(&s.state).unwrap_or(Value::Null);
    v["head"] = json!(s.ledger.head());
    v
}

fn handle<A: App>(
    s: &mut Session<A>,
    port: u16,
    token: &str,
    mut req: Request,
) -> Result<(), String> {
    let host = req
        .headers()
        .iter()
        .find(|h| h.field.equiv("Host"))
        .map(|h| h.value.as_str().to_string());
    if !host_ok(host.as_deref(), port) {
        send(
            req,
            421,
            "text/plain; charset=utf-8",
            "misdirected request".into(),
        );
        return Ok(());
    }
    let url = req.url().to_string();
    let path = url.split('?').next().unwrap_or_default().to_string();
    let Some(rest) = strip_token(&path, token) else {
        send(req, 404, "text/plain; charset=utf-8", "not found".into());
        return Ok(());
    };
    let rest = if rest.is_empty() {
        "/".to_string()
    } else {
        rest.to_string()
    };
    let is_get = matches!(req.method(), Method::Get);
    let is_post = matches!(req.method(), Method::Post);
    match (is_get, is_post, rest.as_str()) {
        (true, _, "/") => send(req, 200, "text/html; charset=utf-8", INDEX.into()),
        (true, _, "/api/state") => send_json(req, 200, state_json(s)),
        (true, _, p) if p.starts_with("/api/evidence/") => {
            let id = &p["/api/evidence/".len()..];
            let blob = s
                .state
                .evidence
                .iter()
                .find(|e| e.id == id)
                .map(|e| e.sha256.clone());
            match blob.map(|sha| read_blob(s.run_dir(), &sha)) {
                Some(Ok(text)) => send(req, 200, "text/plain; charset=utf-8", text),
                _ => send(
                    req,
                    404,
                    "text/plain; charset=utf-8",
                    "no such evidence".into(),
                ),
            }
        }
        (_, true, "/api/decide") | (_, true, "/api/verify") if !is_json(&req) => {
            send(
                req,
                415,
                "text/plain; charset=utf-8",
                "POST needs Content-Type: application/json".into(),
            );
        }
        (_, true, "/api/decide") => {
            let Some(body) = read_body(&mut req) else {
                send(
                    req,
                    413,
                    "text/plain; charset=utf-8",
                    "request body too large".into(),
                );
                return Ok(());
            };
            let Ok(b) = serde_json::from_str::<DecideBody>(&body) else {
                send_json(
                    req,
                    400,
                    json!({"error": "expected {proposal, approve, digest}"}),
                );
                return Ok(());
            };
            match s.decide(&b.proposal, b.approve, &b.digest) {
                Ok(outcome) => send_json(req, 200, json!({"outcome": outcome})),
                Err(e) if e.starts_with("ledger write failed") => return Err(e),
                Err(e) => send_json(req, 409, json!({"error": e})),
            }
        }
        (_, true, "/api/verify") => {
            let (checks, head) = match verify(s.run_dir()) {
                Ok(r) => r,
                Err(e) => {
                    send_json(req, 500, json!({"error": e}));
                    return Ok(());
                }
            };
            let list: Vec<Value> = checks
                .iter()
                .map(|c| json!({"name": c.name, "ok": c.ok, "detail": c.detail}))
                .collect();
            send_json(
                req,
                200,
                json!({"checks": list, "all_ok": all_ok(&checks), "head": head}),
            );
        }
        (_, _, "/api/state") | (_, _, "/") => send(
            req,
            405,
            "text/plain; charset=utf-8",
            "method not allowed".into(),
        ),
        _ => send(req, 404, "text/plain; charset=utf-8", "not found".into()),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::{Counter, asst, call, start, text};
    use crate::testutil::TempDir;
    use std::io::Write as _;
    use std::net::TcpStream;

    fn raw(port: u16, request: &str) -> (u16, String) {
        let mut s = TcpStream::connect(("127.0.0.1", port)).expect("connect");
        s.write_all(request.as_bytes()).unwrap();
        let mut out = String::new();
        s.read_to_string(&mut out).unwrap();
        let code = out
            .split_whitespace()
            .nth(1)
            .and_then(|c| c.parse().ok())
            .unwrap_or(0);
        (code, out)
    }

    /// Starts the server on port 0 with a session that is waiting on one bump proposal.
    fn serve_waiting(dir: &TempDir) -> (u16, String, String) {
        let mut s = start(
            dir.path(),
            vec![
                asst(vec![call("c1", "bump", serde_json::json!({"n": 1}))]),
                text("ok"),
            ],
        );
        s.step().unwrap();
        let digest = s.state.proposals[0].digest.clone();
        let (server, port) = bind(0).unwrap();
        let token = new_token();
        let t = token.clone();
        std::thread::spawn(move || {
            let _ = run::<Counter>(server, port, &t, s);
        });
        (port, token, digest)
    }

    #[test]
    fn server_guards() {
        let d = TempDir::new("srv-guards");
        let (port, token, _) = serve_waiting(&d);
        let host = format!("127.0.0.1:{port}");
        let get = |path: &str, h: &str| {
            raw(
                port,
                &format!("GET {path} HTTP/1.1\r\nHost: {h}\r\nConnection: close\r\n\r\n"),
            )
        };
        assert_eq!(
            get(&format!("/{token}/api/state"), "evil.example").0,
            421,
            "wrong Host"
        );
        assert_eq!(
            get(&format!("/{token}/api/state"), &host).0,
            200,
            "good token and Host"
        );
        assert_eq!(get("/api/state", &host).0, 404, "missing token");
        assert_eq!(get("/wrong-token/api/state", &host).0, 404, "wrong token");
        assert!(host_ok(Some("localhost:5"), 5));
        assert!(!host_ok(Some("127.0.0.1:6"), 5));
        assert!(!host_ok(None, 5));
        assert_eq!(strip_token("/abc/x", "abc"), Some("/x"));
        assert_eq!(
            strip_token("/abcd/x", "abc"),
            None,
            "token must be a whole segment"
        );
    }

    #[test]
    fn post_requires_json() {
        let d = TempDir::new("srv-post");
        let (port, token, digest) = serve_waiting(&d);
        let host = format!("127.0.0.1:{port}");
        let body =
            serde_json::json!({"proposal": "P1", "approve": true, "digest": digest}).to_string();
        let post = |ctype: &str, body: &str| {
            raw(
                port,
                &format!(
                    "POST /{token}/api/decide HTTP/1.1\r\nHost: {host}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                ),
            )
        };
        assert_eq!(post("text/plain", &body).0, 415, "text/plain is refused");
        assert_eq!(
            post(
                "application/json",
                &serde_json::json!({"proposal": "P1", "approve": true, "digest": "bad"})
                    .to_string()
            )
            .0,
            409,
            "wrong digest"
        );
        assert_eq!(post("application/json", &body).0, 200, "good decide");
        assert_eq!(post("application/json", &body).0, 409, "repeated decide");
    }

    #[test]
    fn state_view_has_card_fields() {
        let d = TempDir::new("srv-card");
        let s = start(
            d.path(),
            vec![asst(vec![call("c1", "bump", serde_json::json!({"n": 1}))])],
        );
        let mut s = s;
        s.step().unwrap();
        let v = state_json(&s);
        let p = &v["proposals"][0];
        for field in [
            "summary",
            "detail",
            "warnings",
            "inputs",
            "influenced_by",
            "digest",
        ] {
            assert!(!p[field].is_null(), "card has {field}");
        }
        assert_eq!(p["digest"].as_str().unwrap().len(), 64);
        assert!(v["head"].as_str().unwrap().len() == 64);
    }

    #[test]
    fn verifier_head_can_wrap_at_mobile_widths() {
        // The full 64-character head is plain text, not inside a wrapping code tag.
        assert!(INDEX.contains("id=\"verify-head\""));
        assert!(INDEX.contains("#verify-head { overflow-wrap: anywhere; }"));
    }

    #[test]
    fn state_view_has_mode() {
        let d = TempDir::new("srv-mode");
        let s = start(
            d.path(),
            vec![
                asst(vec![call("c1", "peek", serde_json::json!({}))]),
                text("ok"),
            ],
        );
        let v = state_json(&s);
        assert!(
            v["model"].as_str().unwrap().starts_with("FIXTURE"),
            "mode label is in the view"
        );
        assert_eq!(v["mode"], "fixture");
    }
}
