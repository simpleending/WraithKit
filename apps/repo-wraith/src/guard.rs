//! Path confinement and secret-file rules. Everything the model reads goes through here.

use std::path::{Component, Path, PathBuf};

pub const MAX_FILE_BYTES: u64 = 256 * 1024;

/// `root` must already be canonicalized. Returns the canonical path of a file inside it.
pub fn resolve_in(root: &Path, rel: &str) -> Result<PathBuf, String> {
    let p = Path::new(rel);
    let plain = p.components().all(|c| matches!(c, Component::Normal(_)));
    if rel.is_empty() || rel.contains(':') || p.is_absolute() || !plain {
        return Err(format!(
            "blocked: {rel}: use a relative path inside the repository"
        ));
    }
    // .git holds credentials. .wraith holds this tool's own run records, which the model must not read or edit.
    let off_limits = |q: &Path| {
        q.components().any(|c| {
            c.as_os_str().eq_ignore_ascii_case(".git")
                || c.as_os_str().eq_ignore_ascii_case(".wraith")
        })
    };
    if off_limits(p) {
        return Err(format!("blocked: {rel}: .git and .wraith are off limits"));
    }
    let full = root
        .join(p)
        .canonicalize()
        .map_err(|_| format!("{rel}: not found"))?;
    // A symlink or junction that leaves the repository canonicalizes outside `root`.
    if !full.starts_with(root) {
        return Err(format!("blocked: {rel}: resolves outside the repository"));
    }
    // A symlink inside the repository can still point into one of them.
    if full.strip_prefix(root).is_ok_and(off_limits) {
        return Err(format!("blocked: {rel}: .git and .wraith are off limits"));
    }
    Ok(full)
}

/// Names that look like credentials. Refused for read and for edit.
pub fn is_secret_name(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n == ".env"
        || n.starts_with(".env.")
        || n.starts_with("id_rsa")
        || n.starts_with("id_ed25519")
        || n.starts_with("credentials")
        || n.starts_with("secrets.")
        || [".npmrc", ".pypirc", ".netrc"].contains(&n.as_str())
        || [".pem", ".key", ".pfx", ".p12"]
            .iter()
            .any(|e| n.ends_with(e))
}

/// Paths that run code with repository secrets, or change how the project builds.
pub fn is_build_or_ci(rel: &str) -> bool {
    // Windows accepts `\` as a separator, so `.github\actions\x.yml` must warn too.
    let lower = rel.replace('\\', "/").to_ascii_lowercase();
    lower.starts_with(".github/")
        || ["workflow", "makefile", "build.rs", "package.json"]
            .iter()
            .any(|k| lower.contains(k))
}
