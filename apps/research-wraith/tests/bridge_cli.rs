use std::{fs, process::Command};
#[test]
fn bridge_invalid_inputs_never_start_a_session_or_network() {
    let dir = wraithkit::demo_dir("research-bridge-cli");
    fs::create_dir_all(&dir).unwrap();
    let dir = dir.canonicalize().unwrap();
    let file = dir.join("bad.json");
    fs::write(&file, "{}").unwrap();
    for args in [
        vec!["bridge".to_string()],
        vec!["bridge".into(), file.display().to_string()],
        vec!["bridge".into(), file.display().to_string(), "--live".into()],
        vec!["bridge-resume".into(), dir.display().to_string()],
    ] {
        let out = Command::new(env!("CARGO_BIN_EXE_research-wraith"))
            .args(args)
            .current_dir(&dir)
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(2));
        assert!(!dir.join(".wraith").exists());
    }
    fs::remove_dir_all(dir).unwrap();
}
