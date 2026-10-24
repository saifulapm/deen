//! Download the hadith editions and build the corpus from them.
//!
//! The 30 editions (about 124 MB) land in `<hadith dir>/.download`, each
//! through a `.part` file that curl appends to, so a fetch that is killed at
//! any point leaves bytes on disk that the next fetch continues from with an
//! HTTP Range request. Once every edition is down, `scripts/join-hadith` (the
//! same python `scripts/fetch-hadith` runs) writes the jsonl corpus beside it,
//! collections.json last.

use std::{
    collections::{BTreeMap, VecDeque},
    fs,
    io::Write,
    os::unix::process::CommandExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};

use crate::hadith;

/// Pinned to a commit: the file names are positions in someone else's repo
/// and the numbering is the join key for three languages.
const SHA: &str = "df57907be35291c91ad6a6691180e22ca9920784";
const COLLECTIONS: [&str; 10] = [
    "bukhari", "muslim", "abudawud", "tirmidhi", "nasai", "ibnmajah", "malik", "nawawi", "qudsi",
    "dehlawi",
];
const LANGS: [&str; 3] = ["ara", "eng", "ben"];
const JOIN: &str = include_str!("../scripts/join-hadith");

fn base_url() -> String {
    std::env::var("DEEN_HADITH_URL").unwrap_or_else(|_| {
        format!("https://cdn.jsdelivr.net/gh/fawazahmed0/hadith-api@{SHA}/editions")
    })
}

fn names() -> Vec<String> {
    COLLECTIONS
        .iter()
        .flat_map(|c| LANGS.iter().map(move |l| format!("{l}-{c}.min.json")))
        .collect()
}

/// Edition name to its size in bytes; `None` for a Bangla edition upstream
/// does not have.
type Sizes = BTreeMap<String, Option<u64>>;

fn downloads() -> PathBuf {
    hadith::dir().join(".download")
}

fn len(p: &Path) -> u64 {
    fs::metadata(p).map(|m| m.len()).unwrap_or(0)
}

fn part(p: &Path) -> PathBuf {
    let mut s = p.as_os_str().to_owned();
    s.push(".part");
    PathBuf::from(s)
}

/// Bytes of edition data on disk, finished or not. sizes.json is not counted.
fn saved() -> u64 {
    let dl = downloads();
    names()
        .iter()
        .map(|n| {
            let f = dl.join(n);
            len(&f) + len(&part(&f))
        })
        .sum()
}

fn known_total() -> u64 {
    fs::read(downloads().join("sizes.json"))
        .ok()
        .and_then(|b| serde_json::from_slice::<Sizes>(&b).ok())
        .map(|s| s.values().flatten().sum())
        .unwrap_or(0)
}

pub fn status() -> Value {
    let dir = hadith::dir();
    let done = saved();
    let state = if dir.join("collections.json").is_file() {
        "ready"
    } else if done > 0 {
        "partial"
    } else {
        "missing"
    };
    json!({ "state": state, "done": done, "total": known_total(), "dir": dir.to_string_lossy() })
}

fn emit(v: Value) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{v}");
    let _ = out.flush();
}

/// Fetch, printing NDJSON events. On failure the error event is the last line
/// and the process exits 1, keeping everything already saved.
pub fn fetch() {
    if let Err(e) = run() {
        emit(json!({ "event": "error", "message": format!("{e:#}"), "done": saved(), "total": known_total() }));
        eprintln!("deen: {e:#}");
        std::process::exit(1);
    }
}

