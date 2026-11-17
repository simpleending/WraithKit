//! Fixed local files only. No paths, network destinations or commands come from the model.
use crate::bundle::MAX_BYTES;
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

pub fn read(root: &Path, name: &str, optional: bool) -> Result<Option<String>, String> {
    let p = root.join(name);
    let meta = match fs::symlink_metadata(&p) {
        Ok(m) => m,
        Err(e) if optional && e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("cannot read {name}: {e}")),
    };
    if is_link(&meta) || !meta.is_file() {
        return Err(format!("blocked: {name} must be a regular, unlinked file"));
    }
    let resolved = p
        .canonicalize()
        .map_err(|e| format!("cannot resolve {name}: {e}"))?;
    if resolved.parent() != Some(root) {
        return Err(format!(
            "blocked: {name} resolves outside the bundle directory"
        ));
    }
    if meta.len() > MAX_BYTES {
        return Err(format!("blocked: {name} exceeds 64 KiB"));
    }
    let mut bytes = Vec::new();
    fs::File::open(&p)
        .and_then(|f| f.take(MAX_BYTES + 1).read_to_end(&mut bytes))
        .map_err(|e| format!("cannot read {name}: {e}"))?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err(format!("blocked: {name} exceeds 64 KiB"));
    }
    String::from_utf8(bytes)
        .map(Some)
        .map_err(|_| format!("{name} must be UTF-8"))
}

fn is_link(m: &fs::Metadata) -> bool {
    if m.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if m.file_attributes() & 0x400 != 0 {
            return true;
        }
    }
    false
}

pub fn root(target: &Path) -> Result<PathBuf, String> {
    let p = target
        .canonicalize()
        .map_err(|e| format!("cannot open bundle directory: {e}"))?;
    if !p.is_dir() {
        return Err("target must be a bundle directory containing incident.json".into());
    }
    Ok(p)
}

pub fn replace(root: &Path, text: &str) -> Result<(), String> {
    static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let tmp = root.join(format!(
        ".security-wraith-{}-{}-{}.tmp",
        std::process::id(),
        wraithkit::now_ms(),
        N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp)
        .map_err(|e| format!("cannot create local response-plan temporary file: {e}"))?;
    let write = (|| -> std::io::Result<()> {
        f.write_all(text.as_bytes())?;
        f.sync_all()?;
        drop(f);
        fs::rename(&tmp, root.join("response-plan.json"))
    })();
    if let Err(e) = write {
        let _ = fs::remove_file(&tmp); // only the temp name this call created, never a supplied path
        return Err(format!("could not write local response-plan.json: {e}"));
    }
    Ok(())
}
