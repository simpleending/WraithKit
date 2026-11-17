//! Repo Wraith: finds a bug in a repository and proposes an edit the user must approve.

mod guard;
#[cfg(test)]
mod tests;

use guard::{MAX_FILE_BYTES, is_build_or_ci, is_secret_name, resolve_in};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
use wraithkit::{App, Clean, Demo, EvidenceSink, Kind, Mode, Preview, ToolSpec, args, sha256_hex};

const SKIP_DIRS: &[&str] = &[".git", "target", "node_modules", ".wraith"];
const MAX_LIST: usize = 500;
const MAX_HITS: usize = 50;
const TASK: &str =
    "Customers report invoices are off by one cent. Find the cause and propose a fix.";
const SCRIPT: &str = include_str!("../fixtures/script.json");
const DEMO_FILES: [(&str, &str); 6] = [
    ("README.md", include_str!("../fixtures/demo-repo/README.md")),
    (
        "invoice/money.py",
        include_str!("../fixtures/demo-repo/invoice/money.py"),
    ),
    (
        "invoice/total.py",
        include_str!("../fixtures/demo-repo/invoice/total.py"),
    ),
    (
        "tests/test_money.py",
        include_str!("../fixtures/demo-repo/tests/test_money.py"),
    ),
    (
        ".github/workflows/ci.yml",
        include_str!("../fixtures/demo-repo/.github/workflows/ci.yml"),
    ),
    (".env", include_str!("../fixtures/demo-repo/.env")),
];

pub struct RepoWraith {
    root: PathBuf,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NoArgs {}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReadArgs {
    path: String,
    start_line: Option<usize>,
    end_line: Option<usize>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SearchArgs {
    text: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EditArgs {
    path: String,
    find: String,
    replace: String,
}

/// Every file under the root that a probe may list or search. Skips symlinks, secret names and SKIP_DIRS.
fn walk(dir: &Path, rel: &str, out: &mut Vec<String>) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    let mut entries: Vec<_> = rd.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let name = e.file_name().to_string_lossy().into_owned();
        let Ok(ft) = e.file_type() else { continue };
        if ft.is_symlink() {
            continue;
        }
        if ft.is_dir() {
            if !SKIP_DIRS.contains(&name.as_str()) {
                walk(&e.path(), &format!("{rel}{name}/"), out);
            }
        } else if ft.is_file() && !is_secret_name(&name) {
            out.push(format!("{rel}{name}"));
        }
    }
}

/// Atomically replace an existing target via an exclusively created sibling temporary file.
/// Paths come from the app, never directly from model arguments. Kept separate so collision
/// and failed-replacement behavior can be exercised without relying on timing.
fn replace_file(full: &Path, tmp: &Path, content: &[u8]) -> std::io::Result<()> {
    // create_new refuses existing entries, including planted links. Cleanup is permitted
    // only after this call successfully created the temporary file.
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(tmp)?;
    let result = (|| {
        std::io::Write::write_all(&mut f, content)?;
        f.sync_data()?;
        drop(f);
        // Keep the file's mode, for example the executable bit on a script.
        fs::set_permissions(tmp, fs::metadata(full)?.permissions())?;
        fs::rename(tmp, full)
    })();
    if result.is_err() {
        let _ = fs::remove_file(tmp);
    }
    result
}

impl RepoWraith {
    fn files(&self) -> Vec<String> {
        let mut out = Vec::new();
        walk(&self.root, "", &mut out);
        out
    }

    /// Reads a text file under the root. Refuses secrets, large files and non-UTF-8 content.
    fn read_text(&self, rel: &str) -> Result<(PathBuf, String), String> {
        let full = resolve_in(&self.root, rel)?;
        let name = full
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        if is_secret_name(name) {
            return Err(format!("blocked: {rel}: looks like a secrets file"));
        }
        let meta = fs::metadata(&full).map_err(|e| format!("{rel}: {e}"))?;
        if !meta.is_file() {
            return Err(format!("{rel}: not a file"));
        }
        if meta.len() > MAX_FILE_BYTES {
            return Err(format!("blocked: {rel}: larger than 256 KB"));
        }
        let text =
            fs::read_to_string(&full).map_err(|_| format!("blocked: {rel}: not UTF-8 text"))?;
        Ok((full, text))
    }

