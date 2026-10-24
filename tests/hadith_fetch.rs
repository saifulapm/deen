//! `deen hadith fetch` and `deen hadith status` against a local stand-in for
//! the editions CDN (examples/fake_editions.rs).

use std::{
    fs,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::{Child, Command, Output, Stdio},
};

use serde_json::{json, Value};

const COLLECTIONS: [&str; 10] = [
    "bukhari", "muslim", "abudawud", "tirmidhi", "nasai", "ibnmajah", "malik", "nawawi", "qudsi",
    "dehlawi",
];

fn scratch(name: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("hadith-fetch-{name}"));
    let _ = fs::remove_dir_all(&p);
    fs::create_dir_all(&p).unwrap();
    p
}

fn edition(c: &str, n: u32, text: &str) -> Value {
    json!({
        "metadata": { "name": format!("Collection {c}"), "sections": { "1": "Revelation" } },
        "hadiths": [{
            "hadithnumber": n,
            "text": text,
            "grades": [],
            "reference": { "book": 1, "hadith": n },
        }],
    })
}

/// Arabic and English for all ten collections, Bangla for the eight that have
/// it upstream. Bukhari's Arabic is padded so it is the biggest file and the
/// first one fetched, which puts a cut at 30% inside it.
fn editions(dir: &Path) -> u64 {
    let mut total = 0;
    for c in COLLECTIONS {
        let ar = if c == "bukhari" {
            "إنما الأعمال بالنيات ".repeat(300)
        } else {
            format!("حديث {c}")
        };
        let en = if c == "bukhari" {
            "Narrated 'Umar bin Al-Khattab: I heard Allah's Messenger saying, \"The reward of deeds depends upon the intentions.\"".to_string()
        } else {
            format!("Narrated someone: a saying from {c}.")
        };
        let mut files = vec![("ara", edition(c, 1, &ar)), ("eng", edition(c, 1, &en))];
        if c != "qudsi" && c != "dehlawi" {
            files.push(("ben", edition(c, 1, "কাজের ফল নিয়তের উপর নির্ভরশীল")));
        }
        for (lang, v) in files {
            let body = serde_json::to_vec(&v).unwrap();
            total += body.len() as u64;
            fs::write(dir.join(format!("{lang}-{c}.min.json")), body).unwrap();
        }
    }
    total
}

struct Fake {
    child: Child,
    url: String,
    log: PathBuf,
}

impl Fake {
    fn start(dir: &Path, log: &Path, env: &[(&str, String)]) -> Fake {
        let bin = Path::new(env!("CARGO_BIN_EXE_deen"))
            .parent()
            .unwrap()
            .join("examples/fake_editions");
        let mut child = Command::new(bin)
            .arg(dir)
            .env("FAKE_LOG", log)
            .envs(env.iter().cloned())
            .stdout(Stdio::piped())
            .spawn().expect("cannot start the fake server, is the example built?");
        let mut addr = String::new();
        BufReader::new(child.stdout.as_mut().unwrap())
            .read_line(&mut addr)
            .unwrap();
        Fake {
            child,
            url: format!("http://{}", addr.trim()),
            log: log.to_path_buf(),
        }
    }

    /// Every logged request, as (method, path, range, body bytes).
    fn requests(&self) -> Vec<(String, String, String, u64)> {
        requests(&self.log)
    }
}

fn requests(log: &Path) -> Vec<(String, String, String, u64)> {
    fs::read_to_string(log)
        .unwrap_or_default()
        .lines()
        .map(|l| {
            let f: Vec<&str> = l.split(' ').collect();
            (f[0].into(), f[1].into(), f[2].into(), f[3].parse().unwrap())
        })
        .collect()
}

impl Drop for Fake {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn deen(hadith: &Path, url: &str, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_deen"))
        .args(args)
        .env("DEEN_HADITH", hadith)
        .env("DEEN_HADITH_URL", url)
        .output()
        .unwrap()
}

fn events(out: &Output) -> Vec<Value> {
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|l| serde_json::from_str(l).unwrap_or_else(|e| panic!("not JSON: {l:?}: {e}")))
        .collect()
}

