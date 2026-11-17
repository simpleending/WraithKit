//! Command line shared by all Wraith apps: `demo`, `run`, `resume`, `verify`, `help`.

use crate::{
    App, Mode,
    ledger::{Event, Line},
    model::{FIXTURE_LABEL, Model},
    server,
    session::Session,
    verify::{all_ok, verify},
};
use std::path::{Path, PathBuf};

const RUNS_DIR: &str = ".wraith/runs";

#[derive(Debug, Default, PartialEq)]
pub struct Opts {
    pub port: u16,
    pub open: bool,
    pub live: bool,
    pub pos: Vec<String>,
}

pub fn parse_opts(args: &[String]) -> Result<Opts, String> {
    let mut o = Opts::default();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--port" => {
                let v = it.next().ok_or("--port needs a number")?;
                o.port = v
                    .parse()
                    .map_err(|_| format!("--port must be a number, got {v}"))?;
            }
            "--open" => o.open = true,
            "--live" => o.live = true,
            f if f.starts_with("--") => return Err(format!("unknown option {f}")),
            _ => o.pos.push(a.clone()),
        }
    }
    Ok(o)
}

/// Entry point. Each app's `main` calls this once.
pub fn main<A: App>() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = match run::<A>(&args) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {e}");
            1
        }
    };
    std::process::exit(code);
}

fn help(name: &str) -> String {
    format!(
        "{name} demo   [--port N] [--open]                 fixture model and bundled files (offline)\n\
         {name} run    <target> \"<task>\" --live [--port N] [--open]\n\
         {name} resume <run-dir> [--live] [--port N] [--open]\n\
         {name} verify <run-dir>                            check a run's ledger offline\n\
         {name} help\n\
         Runs are written to ./.wraith/runs/. Port 0 picks a free port."
    )
}

fn run<A: App>(args: &[String]) -> Result<i32, String> {
    let Some((cmd, rest)) = args.split_first() else {
        println!("{}", help(A::NAME));
        return Ok(0);
    };
    let opts = parse_opts(rest)?;
    match cmd.as_str() {
        "demo" => {
            let d = A::demo()?;
            println!("{FIXTURE_LABEL}");
            let model = Model::fixture(A::DEMO_SCRIPT)?;
            let s = Session::<A>::start(
                Path::new(RUNS_DIR),
                &d.target,
                &d.task,
                Mode::Fixture,
                model,
            )?;
            serve(s, &opts)
        }
        "run" => {
            let [target, task] = opts.pos.as_slice() else {
                return Err(format!("usage: {} run <target> \"<task>\" --live", A::NAME));
            };
            if !opts.live {
                return Err("run needs --live (demo is the offline mode)".into());
            }
            let model = Model::live_from_env()?;
            println!("{}", model.label());
            println!("{}", model.disclosure());
            let s = Session::<A>::start(
                Path::new(RUNS_DIR),
                Path::new(target),
                task,
                Mode::Live,
                model,
            )?;
            serve(s, &opts)
        }
        "resume" => {
            let [dir] = opts.pos.as_slice() else {
                return Err(format!("usage: {} resume <run-dir> [--live]", A::NAME));
            };
            let dir = PathBuf::from(dir);
            let live = run_is_live(&dir)?;
            let model = match (live, opts.live) {
                (true, false) => {
                    return Err("this run used live mode; add --live to resume it".into());
                }
                (false, true) => {
                    return Err("this run is a fixture run; resume it without --live".into());
                }
                (true, true) => {
                    let m = Model::live_from_env()?;
                    println!("{}", m.disclosure());
                    m
                }
                (false, false) => Model::fixture(A::DEMO_SCRIPT)?,
            };
            let s = Session::<A>::resume(&dir, model)?;
            serve(s, &opts)
        }
        "verify" => {
            let [dir] = opts.pos.as_slice() else {
                return Err(format!("usage: {} verify <run-dir>", A::NAME));
            };
            let (checks, head) = verify(Path::new(dir))?;
            println!("verify {dir}");
            for c in &checks {
                let mark = if c.ok { "ok  " } else { "FAIL" };
                println!("  {mark} {:<10} {}", c.name, c.detail);
            }
            println!("head {head}");
            println!(
                "(note the head: the ledger is tamper-evident relative to this hash, not signed)"
            );
            Ok(if all_ok(&checks) { 0 } else { 1 })
        }
        "help" | "--help" | "-h" => {
            println!("{}", help(A::NAME));
            Ok(0)
        }
        other => Err(format!("unknown command {other}; try `{} help`", A::NAME)),
    }
}