    /// The exact (find, replace) pair to apply. `find` must match exactly once.
    /// `read_file` shows lines without `\r`, so in a CRLF file a multi-line `find` arrives with `\n`;
    /// it is then matched, and replaced, with `\r\n`.
    fn exact_edit(
        text: &str,
        find: &str,
        replace: &str,
        path: &str,
    ) -> Result<(String, String), String> {
        if find.is_empty() {
            return Err("find must not be empty".into());
        }
        // First and last hit at the same place: exactly one occurrence, overlapping ones included
        // (`matches().count()` would count "aXa" in "aXaXa" once).
        let once = |p: &str| matches!((text.find(p), text.rfind(p)), (Some(a), Some(b)) if a == b);
        if once(find) {
            return Ok((find.into(), replace.into()));
        }
        let crlf = |s: &str| s.replace("\r\n", "\n").replace('\n', "\r\n");
        if !text.contains(find) && find.contains('\n') && once(&crlf(find)) {
            return Ok((crlf(find), crlf(replace)));
        }
        let n = text.match_indices(find).count();
        let n = if n == 1 {
            "more than once (overlapping)".into()
        } else {
            format!("{n} times")
        };
        Err(format!(
            "find text must match exactly once in {path}; it matched {n}"
        ))
    }

    /// The edited file's path relative to the root, as it really is on disk. The card shows this, not
    /// the string the model typed: a short name (`GITHUB~1/...`) or an in-repo symlink can point
    /// somewhere the typed path does not reveal.
    fn real_rel(&self, full: &Path) -> String {
        full.strip_prefix(&self.root).map_or_else(
            |_| full.display().to_string(),
            |p| p.to_string_lossy().replace('\\', "/"),
        )
    }
}

impl App for RepoWraith {
    const NAME: &'static str = "repo-wraith";
    const DEMO_SCRIPT: &'static str = SCRIPT;

    fn open(target: &Path, _mode: Mode) -> Result<Self, String> {
        let root = target
            .canonicalize()
            .map_err(|e| format!("{}: {e}", target.display()))?;
        if !root.is_dir() {
            return Err(format!("{} is not a folder", root.display()));
        }
        Ok(RepoWraith { root })
    }

    fn system_prompt(&self) -> String {
        include_str!("prompt.md").into()
    }

    fn tools(&self) -> Vec<ToolSpec> {
        vec![
            ToolSpec {
                name: "list_files",
                kind: Kind::Probe,
                description: "List files in the repository.",
                parameters: json!({"type": "object", "additionalProperties": false, "properties": {}}),
            },
            ToolSpec {
                name: "read_file",
                kind: Kind::Probe,
                description: "Read a text file. Returns an evidence ID to cite.",
                parameters: json!({"type": "object", "additionalProperties": false, "required": ["path"],
                    "properties": {"path": {"type": "string"},
                        "start_line": {"type": "integer", "minimum": 1},
                        "end_line": {"type": "integer", "minimum": 1}}}),
            },
            ToolSpec {
                name: "search",
                kind: Kind::Probe,
                description: "Find a literal, case-sensitive string in the repository.",
                parameters: json!({"type": "object", "additionalProperties": false, "required": ["text"],
                    "properties": {"text": {"type": "string", "minLength": 2}}}),
            },
            ToolSpec {
                name: "edit_file",
                kind: Kind::Effect,
                description: "Propose replacing one exact block of text in an existing file. The user must approve.",
                parameters: json!({"type": "object", "additionalProperties": false,
                    "required": ["path", "find", "replace"],
                    "properties": {"path": {"type": "string"}, "find": {"type": "string", "minLength": 1},
                        "replace": {"type": "string"}}}),
            },
        ]
    }