fn status(hadith: &Path) -> Value {
    let out = deen(hadith, "http://127.0.0.1:9", &["hadith", "status"]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    serde_json::from_slice(&out.stdout).unwrap()
}

#[test]
fn fetch_downloads_converts_and_the_corpus_answers_a_search() {
    let root = scratch("full");
    let ed = root.join("editions");
    fs::create_dir_all(&ed).unwrap();
    let total = editions(&ed);
    let hadith = root.join("hadith");
    let fake = Fake::start(&ed, &root.join("log"), &[]);

    assert_eq!(status(&hadith)["state"], "missing");

    let out = deen(&hadith, &fake.url, &["hadith", "fetch", "--json"]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let ev = events(&out);
    let kinds: Vec<&str> = ev.iter().map(|e| e["event"].as_str().unwrap()).collect();
    assert_eq!(kinds.first(), Some(&"progress"));
    assert_eq!(&kinds[kinds.len() - 2..], ["converting", "done"]);
    let last_progress = ev.iter().rev().find(|e| e["event"] == "progress").unwrap();
    assert_eq!(last_progress["done"], total);
    assert_eq!(last_progress["total"], total);
    assert_eq!(ev.last().unwrap()["total"], total);

    assert!(!hadith.join(".download").exists());
    assert_eq!(status(&hadith)["state"], "ready");

    let out = deen(&hadith, &fake.url, &["hadith", "search", "intentions"]);
    let found: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(found["total"], 1);
    assert_eq!(found["hits"][0]["collection"], "bukhari");
    assert_eq!(found["hits"][0]["hadith"]["n"], "1");
    assert_eq!(found["hits"][0]["hadith"]["bn"], "কাজের ফল নিয়তের উপর নির্ভরশীল");
}

#[test]
fn a_cut_download_reads_partial_and_the_next_fetch_resumes_it() {
    let root = scratch("resume");
    let ed = root.join("editions");
    fs::create_dir_all(&ed).unwrap();
    let total = editions(&ed);
    let hadith = root.join("hadith");
    let cut = total * 3 / 10;

    let first = Fake::start(&ed, &root.join("log1"), &[("FAKE_CUT", cut.to_string())]);
    let out = deen(&hadith, &first.url, &["hadith", "fetch", "--json"]);
    assert_eq!(out.status.code(), Some(1));
    let ev = events(&out);
    let err = ev.last().unwrap();
    assert_eq!(err["event"], "error");
    assert!(err["message"].as_str().is_some_and(|m| !m.is_empty()));
    assert_eq!(err["done"], cut);
    assert_eq!(err["total"], total);
    drop(first);

    let st = status(&hadith);
    assert_eq!(st["state"], "partial");
    assert_eq!(st["done"], cut);
    assert_eq!(st["total"], total);
    let part = hadith.join(".download/ara-bukhari.min.json.part");
    assert_eq!(fs::metadata(&part).unwrap().len(), cut);

    let second = Fake::start(&ed, &root.join("log2"), &[]);
    let out = deen(&hadith, &second.url, &["hadith", "fetch", "--json"]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(events(&out).last().unwrap()["event"], "done");

    let reqs = second.requests();
    assert!(
        reqs.iter()
            .any(|r| r.0 == "GET" && r.1 == "/ara-bukhari.min.json" && r.2 == format!("bytes={cut}-")),
        "no Range request for the partial file: {reqs:?}"
    );
    let served: u64 = requests(&root.join("log1"))
        .iter()
        .chain(reqs.iter())
        .map(|r| r.3)
        .sum();
    assert_eq!(served, total);
    assert_eq!(status(&hadith)["state"], "ready");
}

#[test]
fn a_missing_bangla_edition_is_not_an_error() {
    let root = scratch("noben");
    let ed = root.join("editions");
    fs::create_dir_all(&ed).unwrap();
    let ben = ed.join("ben-bukhari.min.json");
    let total = editions(&ed) - fs::metadata(&ben).unwrap().len();
    fs::remove_file(&ben).unwrap();
    let hadith = root.join("hadith");
    let fake = Fake::start(&ed, &root.join("log"), &[]);

    let out = deen(&hadith, &fake.url, &["hadith", "fetch", "--json"]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(events(&out).last().unwrap(), &json!({ "event": "done", "total": total }));

    let out = deen(&hadith, &fake.url, &["hadith", "show", "bukhari:1"]);
    let shown: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(shown["hit"]["hadith"]["en"].as_str().unwrap().contains("intentions"));
    assert!(shown["hit"]["hadith"].get("bn").is_none());
}

/// Pids whose parent is `ppid`, from /proc.
fn children_of(ppid: u32) -> Vec<u32> {
    fs::read_dir("/proc")
        .unwrap()
        .flatten()
        .filter_map(|e| e.file_name().to_str()?.parse::<u32>().ok())
        .filter(|pid| {
            // The fields after the parenthesised name: state, then ppid.
            fs::read_to_string(format!("/proc/{pid}/stat"))
                .ok()
                .and_then(|s| s.rsplit_once(") ").map(|(_, r)| r.to_string()))
                .and_then(|r| r.split(' ').nth(1).and_then(|p| p.parse::<u32>().ok()))
                == Some(ppid)
        })
        .collect()
}

/// Kill a fetch mid-download with `signal`: its curl must not outlive it and
/// the bytes it saved must stay for the resume.
fn killing_the_fetch_ends_its_curl(name: &str, signal: &str) {
    let root = scratch(name);
    let ed = root.join("editions");
    fs::create_dir_all(&ed).unwrap();
    editions(&ed);
    let hadith = root.join("hadith");
    let fake = Fake::start(&ed, &root.join("log"), &[("FAKE_SLOW_MS", "100".into())]);

    let mut fetch = Command::new(env!("CARGO_BIN_EXE_deen"))
        .args(["hadith", "fetch", "--json"])
        .env("DEEN_HADITH", &hadith)
        .env("DEEN_HADITH_URL", &fake.url)
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    let part = hadith.join(".download/ara-bukhari.min.json.part");
    let start = std::time::Instant::now();
    while fs::metadata(&part).map(|m| m.len()).unwrap_or(0) == 0 {
        assert!(start.elapsed().as_secs() < 10, "no bytes arrived");
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let kids = children_of(fetch.id());
    assert!(!kids.is_empty(), "the fetch has no curl child to check");

    let killed = Command::new("kill")
        .args([signal, &fetch.id().to_string()])
        .status()
        .unwrap();
    assert!(killed.success());
    fetch.wait().unwrap();

    let start = std::time::Instant::now();
    while kids.iter().any(|k| Path::new(&format!("/proc/{k}")).exists()) {
        assert!(start.elapsed().as_secs() < 2, "a child of the fetch outlived it: {kids:?}");
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(fs::metadata(&part).unwrap().len() > 0);
}

#[test]
fn sigterm_on_the_fetch_ends_its_curl_and_keeps_the_bytes() {
    killing_the_fetch_ends_its_curl("sigterm", "-TERM");
}

#[test]
fn sigkill_on_the_fetch_ends_its_curl_and_keeps_the_bytes() {
    killing_the_fetch_ends_its_curl("sigkill", "-KILL");
}

#[test]
fn a_head_without_content_length_still_gets_the_size() {
    let root = scratch("nolength");
    let ed = root.join("editions");
    fs::create_dir_all(&ed).unwrap();
    let total = editions(&ed);
    let hadith = root.join("hadith");
    let fake = Fake::start(&ed, &root.join("log"), &[("FAKE_HEAD_NO_LENGTH", "1".into())]);

    let out = deen(&hadith, &fake.url, &["hadith", "fetch", "--json"]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(events(&out).last().unwrap(), &json!({ "event": "done", "total": total }));
}

// curl's own code for no connection is 000, which read as "000curl: (7) ..." glued to its error (#3JX22QCJ).
#[test]
fn an_unreachable_cdn_is_said_plainly() {
    let dir = scratch("unreachable");
    let out = deen(&dir.join("hadith"), "http://127.0.0.1:9", &["hadith", "fetch", "--json"]);
    assert!(!out.status.success());
    let error = events(&out).into_iter().find(|e| e["event"] == "error").expect("an error event");
    let message = error["message"].as_str().unwrap();
    assert!(message.starts_with("cannot reach ara-bukhari.min.json: Failed to connect"), "{message}");
    assert!(!message.contains("000"), "{message}");
}
