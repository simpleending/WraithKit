//! Research Wraith: a cited brief from sources the user allowed, with conflicts kept visible.

mod bridge;
mod guard;
#[cfg(test)]
mod tests;

use guard::{
    brief_name_error, check_url, html_to_text, is_public, next_hop, valid_brief_name, vet_hop,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    net::{SocketAddr, ToSocketAddrs},
    path::{Component, Path, PathBuf},
    time::Duration,
};
use wraithkit::{App, Clean, Demo, EvidenceSink, Kind, Mode, Preview, ToolSpec, args, sha256_hex};

const MAX_REDIRECTS: usize = 3;
const MAX_BODY: u64 = 2_000_000;
const TASK: &str =
    "When do Hollow Hacks 2026 submissions close, and how big can teams be? Write a short brief.";
const SCRIPT: &str = include_str!("../fixtures/script.json");
const DEMO_FILES: [(&str, &str); 4] = [
    ("index.json", include_str!("../fixtures/index.json")),
    ("rules.html", include_str!("../fixtures/rules.html")),
    ("guide.html", include_str!("../fixtures/guide.html")),
    ("faq.html", include_str!("../fixtures/faq.html")),
];

pub struct ResearchWraith {
    mode: Mode,
    /// Where `briefs/` is written: the target folder, or the folder that holds the sources file.
    base: PathBuf,
    /// Hosts the run may fetch. Built from the target only. The model cannot change it.
    allowed: Vec<String>,
    /// Fixture mode: URL to bundled file name.
    fixtures: BTreeMap<String, String>,
    /// Live mode: the source URLs from the sources file. They are listed in the system prompt.
    sources: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FetchArgs {
    url: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BriefArgs {
    name: String,
    markdown: String,
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .https_only(true)
        .max_redirects(0)
        .http_status_as_error(false)
        .timeout_global(Some(Duration::from_secs(20)))
        .build()
        .into()
}

/// Only ordinary directories/files may be used as brief destinations. Unlike `exists`, this
/// sees dangling links and fails closed on errors other than a genuinely missing entry.
fn plain_entry(path: &Path, directory: bool) -> std::io::Result<Option<fs::Metadata>> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    let linked = metadata.file_type().is_symlink();
    #[cfg(windows)]
    let linked = {
        use std::os::windows::fs::MetadataExt;
        linked || metadata.file_attributes() & 0x400 != 0 // any reparse point, including junctions
    };
    if linked || (directory && !metadata.is_dir()) || (!directory && !metadata.is_file()) {
        return Err(std::io::Error::other(
            "expected an ordinary path, not a link or special file",
        ));
    }
    Ok(Some(metadata))
}

/// Write completely before replacing the destination. A read/write/sync failure leaves the old
/// brief intact. The reader also lets tests simulate a failure after writing only part of a brief.
fn replace_brief(path: &Path, mut content: impl std::io::Read) -> std::io::Result<()> {
    static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let parent = path
        .parent()
        .ok_or_else(|| std::io::Error::other("brief has no parent"))?;
    plain_entry(parent, true)?;
    let existing = plain_entry(path, false)?;
    if existing
        .as_ref()
        .is_some_and(|m| m.permissions().readonly())
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "brief is read-only",
        ));
    }
    let tmp = parent.join(format!(
        ".wraith-{}-{}-{}.tmp",
        std::process::id(),
        wraithkit::now_ms(),
        N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    // create_new never follows a planted link. Do not remove anything unless we created it.
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp)?;
    let result = (|| {
        if let Some(metadata) = existing {
            file.set_permissions(metadata.permissions())?;
        }
        std::io::copy(&mut content, &mut file)?;
        file.sync_all()?;
        drop(file);
        plain_entry(parent, true)?;
        plain_entry(path, false)?;
        fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

impl ResearchWraith {
    fn write_brief(&self, a: &BriefArgs) -> Result<String, String> {
        let (rel, path) = self.brief_path(&a.name)?;
        let parent = self.base.join("briefs");
        match fs::create_dir(&parent) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(format!("could not create briefs/: {e}")),
        }
        // Check again after creating the parent, before opening any output.
        self.brief_path(&a.name)?;
        replace_brief(&path, a.markdown.as_bytes())
            .map_err(|e| format!("could not write {rel}: {e}"))?;
        Ok(format!("saved {rel} ({} bytes)", a.markdown.len()))
    }

    /// The bundled file for a fixture URL. Only checked names from the index are read.
    fn fixture_fetch(&self, url: &str) -> Result<(String, String), String> {
        let file = self
            .fixtures
            .get(url)
            .ok_or_else(|| format!("blocked: {url} is not in the fixture index"))?;
        let mut parts = Path::new(file).components();
        if !matches!(
            (parts.next(), parts.next()),
            (Some(Component::Normal(_)), None)
        ) {
            return Err(format!(
                "fixture index entry for {url} must be a plain file name"
            ));
        }
        let raw =
            fs::read_to_string(self.base.join(file)).map_err(|e| format!("fixture {file}: {e}"))?;
        let text = if file.ends_with(".html") {
            html_to_text(&raw)
        } else {
            raw
        };
        Ok((url.to_string(), text))
    }

    /// Live fetch with a per-hop check: allowlist, public addresses, and at most MAX_REDIRECTS redirects.
    fn fetch_live(&self, start: &str) -> Result<(String, String), String> {
        let http = agent();
        let mut url = start.to_string();
        let mut hops = 0;
        loop {
            let host = vet_hop(&url, &self.allowed)?;
            let addrs: Vec<SocketAddr> = (host.as_str(), 443)
                .to_socket_addrs()
                .map_err(|e| format!("DNS lookup failed for {host}: {e}"))?
                .collect();
            if let Some(a) = addrs.iter().find(|a| !is_public(a.ip())) {
                return Err(format!("blocked: {host} resolves to non-public {}", a.ip()));
            }
            // shortcut: ureq resolves DNS again when it connects, so a DNS rebinding window exists. Acceptable while hosts come from the user's sources file. Upgrade with a custom resolver if hosts ever come from the model.
            let mut resp = http
                .get(&url)
                .call()
                .map_err(|e| format!("fetch failed: {e}"))?;
            let status = resp.status().as_u16();
            let location = resp
                .headers()
                .get("location")
                .and_then(|v| v.to_str().ok())
                .map(str::to_owned);
            if let Some(next) = next_hop(&url, status, location.as_deref())? {
                hops += 1;
                if hops > MAX_REDIRECTS {
                    return Err("blocked: too many redirects".into());
                }
                url = next;
                continue;
            }
            if status != 200 {
                return Err(format!("HTTP {status} from {url}"));
            }
            let ctype = resp
                .headers()
                .get("content-type")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
                .to_ascii_lowercase();
            if !(ctype.starts_with("text/") || ctype.contains("json")) {
                return Err(format!("unsupported content type {ctype:?}"));
            }
            let body = resp
                .body_mut()
                .with_config()
                .limit(MAX_BODY)
                .read_to_string()
                .map_err(|e| format!("read failed: {e}"))?;
            let text = if ctype.contains("html") {
                html_to_text(&body)
            } else {
                body
            };
            return Ok((url, text));
        }
    }

    fn brief_path(&self, name: &str) -> Result<(String, PathBuf), String> {
        // `base` is resolved once at open. Do not follow a replacement of it or an ancestor.
        let current = fs::canonicalize(&self.base)
            .map_err(|e| format!("cannot resolve brief folder: {e}"))?;
        if current != self.base {
            return Err("blocked: the brief folder has moved or become a link".into());
        }
        let parent = self.base.join("briefs");
        plain_entry(&parent, true).map_err(|e| format!("blocked: briefs/: {e}"))?;
        let rel = format!("briefs/{name}.md");
        let path = parent.join(format!("{name}.md"));
        plain_entry(&path, false).map_err(|e| format!("blocked: {rel}: {e}"))?;
        Ok((rel, path))
    }
}

impl App for ResearchWraith {
    const NAME: &'static str = "research-wraith";
    const DEMO_SCRIPT: &'static str = SCRIPT;

    fn open(target: &Path, mode: Mode) -> Result<Self, String> {
        match mode {
            Mode::Fixture => {
                let raw = fs::read_to_string(target.join("index.json"))
                    .map_err(|e| format!("fixture folder needs index.json: {e}"))?;
                let fixtures: BTreeMap<String, String> =
                    serde_json::from_str(&raw).map_err(|e| format!("index.json: {e}"))?;
                // Entries that fail the URL rules are left out of the allowlist. A fetch of one is still refused by name.
                let allowed = fixtures.keys().filter_map(|u| check_url(u).ok()).collect();
                let sources = fixtures.keys().cloned().collect();
                Ok(ResearchWraith {
                    mode,
                    base: fs::canonicalize(target)
                        .map_err(|e| format!("cannot resolve target: {e}"))?,
                    allowed,
                    fixtures,
                    sources,
                })
            }
            Mode::Live => {
                let raw = fs::read_to_string(target)
                    .map_err(|e| format!("cannot read sources file {}: {e}", target.display()))?;
                let mut sources = Vec::new();
                let mut allowed = Vec::new();
                for (i, line) in raw.lines().enumerate() {
                    let line = line.trim();
                    if line.is_empty() || line.starts_with('#') {
                        continue;
                    }
                    let host =
                        check_url(line).map_err(|e| format!("sources file line {}: {e}", i + 1))?;
                    allowed.push(host);
                    sources.push(line.to_string());
                }
                if sources.is_empty() {
                    return Err("the sources file lists no https URLs".into());
                }
                let base = target
                    .parent()
                    .filter(|p| !p.as_os_str().is_empty())
                    .unwrap_or(Path::new("."))
                    .to_path_buf();
                Ok(ResearchWraith {
                    mode,
                    base: fs::canonicalize(base)
                        .map_err(|e| format!("cannot resolve source folder: {e}"))?,
                    allowed,
                    fixtures: BTreeMap::new(),
                    sources,
                })
            }
        }
    }

    fn system_prompt(&self) -> String {
        let mut p = include_str!("prompt.md").to_string();
        p.push_str("\nSources you may fetch:\n");
        for s in &self.sources {
            p.push_str(&format!("- {s}\n"));
        }
        p
    }

    fn tools(&self) -> Vec<ToolSpec> {
        vec![
            ToolSpec {
                name: "fetch",
                kind: Kind::Probe,
                description: "Fetch one allowed source. Returns an evidence ID to cite.",
                parameters: json!({"type": "object", "additionalProperties": false, "required": ["url"],
                    "properties": {"url": {"type": "string"}}}),
            },
            ToolSpec {
                name: "save_brief",
                kind: Kind::Effect,
                description: "Propose saving the brief as briefs/<name>.md. The user must approve.",
                parameters: json!({"type": "object", "additionalProperties": false, "required": ["name", "markdown"],
                    "properties": {
                        "name": {"type": "string", "description": "File name without an extension: 1 to 40 characters of a-z, 0-9 and hyphens only, for example rfc2606-domains"},
                        "markdown": {"type": "string"}}}),
            },
        ]
    }

    fn probe(&self, ev: &mut EvidenceSink<'_>, tool: &str, a: &Value) -> Result<String, String> {
        if tool != "fetch" {
            return Err(format!("unknown tool {tool}"));
        }
        let a: FetchArgs = args(a)?;
        vet_hop(&a.url, &self.allowed)?;
        let (final_url, text) = match self.mode {
            Mode::Fixture => self.fixture_fetch(&a.url)?,
            Mode::Live => self.fetch_live(&a.url)?,
        };
        let id = ev.add(&format!("web:{final_url}"), &text)?;
        Ok(format!("[{id}] {final_url}\n{text}"))
    }

    fn preview(&self, tool: &str, a: &Value) -> Result<Preview, String> {
        if tool != "save_brief" {
            return Err(format!("unknown effect {tool}"));
        }
        let a: BriefArgs = args(a)?;
        if !valid_brief_name(&a.name) {
            return Err(brief_name_error(&a.name));
        }
        let (rel, path) = self.brief_path(&a.name)?;
        let (state, warnings) = match fs::read(&path) {
            Ok(bytes) => (
                sha256_hex(&bytes),
                vec!["This replaces an existing brief.".to_string()],
            ),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => ("absent".to_string(), vec![]),
            Err(e) => return Err(format!("cannot read {rel}: {e}")),
        };
        let lines = a.markdown.lines().count();
        Ok(Preview {
            summary: format!("Write {rel} ({lines} lines)"),
            // The whole brief: anything cut from the card would be written without being seen.
            detail: a.markdown.clone(),
            warnings,
            inputs: BTreeMap::from([
                (rel, state),
                // Bind the real destination across process restarts as well as within a run.
                (
                    "resolved_destination".into(),
                    sha256_hex(path.as_os_str().as_encoded_bytes()),
                ),
            ]),
        })
    }

    fn apply(&self, tool: &str, a: Clean<Value>) -> Result<String, String> {
        if tool != "save_brief" {
            return Err(format!("unknown effect {tool}"));
        }
        let a: BriefArgs = args(a.get())?;
        if !valid_brief_name(&a.name) {
            return Err(brief_name_error(&a.name));
        }
        self.write_brief(&a)
    }

    fn demo() -> Result<Demo, String> {
        let dir = wraithkit::demo_dir(Self::NAME);
        fs::create_dir_all(&dir).map_err(|e| format!("could not create {}: {e}", dir.display()))?;
        for (name, body) in DEMO_FILES {
            fs::write(dir.join(name), body).map_err(|e| format!("could not write {name}: {e}"))?;
        }
        Ok(Demo {
            target: dir,
            task: TASK.into(),
        })
    }
}

fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    if let Some(result) = bridge::command(&argv) {
        match result {
            Ok(code) => std::process::exit(code),
            Err(error) => {
                eprintln!("error: {error}");
                std::process::exit(2);
            }
        }
    }
    if argv.is_empty()
        || argv
            .iter()
            .any(|a| ["help", "--help", "-h"].contains(&a.as_str()))
    {
        println!(
            "research-wraith bridge <handoff.json> [--port N] [--open]   offline release brief; fresh approval\nresearch-wraith bridge-resume <run-folder> [--port N] [--open]   resume that offline bridge\n"
        );
    }
    wraithkit::cli::main::<ResearchWraith>()
}