/// A child that dies with this process. The App sends SIGTERM to deen alone
/// when it closes, and a curl left behind would keep appending to a .part the
/// next fetch is also appending to.
fn command(program: &str) -> Command {
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

/// HEAD every edition at once. A missing Arabic or English edition is fatal,
/// as in scripts/fetch-hadith; a missing Bangla one is recorded as absent.
fn sizes(base: &str) -> Result<Sizes> {
    let heads: Vec<_> = names()
        .into_iter()
        .map(|n| {
            let child = command("curl")
                .args(["-sSIL", "-o", "/dev/null", "-H", "Accept-Encoding: identity"])
                .args(["-w", "%{http_code} %header{content-length}"])
                .arg(format!("{base}/{n}"))
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .context("cannot run curl");
            (n, child)
        })
        .collect();
    let mut sizes = Sizes::new();
    for (n, child) in heads {
        let out = child?.wait_with_output()?;
        let text = String::from_utf8_lossy(&out.stdout);
        let mut f = text.split_whitespace();
        let size = match (f.next(), f.next()) {
            (Some("200"), Some(l)) => Some(l.parse::<u64>().with_context(|| format!("{n}: bad Content-Length {l:?}"))?),
            // A jsDelivr edge answers HEAD without Content-Length on a cold
            // cache miss; the total in a one-byte Range answer is the size.
            (Some("200"), None) => Some(ranged_size(base, &n)?),
            (Some("404"), _) if n.starts_with("ben-") => None,
            _ => bail!(
                "cannot reach {n}: {}{}",
                text.trim(),
                String::from_utf8_lossy(&out.stderr).trim()
            ),
        };
        sizes.insert(n, size);
    }
    Ok(sizes)
}

fn ranged_size(base: &str, n: &str) -> Result<u64> {
    let out = command("curl")
        .args(["-sSL", "-r", "0-0", "-o", "/dev/null", "-w", "%{http_code} %header{content-range}"])
        .arg(format!("{base}/{n}"))
        .output()
        .context("cannot run curl")?;
    let text = String::from_utf8_lossy(&out.stdout);
    text.strip_prefix("206 bytes 0-0/")
        .and_then(|t| t.trim().parse().ok())
        .with_context(|| format!("cannot learn the size of {n}: {}", text.trim()))
}

struct Progress {
    total: u64,
    window: VecDeque<(Instant, u64)>,
}

impl Progress {
    fn tick(&mut self) {
        let now = Instant::now();
        let done = saved();
        self.window.push_back((now, done));
        while self.window.len() > 1 && now - self.window[0].0 > Duration::from_secs(3) {
            self.window.pop_front();
        }
        let (t0, d0) = self.window[0];
        let secs = (now - t0).as_secs_f64();
        let rate = if secs > 0.0 { (done.saturating_sub(d0) as f64 / secs) as u64 } else { 0 };
        emit(json!({ "event": "progress", "done": done, "total": self.total, "rate": rate }));
    }
}

fn run() -> Result<()> {
    let dir = hadith::dir();
    let dl = downloads();
    fs::create_dir_all(&dl).with_context(|| format!("cannot create {}", dl.display()))?;

    let base = base_url();
    let sizes = sizes(&base)?;
    fs::write(dl.join("sizes.json"), serde_json::to_vec(&sizes)?)?;
    let total = sizes.values().flatten().sum();
    let mut progress = Progress { total, window: VecDeque::new() };
    progress.tick();

    // In fetch-hadith's order, bukhari first, not sizes.json's sorted order.
    for n in &names() {
        let Some(size) = sizes[n] else { continue };
        let dest = dl.join(n);
        let tmp = part(&dest);
        if len(&dest) == size {
            continue;
        }
        if len(&tmp) > size {
            fs::remove_file(&tmp)?;
        }
        if len(&tmp) < size {
            // -C - asks for the bytes past what the .part already holds.
            let mut child = command("curl")
                .args(["-fsSL", "--retry", "2", "--retry-delay", "1", "-C", "-", "-o"])
                .arg(&tmp)
                .arg(format!("{base}/{n}"))
                .stderr(Stdio::piped())
                .spawn()
                .context("cannot run curl")?;
            let status = loop {
                if let Some(s) = child.try_wait()? {
                    break s;
                }
                // Wait in short steps so a small file does not cost 250 ms.
                thread::sleep(Duration::from_millis(25));
                if progress.window.back().is_some_and(|w| w.0.elapsed() >= Duration::from_millis(250)) {
                    progress.tick();
                }
            };
            if !status.success() {
                let err = child.wait_with_output()?.stderr;
                bail!("could not fetch {n}: {}", String::from_utf8_lossy(&err).trim());
            }
        }
        if len(&tmp) != size {
            bail!("{n} came to {} bytes, expected {size}", len(&tmp));
        }
        fs::rename(&tmp, &dest)?;
        progress.tick();
    }

    emit(json!({ "event": "converting" }));
    let mut py = command("python3")
        .arg("-I")
        .arg("-")
        .arg(&dl)
        .arg(&dir)
        .arg(SHA)
        .arg(COLLECTIONS.join(" "))
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .context("cannot run python3")?;
    py.stdin.take().unwrap().write_all(JOIN.as_bytes())?;
    let out = py.wait_with_output()?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        bail!("building the corpus failed: {}", err.trim().lines().last().unwrap_or(""));
    }
    fs::remove_dir_all(&dl)?;
    emit(json!({ "event": "done", "total": total }));
    Ok(())
}
