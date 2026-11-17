//! Offline verifier. It reads `ledger.jsonl` and the blobs directly and takes no lock, so it also works on a run that is open.

use crate::{
    ledger::{ClaimStatus, Event, Line, line_hash, read_blob},
    session::{proposal_digest, quote_found},
    sha256_hex,
};
use std::{collections::HashMap, fs, path::Path};

pub struct Check {
    pub name: &'static str,
    pub ok: bool,
    pub detail: String,
}

fn check(name: &'static str, failure: Option<String>, ok_detail: String) -> Check {
    match failure {
        None => Check {
            name,
            ok: true,
            detail: ok_detail,
        },
        Some(detail) => Check {
            name,
            ok: false,
            detail,
        },
    }
}

pub fn all_ok(checks: &[Check]) -> bool {
    checks.iter().all(|c| c.ok)
}

/// Runs every check. Returns the checks and the head hash. The head hash is what to note down.
/// The API key check reads `WRAITH_API_KEY` from the environment.
pub fn verify(run_dir: &Path) -> Result<(Vec<Check>, String), String> {
    let key = std::env::var("WRAITH_API_KEY").ok();
    verify_with(run_dir, key.as_deref())
}

/// `verify`, with the key passed in explicitly. Tests use this to avoid changing the process environment.
pub fn verify_with(run_dir: &Path, api_key: Option<&str>) -> Result<(Vec<Check>, String), String> {
    let text = fs::read_to_string(run_dir.join("ledger.jsonl"))
        .map_err(|e| format!("cannot read ledger.jsonl in {}: {e}", run_dir.display()))?;

    // Parse complete lines. Stop at the first line that does not parse, and report it in the chain check.
    // A final line with no newline is a write cut short by a crash, or one still being written by an
    // open session. `Ledger::open` drops it the same way, so it is noted, not failed.
    let mut lines: Vec<Line> = Vec::new();
    let mut chain_err = None;
    let mut torn = false;
    for (i, raw) in text.split_inclusive('\n').enumerate() {
        if !raw.ends_with('\n') {
            torn = true;
            break;
        }
        match serde_json::from_str::<Line>(raw) {
            Ok(l) => lines.push(l),
            Err(e) => {
                chain_err = Some(format!("seq {i} does not parse: {e}"));
                break;
            }
        }
    }
    if chain_err.is_none() {
        for (i, l) in lines.iter().enumerate() {
            let prev = if i == 0 {
                ""
            } else {
                lines[i - 1].hash.as_str()
            };
            let bad = if l.seq != i as u64 {
                Some(format!("seq {i} is numbered {}", l.seq))
            } else if l.prev != prev {
                Some(format!(
                    "seq {i}: prev does not link to seq {}",
                    i.saturating_sub(1)
                ))
            } else if l.hash != line_hash(l.seq, &l.prev, l.at_ms, &l.event) {
                Some(format!("seq {i}: hash does not match its content"))
            } else {
                None
            };
            if bad.is_some() {
                chain_err = bad;
                break;
            }
        }
    }
    let torn_note = if torn {
        " (+1 torn final line ignored, as resume would drop it)"
    } else {
        ""
    };
    let mut checks = vec![check(
        "chain",
        chain_err,
        format!("{} lines{torn_note}", lines.len()),
    )];

    // Evidence: id -> (blob hash, seq).
    let mut evidence: HashMap<String, (String, u64)> = HashMap::new();
    let mut blob_count = 0;
    let mut blob_err = None;
    for l in &lines {
        if let Event::Evidence { id, sha256, .. } = &l.event {
            evidence.insert(id.clone(), (sha256.clone(), l.seq));
            blob_count += 1;
            let ok = read_blob(run_dir, sha256).is_ok_and(|t| sha256_hex(t.as_bytes()) == *sha256);
            if !ok && blob_err.is_none() {
                blob_err = Some(format!(
                    "blob {sha256} is missing or its content does not match its name"
                ));
            }
        }
    }
    checks.push(check("blobs", blob_err, format!("{blob_count} blobs")));

    // Cites: each stored `found` value must recompute, and a missing quote forces `unsupported`.
    let mut cite_count = 0;
    let mut quote_err = None;
    for l in &lines {
        let Event::Claim { status, cites, .. } = &l.event else {
            continue;
        };
        if cites.is_empty() && *status != ClaimStatus::Unsupported && quote_err.is_none() {
            quote_err = Some(format!(
                "seq {}: claim has no cites but is not unsupported",
                l.seq
            ));
        }
        for c in cites {
            cite_count += 1;
            // Only evidence recorded before the claim counts, as it did when the session checked the quote.
            let now = evidence
                .get(&c.evidence_id)
                .filter(|(_, seq)| *seq < l.seq)
                .is_some_and(|(sha, _)| {
                    read_blob(run_dir, sha).is_ok_and(|t| quote_found(&t, &c.quote))
                });
            if now != c.found && quote_err.is_none() {
                quote_err = Some(format!(
                    "seq {}: cite {} recorded found={} but recomputes to {now}",
                    l.seq, c.evidence_id, c.found
                ));
            }
            if !c.found && *status != ClaimStatus::Unsupported && quote_err.is_none() {
                quote_err = Some(format!(
                    "seq {}: a quote is missing but the claim is not unsupported",
                    l.seq
                ));
            }
        }
    }
    checks.push(check("quotes", quote_err, format!("{cite_count} cites")));

    // Proposals: the digest must recompute, and every influencing evidence ID must exist before the proposal.
    let mut proposals: HashMap<String, (String, u64)> = HashMap::new();
    let mut digest_err = None;
    let mut proposal_count = 0;
    for l in &lines {
        let Event::Proposed {
            id,
            tool,
            args,
            inputs,
            influenced_by,
            digest,
            ..
        } = &l.event
        else {
            continue;
        };
        proposal_count += 1;
        proposals.insert(id.clone(), (digest.clone(), l.seq));
        if *digest != proposal_digest(tool, args, inputs, influenced_by) && digest_err.is_none() {
            digest_err = Some(format!("seq {}: {id} digest does not recompute", l.seq));
        }
        for e in influenced_by {
            let before = evidence.get(e).is_some_and(|(_, seq)| *seq < l.seq);
            if !before && digest_err.is_none() {
                digest_err = Some(format!(
                    "seq {}: {id} cites evidence {e} that was not recorded before it",
                    l.seq
                ));
            }
        }
    }
    checks.push(check(
        "digests",
        digest_err,
        format!("{proposal_count} proposals"),
    ));

    // Authorization: an effect needs an earlier approval for the same proposal and the same digest.
    let mut auth_err = None;
    let mut effects: HashMap<String, usize> = HashMap::new();
    let mut effect_count = 0;
    for l in &lines {
        let Event::EffectStarted { proposal, digest } = &l.event else {
            continue;
        };
        effect_count += 1;
        *effects.entry(proposal.clone()).or_default() += 1;
        let Some((proposed_digest, _)) = proposals.get(proposal) else {
            auth_err.get_or_insert(format!(
                "seq {}: effect for unknown proposal {proposal}",
                l.seq
            ));
            continue;
        };
        let approved = lines.iter().any(|d| {
            d.seq < l.seq
                && matches!(&d.event, Event::Decided { proposal: p, approve: true, digest: dg, .. }
                    if p == proposal && dg == digest && digest == proposed_digest)
        });
        if !approved {
            auth_err.get_or_insert(format!(
                "seq {}: {proposal} ran without an earlier approval of the same digest",
                l.seq
            ));
        }
    }
    checks.push(check(
        "authorized",
        auth_err,
        format!("{effect_count} effects"),
    ));

    let mut once_err = None;
    for (p, n) in &effects {
        if *n > 1 {
            once_err.get_or_insert(format!("{p} started {n} times"));
        }
    }
    checks.push(check(
        "once",
        once_err,
        "at most one start per proposal".into(),
    ));

    // Secrets: the key must not appear in the ledger or any blob. The key itself is never printed.
    let secret = match api_key.filter(|k| k.len() >= 8) {
        None => Check {
            name: "secrets",
            ok: true,
            detail: "WRAITH_API_KEY not set: skipped".into(),
        },
        Some(key) => {
            let mut found = if text.contains(key) {
                Some("ledger.jsonl".to_string())
            } else {
                None
            };
            if let Ok(entries) = fs::read_dir(run_dir.join("blobs")) {
                for e in entries.flatten() {
                    if found.is_none()
                        && fs::read(e.path())
                            .is_ok_and(|b| b.windows(key.len()).any(|w| w == key.as_bytes()))
                    {
                        found = Some(format!("blob {}", e.file_name().to_string_lossy()));
                    }
                }
            }
            check(
                "secrets",
                found.map(|w| format!("the API key appears in {w}")),
                "key not found in ledger or blobs".into(),
            )
        }
    };
    checks.push(secret);

    let head = lines.last().map_or_else(String::new, |l| l.hash.clone());
    Ok((checks, head))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Mode,
        ledger::Ledger,
        session::Session,
        testkit::{Counter, asst, call, fixture, run_to_end, text},
        testutil::TempDir,
    };

    /// A finished run: peek, bump (approved), finish.
    fn finished_run(dir: &Path) -> std::path::PathBuf {
        let model = fixture(vec![
            asst(vec![call("c1", "peek", serde_json::json!({}))]),
            asst(vec![call(
                "c2",
                "claim",
                serde_json::json!({"text": "hello", "status": "supported",
                "cites": [{"evidence_id": "E1", "quote": "hello world text"}]}),
            )]),
            asst(vec![call("c3", "bump", serde_json::json!({"n": 2}))]),
            text("all done"),
        ]);
        let mut s = Session::<Counter>::start(dir, dir, "task", Mode::Fixture, model).unwrap();
        run_to_end(&mut s);
        let digest = s.state.proposals[0].digest.clone();
        s.decide("P1", true, &digest).unwrap();
        run_to_end(&mut s);
        assert_eq!(s.state.status, Some(crate::ledger::RunStatus::Finished));
        s.run_dir().to_path_buf()
    }

    #[test]
    fn verify_clean_run_passes() {
        let d = TempDir::new("vclean");
        let run = finished_run(d.path());
        let (checks, head) = verify_with(&run, None).unwrap();
        for c in &checks {
            assert!(c.ok, "{}: {}", c.name, c.detail);
        }
        assert_eq!(checks.len(), 7);
        assert_eq!(head.len(), 64);
    }

    /// The first string value in an event, skipping the `kind` tag.
    fn first_string(v: &mut serde_json::Value) -> Option<&mut String> {
        match v {
            serde_json::Value::String(s) => Some(s),
            serde_json::Value::Array(a) => a.iter_mut().find_map(first_string),
            serde_json::Value::Object(o) => o
                .iter_mut()
                .filter(|(k, _)| k.as_str() != "kind")
                .find_map(|(_, v)| first_string(v)),
            _ => None,
        }
    }

    #[test]
    fn verify_detects_tamper() {
        let d = TempDir::new("vtamper");
        let run = finished_run(d.path());
        let original = fs::read_to_string(run.join("ledger.jsonl")).unwrap();
        let lines: Vec<&str> = original.lines().collect();
        for i in 0..lines.len() {
            // Change one character inside a string value of the event. The line still parses.
            let mut v: serde_json::Value = serde_json::from_str(lines[i]).unwrap();
            let s = first_string(&mut v["event"]).expect("every event has a string value");
            s.push('x');
            let mut edited: Vec<String> = lines.iter().map(|s| s.to_string()).collect();
            edited[i] = v.to_string();
            fs::write(run.join("ledger.jsonl"), edited.join("\n") + "\n").unwrap();
            let (checks, _) = verify_with(&run, None).unwrap();
            let chain = checks.iter().find(|c| c.name == "chain").unwrap();
            assert!(!chain.ok, "tamper at seq {i} must fail the chain");
            assert!(
                chain.detail.contains(&format!("seq {i}")),
                "names seq {i}: {}",
                chain.detail
            );
            fs::write(run.join("ledger.jsonl"), &original).unwrap();
        }
        // A corrupted blob fails the blob check and names the blob.
        let blob = fs::read_dir(run.join("blobs"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        fs::write(&blob, "tampered evidence").unwrap();
        let (checks, _) = verify_with(&run, None).unwrap();
        assert!(!checks.iter().find(|c| c.name == "blobs").unwrap().ok);
    }

    #[test]
    fn verify_flags_unauthorized_effect() {
        let d = TempDir::new("vauth");
        let mut l = Ledger::open(d.path()).unwrap();
        let args = serde_json::json!({"n": 1});
        let inputs = std::collections::BTreeMap::from([("counter".to_string(), "0".to_string())]);
        let digest = proposal_digest("bump", &args, &inputs, &[]);
        l.append(Event::Proposed {
            id: "P1".into(),
            call_id: "c".into(),
            tool: "bump".into(),
            args,
            summary: String::new(),
            detail: String::new(),
            warnings: vec![],
            inputs,
            influenced_by: vec![],
            digest: digest.clone(),
        })
        .unwrap();
        l.append(Event::EffectStarted {
            proposal: "P1".into(),
            digest,
        })
        .unwrap();
        drop(l);
        let (checks, _) = verify_with(d.path(), None).unwrap();
        let by = |n: &str| checks.iter().find(|c| c.name == n).unwrap().ok;
        assert!(
            by("chain"),
            "the chain is valid, so only authorization should fail"
        );
        assert!(!by("authorized"));
    }

    #[test]
    fn verify_accepts_cite_of_later_evidence() {
        // A model cites E1 before it exists, so the session records found=false. E1 arrives later with the quote.
        let d = TempDir::new("vlater");
        let mut l = Ledger::open(d.path()).unwrap();
        let quote = "hello world text";
        l.append(Event::Claim {
            text: "early".into(),
            status: ClaimStatus::Unsupported,
            cites: vec![crate::ledger::Cite {
                evidence_id: "E1".into(),
                quote: quote.into(),
                found: false,
            }],
        })
        .unwrap();
        let sha = l.put_blob(quote).unwrap();
        l.append(Event::Evidence {
            id: "E1".into(),
            source: "s".into(),
            sha256: sha,
            bytes: quote.len(),
            flags: vec![],
        })
        .unwrap();
        drop(l);
        let (checks, _) = verify_with(d.path(), None).unwrap();
        let quotes = checks.iter().find(|c| c.name == "quotes").unwrap();
        assert!(quotes.ok, "an honest run passes: {}", quotes.detail);
    }

    #[test]
    fn verify_while_open() {
        let d = TempDir::new("vopen");
        let model = fixture(vec![asst(vec![call(
            "c1",
            "bump",
            serde_json::json!({"n": 1}),
        )])]);
        let mut s =
            Session::<Counter>::start(d.path(), d.path(), "task", Mode::Fixture, model).unwrap();
        run_to_end(&mut s);
        // The session still holds the run's lock. verify must still read it.
        let (checks, _) = verify_with(s.run_dir(), None).expect("verify reads an open run");
        assert!(all_ok(&checks), "all checks pass while the run is open");
    }

    #[test]
    fn key_never_persisted() {
        let d = TempDir::new("vkey");
        let run = finished_run(d.path());
        let key = "sk-test-KEY-0123456789";
        let (checks, _) = verify_with(&run, Some(key)).unwrap();
        assert!(
            checks.iter().find(|c| c.name == "secrets").unwrap().ok,
            "a run that never saw the key passes"
        );
        // A planted key must be caught, so the check is not vacuous.
        let mut l = Ledger::open(&run).unwrap();
        l.append(Event::ToolResult {
            call_id: "x".into(),
            tool: "t".into(),
            ok: true,
            content: key.into(),
        })
        .unwrap();
        drop(l);
        let (checks, _) = verify_with(&run, Some(key)).unwrap();
        assert!(!checks.iter().find(|c| c.name == "secrets").unwrap().ok);
    }

    #[test]
    fn torn_final_line_is_noted_not_failed() {
        let d = TempDir::new("vtorn");
        let run = finished_run(d.path());
        let mut f = fs::OpenOptions::new()
            .append(true)
            .open(run.join("ledger.jsonl"))
            .unwrap();
        std::io::Write::write_all(&mut f, b"{\"seq\":99,\"pre").unwrap();
        drop(f);
        let (checks, _) = verify_with(&run, None).unwrap();
        let chain = checks.iter().find(|c| c.name == "chain").unwrap();
        assert!(
            chain.ok,
            "a torn tail is what a crash leaves: {}",
            chain.detail
        );
        assert!(
            chain.detail.contains("torn final line ignored"),
            "{}",
            chain.detail
        );
    }
}
