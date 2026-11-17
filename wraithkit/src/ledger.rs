//! Append-only, hash-chained JSONL ledger plus content-addressed evidence blobs.

use crate::{canonical, now_ms, sha256_hex};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{self, BufRead, BufReader, Seek, Write},
    path::{Path, PathBuf},
};

pub const LOCKED_MSG: &str = "another WraithKit process has this run open";

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Fixture,
    Live,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Running,
    Waiting,
    Finished,
    Failed,
    NeedsReview,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ClaimStatus {
    Supported,
    Conflicted,
    Unsupported,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Cite {
    pub evidence_id: String,
    pub quote: String,
    pub found: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Event {
    RunStarted {
        app: String,
        task: String,
        target: String,
        mode: Mode,
        model: String,
        disclosure: String,
    },
    ModelTurn {
        message: Value,
        usage: Option<Value>,
    },
    ToolResult {
        call_id: String,
        tool: String,
        ok: bool,
        content: String,
    },
    Evidence {
        id: String,
        source: String,
        sha256: String,
        bytes: usize,
        flags: Vec<String>,
    },
    Claim {
        text: String,
        status: ClaimStatus,
        cites: Vec<Cite>,
    },
    Proposed {
        id: String,
        call_id: String,
        tool: String,
        args: Value,
        summary: String,
        detail: String,
        warnings: Vec<String>,
        inputs: BTreeMap<String, String>,
        influenced_by: Vec<String>,
        digest: String,
    },
    Decided {
        proposal: String,
        approve: bool,
        digest: String,
        reason: String,
    },
    EffectStarted {
        proposal: String,
        digest: String,
    },
    EffectDone {
        proposal: String,
        ok: bool,
        receipt: String,
    },
    /// A probe the app refused for policy reasons. Apps signal this with an error text that starts with `blocked: `.
    Blocked {
        call_id: String,
        tool: String,
        reason: String,
    },
    RunFinished {
        status: RunStatus,
        summary: String,
    },
    /// A failed run (for example a model timeout) was reopened and continues. `after` is the failure it recovers from.
    RunResumed {
        after: String,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Line {
    pub seq: u64,
    pub prev: String,
    pub at_ms: u64,
    pub event: Event,
    pub hash: String,
}

pub fn line_hash(seq: u64, prev: &str, at_ms: u64, event: &Event) -> String {
    sha256_hex(
        canonical(&json!({"seq": seq, "prev": prev, "at_ms": at_ms, "event": event})).as_bytes(),
    )
}

pub struct Ledger {
    file: File,
    // Held only for its lock. The lock lives on its own file so `verify` can read ledger.jsonl
    // while a session is open. On Windows an exclusive lock on the ledger itself blocks readers.
    _lock: File,
    pub dir: PathBuf,
    pub lines: Vec<Line>,
}

impl Ledger {
    /// Opens (or creates) a run folder, takes its lock, and loads the chain. A torn final line is dropped.
    pub fn open(dir: &Path) -> io::Result<Self> {
        fs::create_dir_all(dir.join("blobs"))?;
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(dir.join("ledger.lock"))?;
        lock.try_lock().map_err(|e| match e {
            fs::TryLockError::WouldBlock => io::Error::other(LOCKED_MSG),
            fs::TryLockError::Error(e) => e,
        })?;
        // Not append mode: Windows refuses `set_len` on append handles.
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(dir.join("ledger.jsonl"))?;
        let (mut lines, mut good_len, mut buf) = (Vec::new(), 0u64, String::new());
        let mut reader = BufReader::new(&file);
        loop {
            buf.clear();
            let n = reader.read_line(&mut buf)?;
            if n == 0 || !buf.ends_with('\n') {
                break; // end of file, or a torn final write
            }
            let line: Line = serde_json::from_str(&buf).map_err(|e| {
                io::Error::other(format!("ledger line {} is corrupt: {e}", lines.len()))
            })?;
            lines.push(line);
            good_len += n as u64;
        }
        drop(reader);
        if file.metadata()?.len() != good_len {
            file.set_len(good_len)?;
        }
        file.seek(io::SeekFrom::End(0))?;
        Ok(Ledger {
            file,
            _lock: lock,
            dir: dir.to_path_buf(),
            lines,
        })
    }

    pub fn head(&self) -> &str {
        self.lines.last().map_or("", |l| l.hash.as_str())
    }

    /// Writes one line and syncs it to disk before returning.
    pub fn append(&mut self, event: Event) -> io::Result<()> {
        let seq = self.lines.len() as u64;
        let at_ms = now_ms();
        let prev = self.head().to_owned();
        let hash = line_hash(seq, &prev, at_ms, &event);
        let line = Line {
            seq,
            prev,
            at_ms,
            event,
            hash,
        };
        let mut s = serde_json::to_string(&line).map_err(io::Error::other)?;
        s.push('\n');
        self.file.write_all(s.as_bytes())?;
        self.file.sync_data()?;
        self.lines.push(line);
        Ok(())
    }

    /// Stores text under its SHA-256 and returns the hash.
    pub fn put_blob(&self, text: &str) -> io::Result<String> {
        let sha = sha256_hex(text.as_bytes());
        let p = self.dir.join("blobs").join(format!("{sha}.txt"));
        // An existing file is trusted only if its content still has this hash: a crash mid-write can leave a partial one.
        if fs::read(&p).is_ok_and(|b| sha256_hex(&b) == sha) {
            return Ok(sha);
        }
        // Write aside, then rename, so the final name never holds partial text.
        let tmp = p.with_extension("tmp");
        let mut f = File::create(&tmp)?;
        f.write_all(text.as_bytes())?;
        f.sync_data()?;
        drop(f);
        fs::rename(&tmp, &p)?;
        Ok(sha)
    }

    pub fn blob(&self, sha: &str) -> io::Result<String> {
        read_blob(&self.dir, sha)
    }
}

/// Reads `blobs/<sha>.txt`. Only a 64-char lowercase hex name is accepted, so the name cannot escape the folder.
pub fn read_blob(dir: &Path, sha: &str) -> io::Result<String> {
    let valid = sha.len() == 64
        && sha
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
    if !valid {
        return Err(io::Error::other("bad blob name"));
    }
    fs::read_to_string(dir.join("blobs").join(format!("{sha}.txt")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TempDir;

    fn note(i: usize) -> Event {
        Event::ToolResult {
            call_id: format!("c{i}"),
            tool: "t".into(),
            ok: true,
            content: format!("n{i}"),
        }
    }

    #[test]
    fn ledger_chain_links() {
        let d = TempDir::new("chain");
        let mut l = Ledger::open(d.path()).unwrap();
        for i in 0..3 {
            l.append(note(i)).unwrap();
        }
        assert_eq!(l.lines.len(), 3);
        assert_eq!(l.lines[0].prev, "");
        for (i, line) in l.lines.iter().enumerate() {
            assert_eq!(line.seq, i as u64);
            assert_eq!(
                line.hash,
                line_hash(line.seq, &line.prev, line.at_ms, &line.event)
            );
            if i > 0 {
                assert_eq!(line.prev, l.lines[i - 1].hash);
            }
        }
        drop(l);
        let again = Ledger::open(d.path()).unwrap();
        assert_eq!(
            again.lines.len(),
            3,
            "a reopened ledger reads back the same chain"
        );
        assert_eq!(again.head(), again.lines[2].hash);
    }

    #[test]
    fn ledger_survives_torn_tail() {
        let d = TempDir::new("torn");
        let mut l = Ledger::open(d.path()).unwrap();
        l.append(note(0)).unwrap();
        l.append(note(1)).unwrap();
        drop(l);
        let path = d.path().join("ledger.jsonl");
        let mut f = OpenOptions::new().append(true).open(&path).unwrap();
        f.write_all(b"{\"seq\":2,\"pre").unwrap(); // no trailing newline: a crash mid-write
        drop(f);
        let l = Ledger::open(d.path()).unwrap();
        assert_eq!(l.lines.len(), 2, "the torn line is dropped");
        assert!(
            fs::read_to_string(&path).unwrap().ends_with('\n'),
            "the file is cut back to whole lines"
        );
    }

    #[test]
    fn ledger_rejects_corrupt_middle() {
        let d = TempDir::new("corrupt");
        let mut l = Ledger::open(d.path()).unwrap();
        l.append(note(0)).unwrap();
        drop(l);
        let path = d.path().join("ledger.jsonl");
        let mut f = OpenOptions::new().append(true).open(&path).unwrap();
        f.write_all(b"this is not json\n").unwrap();
        drop(f);
        let err = Ledger::open(d.path())
            .err()
            .expect("corrupt line must refuse");
        assert!(
            err.to_string().contains("ledger line 1"),
            "names the line: {err}"
        );
    }

    #[test]
    fn ledger_is_exclusive() {
        let d = TempDir::new("lock");
        let _first = Ledger::open(d.path()).unwrap();
        let err = Ledger::open(d.path())
            .err()
            .expect("second open must refuse");
        assert_eq!(err.to_string(), LOCKED_MSG);
    }

    #[test]
    fn blob_roundtrip() {
        let d = TempDir::new("blob");
        let l = Ledger::open(d.path()).unwrap();
        let sha = l.put_blob("hello blob").unwrap();
        assert_eq!(sha, sha256_hex(b"hello blob"));
        assert_eq!(l.blob(&sha).unwrap(), "hello blob");
    }

    #[test]
    fn blob_replaces_partial_file() {
        let d = TempDir::new("blobpartial");
        let l = Ledger::open(d.path()).unwrap();
        let text = "the full evidence text";
        let sha = sha256_hex(text.as_bytes());
        // A crash mid-write left the start of the text under the final name.
        fs::write(
            d.path().join("blobs").join(format!("{sha}.txt")),
            "the full",
        )
        .unwrap();
        assert_eq!(l.put_blob(text).unwrap(), sha);
        assert_eq!(l.blob(&sha).unwrap(), text, "the partial file is replaced");
        assert!(
            !d.path().join("blobs").join(format!("{sha}.tmp")).exists(),
            "no temp file is left"
        );
    }

    #[test]
    fn blob_rejects_bad_name() {
        let d = TempDir::new("badblob");
        for bad in ["../ledger", "ABCDEF", "a", &"A".repeat(64), &"g".repeat(64)] {
            assert!(read_blob(d.path(), bad).is_err(), "{bad} must be refused");
        }
    }

    #[test]
    fn event_serde_roundtrip() {
        let events = vec![
            Event::RunStarted {
                app: "a".into(),
                task: "t".into(),
                target: "x".into(),
                mode: Mode::Fixture,
                model: "m".into(),
                disclosure: String::new(),
            },
            Event::ModelTurn {
                message: json!({"role": "assistant"}),
                usage: None,
            },
            note(0),
            Event::Evidence {
                id: "E1".into(),
                source: "s".into(),
                sha256: "x".into(),
                bytes: 3,
                flags: vec![],
            },
            Event::Claim {
                text: "c".into(),
                status: ClaimStatus::Unsupported,
                cites: vec![Cite {
                    evidence_id: "E1".into(),
                    quote: "q".into(),
                    found: false,
                }],
            },
            Event::Proposed {
                id: "P1".into(),
                call_id: "c".into(),
                tool: "t".into(),
                args: json!({}),
                summary: "s".into(),
                detail: "d".into(),
                warnings: vec![],
                inputs: BTreeMap::new(),
                influenced_by: vec![],
                digest: "d".into(),
            },
            Event::Decided {
                proposal: "P1".into(),
                approve: true,
                digest: "d".into(),
                reason: String::new(),
            },
            Event::EffectStarted {
                proposal: "P1".into(),
                digest: "d".into(),
            },
            Event::EffectDone {
                proposal: "P1".into(),
                ok: true,
                receipt: "r".into(),
            },
            Event::Blocked {
                call_id: "c".into(),
                tool: "fetch".into(),
                reason: "blocked: x".into(),
            },
            Event::RunFinished {
                status: RunStatus::NeedsReview,
                summary: String::new(),
            },
            Event::RunResumed {
                after: "timeout".into(),
            },
        ];
        for e in events {
            let s = serde_json::to_string(&e).unwrap();
            let back: Event = serde_json::from_str(&s).unwrap();
            assert_eq!(back, e);
        }
    }
}