/// Starts the UI for a session and blocks until the process stops.
pub fn serve<A: App>(s: Session<A>, opts: &Opts) -> Result<i32, String> {
    let (listener, port) = server::bind(opts.port)?;
    let token = server::new_token();
    let url = format!("http://127.0.0.1:{port}/{token}/");
    println!("run folder: {}", s.run_dir().display());
    println!("open {url}");
    if opts.open {
        open_browser(&url);
    }
    let dir: PathBuf = s.run_dir().to_path_buf();
    let result = server::run(listener, port, &token, s);
    println!("run folder: {}", dir.display());
    result.map(|()| 0)
}

fn open_browser(url: &str) {
    #[cfg(windows)]
    let spawned = std::process::Command::new("cmd")
        .args(["/c", "start", "", url])
        .spawn();
    #[cfg(target_os = "macos")]
    let spawned = std::process::Command::new("open").arg(url).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let spawned = std::process::Command::new("xdg-open").arg(url).spawn();
    if spawned.is_err() {
        println!("could not open a browser; open the URL above by hand");
    }
}

/// True when the run was started in live mode. Reads only the first ledger line.
pub fn run_is_live(run_dir: &Path) -> Result<bool, String> {
    let text = std::fs::read_to_string(run_dir.join("ledger.jsonl"))
        .map_err(|e| format!("cannot read {}: {e}", run_dir.display()))?;
    let first = text.lines().next().ok_or("the run has no events")?;
    let line: Line =
        serde_json::from_str(first).map_err(|e| format!("first event does not parse: {e}"))?;
    match line.event {
        Event::RunStarted { mode, .. } => Ok(mode == Mode::Live),
        _ => Err(format!("{} is not a WraithKit run", run_dir.display())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::Counter;
    use crate::testutil::TempDir;

    fn args(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn live_requires_flag() {
        let err = run::<Counter>(&args(&["run", ".", "a task"])).unwrap_err();
        assert!(err.contains("--live"), "{err}");
    }

    #[test]
    fn parses_options_and_positionals() {
        let o = parse_opts(&args(&[
            "target",
            "task text",
            "--live",
            "--port",
            "8123",
            "--open",
        ]))
        .unwrap();
        assert!(o.live && o.open);
        assert_eq!(o.port, 8123);
        assert_eq!(o.pos, ["target", "task text"]);
        assert!(parse_opts(&args(&["--port", "abc"])).is_err());
        assert!(parse_opts(&args(&["--port"])).is_err());
        assert!(parse_opts(&args(&["--nope"])).is_err());
        assert_eq!(parse_opts(&[]).unwrap(), Opts::default());
    }

    #[test]
    fn unknown_command_and_missing_args_are_errors() {
        assert!(run::<Counter>(&args(&["frobnicate"])).is_err());
        assert!(
            run::<Counter>(&args(&["verify"])).is_err(),
            "verify needs a run folder"
        );
        assert!(run::<Counter>(&args(&["resume"])).is_err());
    }

    #[test]
    fn run_is_live_reads_the_mode() {
        let d = TempDir::new("cli-mode");
        let s = crate::testkit::start(d.path(), vec![]);
        assert!(!run_is_live(s.run_dir()).unwrap());
        assert!(
            run_is_live(d.path()).is_err(),
            "a folder with no run is refused"
        );
    }
}
