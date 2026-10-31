//! Build the hadith corpus from the deen-data snapshot's editions.
//!
//! The 28 editions (about 120 MB) come with deen-data, which the install
//! unpacks before Deen first opens. `scripts/join-hadith` writes the jsonl
//! corpus from them, collections.json last, and prints a line as it finishes
//! each collection; each line is a progress event here, in bytes of the
//! editions joined so far.

use std::{
    fs,
    io::{BufRead, BufReader, Write},
    os::unix::process::CommandExt,
    path::PathBuf,
    process::{Command, Stdio},
    time::Instant,
};

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};

use crate::hadith;

const COLLECTIONS: [&str; 10] = [
    "bukhari", "muslim", "abudawud", "tirmidhi", "nasai", "ibnmajah", "malik", "nawawi", "qudsi",
    "dehlawi",
];
const LANGS: [&str; 3] = ["ara", "eng", "ben"];
const JOIN: &str = include_str!("../scripts/join-hadith");
/// What join-hadith writes. A corpus of another format was joined by an older
/// deen, so status reads it as missing and the next open rebuilds it.
const FORMAT: u64 = 3;

/// The deen-data snapshot. `DEEN_SOURCES` overrides.
pub fn sources() -> PathBuf {
    if let Ok(p) = std::env::var("DEEN_SOURCES") {
        return PathBuf::from(p);
    }
    let base = std::env::var("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".local/share")
        });
    base.join("deen-data")
}

/// Ready when the corpus was joined by this deen from the snapshot on disk; a
/// corpus with no snapshot beside it to compare is kept.
pub fn status() -> Value {
    let dir = hadith::dir();
    let format = fs::read(dir.join("collections.json"))
        .ok()
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        .and_then(|m| m["source"]["format"].as_u64());
    let same_manifest = match fs::read(sources().join("manifest.toml")) {
        Ok(now) => fs::read(dir.join("manifest.toml")).is_ok_and(|built| built == now),
        Err(_) => true,
    };
    let state = if format == Some(FORMAT) && same_manifest { "ready" } else { "missing" };
    json!({ "state": state, "dir": dir.to_string_lossy() })
}

fn emit(v: Value) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{v}");
    let _ = out.flush();
}

/// Fetch, printing NDJSON events. On failure the error event is the last line
/// and the process exits 1.
pub fn fetch() {
    let mut progress = (0, 0);
    if let Err(e) = run(&mut progress) {
        emit(json!({ "event": "error", "message": format!("{e:#}"), "done": progress.0, "total": progress.1 }));
        eprintln!("deen: {e:#}");
        std::process::exit(1);
    }
}

/// A child that dies with this process. The App sends SIGTERM to deen alone
/// when it closes, and a join left behind would keep writing the corpus the
/// next fetch also writes. Audio and prayer curls use it too.
pub fn command(program: &str) -> Command {
    extern "C" {
        fn prctl(option: i32, arg: u64, ...) -> i32;
    }
    const PR_SET_PDEATHSIG: i32 = 1;
    const SIGTERM: u64 = 15;
    let mut cmd = Command::new(program);
    // SAFETY: prctl is async-signal-safe and touches only the child's own
    // process attributes.
    unsafe {
        cmd.pre_exec(|| {
            prctl(PR_SET_PDEATHSIG, SIGTERM);
            Ok(())
        });
    }
    cmd
}

/// `progress` is (done, total) in bytes, for the error event.
fn run(progress: &mut (u64, u64)) -> Result<()> {
    let src = sources();
    let editions = src.join("hadith");
    if !editions.join("ara-bukhari.min.json").is_file() {
        bail!("cannot read the hadith editions at {}: run dots update, or set DEEN_SOURCES", editions.display());
    }
    // A Bangla edition two collections lack counts nothing.
    let size = |c: &str| -> u64 {
        LANGS.iter().map(|l| fs::metadata(editions.join(format!("{l}-{c}.min.json"))).map_or(0, |m| m.len())).sum()
    };
    progress.1 = COLLECTIONS.iter().map(|c| size(c)).sum();
    let start = Instant::now();
    let tick = |done: u64, total: u64| {
        let secs = start.elapsed().as_secs_f64();
        let rate = if secs > 0.0 { (done as f64 / secs) as u64 } else { 0 };
        emit(json!({ "event": "progress", "done": done, "total": total, "rate": rate }));
    };
    tick(0, progress.1);

    let dir = hadith::dir();
    fs::create_dir_all(&dir).with_context(|| format!("cannot create {}", dir.display()))?;
    let mut py = command("python3")
        .arg("-I")
        .arg("-")
        .arg(&src)
        .arg(&dir)
        .arg(FORMAT.to_string())
        .arg(COLLECTIONS.join(" "))
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .context("cannot run python3")?;
    py.stdin.take().unwrap().write_all(JOIN.as_bytes())?;
    let mut last = String::new();
    for line in BufReader::new(py.stderr.take().unwrap()).lines() {
        let line = line?;
        if let Some(c) = COLLECTIONS.iter().find(|c| line.starts_with(&format!("fetch-hadith: {c}: "))) {
            progress.0 += size(c);
            tick(progress.0, progress.1);
        } else if !line.trim().is_empty() {
            last = line;
        }
    }
    if !py.wait()?.success() {
        bail!("building the corpus failed: {}", last.trim());
    }
    emit(json!({ "event": "done", "total": progress.1 }));
    Ok(())
}