    fn probe(&self, ev: &mut EvidenceSink<'_>, tool: &str, a: &Value) -> Result<String, String> {
        match tool {
            "read_file" => {
                let a: ReadArgs = args(a)?;
                let (_, text) = self.read_text(&a.path)?;
                let id = ev.add(&format!("repo:{}", a.path), &text)?;
                let lines: Vec<&str> = text.lines().collect();
                let total = lines.len();
                let s = a.start_line.unwrap_or(1).max(1);
                let e = a.end_line.unwrap_or(total).min(total);
                let body: String = (s..=e)
                    .filter_map(|n| lines.get(n - 1).map(|l| format!("{n:>4} | {l}\n")))
                    .collect();
                Ok(format!(
                    "[{id}] {} lines {s}-{e} of {total}\n{body}",
                    a.path
                ))
            }
            "list_files" => {
                let _: NoArgs = args(a)?;
                let files = self.files();
                let mut lines: Vec<String> = files
                    .iter()
                    .take(MAX_LIST)
                    .map(|f| {
                        let bytes = fs::metadata(self.root.join(f)).map_or(0, |m| m.len());
                        format!("{f} ({bytes} bytes)")
                    })
                    .collect();
                if files.len() > MAX_LIST {
                    lines.push(format!("... and {} more", files.len() - MAX_LIST));
                }
                Ok(lines.join("\n"))
            }
            "search" => {
                let a: SearchArgs = args(a)?;
                if a.text.chars().count() < 2 {
                    return Err("search text must be at least 2 characters".into());
                }
                let mut hits = vec![];
                'files: for f in self.files() {
                    let Ok((_, text)) = self.read_text(&f) else {
                        continue;
                    };
                    for (i, line) in text.lines().enumerate() {
                        if line.contains(&a.text) {
                            hits.push(format!("{f}:{}: {}", i + 1, line.trim()));
                            if hits.len() == MAX_HITS {
                                break 'files;
                            }
                        }
                    }
                }
                if hits.is_empty() {
                    return Ok("no matches".into());
                }
                // The hit lines are file text the model reads, so they are evidence: they then count in
                // a proposal's `influenced_by` and get the agent-phrase flag, like `read_file` output.
                let text = hits.join("\n");
                let id = ev.add(&format!("repo:search:{}", a.text), &text)?;
                Ok(format!("[{id}] {} hits\n{text}", hits.len()))
            }
            _ => Err(format!("unknown tool {tool}")),
        }
    }

    fn preview(&self, tool: &str, a: &Value) -> Result<Preview, String> {
        if tool != "edit_file" {
            return Err(format!("unknown effect {tool}"));
        }
        let a: EditArgs = args(a)?;
        let (full, text) = self.read_text(&a.path)?;
        Self::exact_edit(&text, &a.find, &a.replace, &a.path)?;
        let rel = self.real_rel(&full);
        let mut warnings = vec![];
        if is_build_or_ci(&rel) || is_build_or_ci(&a.path) {
            warnings.push(
                "This file controls builds or CI, which can run code with repository secrets."
                    .into(),
            );
        }
        if rel != a.path.replace('\\', "/") {
            warnings.push(format!(
                "The model asked for {}, which is really {rel}.",
                a.path
            ));
        }
        let detail: String = a
            .find
            .lines()
            .map(|l| format!("- {l}\n"))
            .chain(a.replace.lines().map(|l| format!("+ {l}\n")))
            .collect();
        Ok(Preview {
            summary: format!("Edit {rel}: replace 1 block"),
            detail,
            warnings,
            inputs: BTreeMap::from([
                (rel, sha256_hex(text.as_bytes())),
                // A resumed run may resolve its root alias somewhere else with identical contents.
                (
                    "resolved_destination".into(),
                    sha256_hex(full.as_os_str().as_encoded_bytes()),
                ),
            ]),
        })
    }

    fn apply(&self, tool: &str, a: Clean<Value>) -> Result<String, String> {
        if tool != "edit_file" {
            return Err(format!("unknown effect {tool}"));
        }
        let a: EditArgs = args(a.get())?;
        let (full, text) = self.read_text(&a.path)?;
        let (find, replace) = Self::exact_edit(&text, &a.find, &a.replace, &a.path)?;
        let new = text.replacen(&find, &replace, 1);
        // A unique sibling keeps replacement on one volume without deleting another file first.
        static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let tmp = full.with_file_name(format!(
            ".repo-wraith-{}-{}-{}.tmp",
            std::process::id(),
            wraithkit::now_ms(),
            N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        let rel = self.real_rel(&full);
        replace_file(&full, &tmp, new.as_bytes())
            .map_err(|e| format!("could not write {rel}: {e}"))?;
        Ok(format!(
            "edited {rel} (sha256 {} -> {})",
            &sha256_hex(text.as_bytes())[..12],
            &sha256_hex(new.as_bytes())[..12]
        ))
    }

    fn demo() -> Result<Demo, String> {
        let dir = wraithkit::demo_dir(Self::NAME);
        for (rel, body) in DEMO_FILES {
            let p = dir.join(rel);
            if let Some(parent) = p.parent() {
                fs::create_dir_all(parent)
                    .map_err(|e| format!("could not create {}: {e}", parent.display()))?;
            }
            fs::write(&p, body).map_err(|e| format!("could not write {}: {e}", p.display()))?;
        }
        Ok(Demo {
            target: dir,
            task: TASK.into(),
        })
    }
}

fn main() {
    wraithkit::cli::main::<RepoWraith>()
}
