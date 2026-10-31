//! `deen hadith fetch` and `deen hadith status` on a scratch deen-data
//! snapshot.

use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
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

/// A snapshot as deen-data lays it out: its manifest, and Arabic and English
/// for all ten collections, Bangla for the eight that have it upstream.
/// Returns the editions' bytes.
fn snapshot(dir: &Path) -> u64 {
    fs::create_dir_all(dir.join("hadith")).unwrap();
    fs::write(dir.join("manifest.toml"), "# a snapshot\n").unwrap();
    fs::write(dir.join("hadith-books-bn.toml"), "[collections]\nbukhari = \"সহীহ বুখারী\"\n").unwrap();
    let mut total = 0;
    for c in COLLECTIONS {
        let en = if c == "bukhari" {
            "Narrated 'Umar bin Al-Khattab: I heard Allah's Messenger saying, \"The reward of deeds depends upon the intentions.\"".to_string()
        } else {
            format!("Narrated someone: a saying from {c}.")
        };
        let mut files = vec![("ara", edition(c, 1, &format!("حديث {c}"))), ("eng", edition(c, 1, &en))];
        if c != "qudsi" && c != "dehlawi" {
            files.push(("ben", edition(c, 1, "কাজের ফল নিয়তের উপর নির্ভরশীল")));
        }
        for (lang, v) in files {
            let body = serde_json::to_vec(&v).unwrap();
            total += body.len() as u64;
            fs::write(dir.join(format!("hadith/{lang}-{c}.min.json")), body).unwrap();
        }
    }
    total
}

fn deen(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_deen"))
        .args(args)
        .env("DEEN_HADITH", root.join("hadith"))
        .env("DEEN_SOURCES", root.join("deen-data"))
        .output()
        .unwrap()
}

fn events(out: &Output) -> Vec<Value> {
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|l| serde_json::from_str(l).unwrap_or_else(|e| panic!("not JSON: {l:?}: {e}")))
        .collect()
}

fn state(root: &Path) -> Value {
    let out = deen(root, &["hadith", "status"]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    serde_json::from_slice::<Value>(&out.stdout).unwrap()["state"].clone()
}

fn fetched(root: &Path) -> Vec<Value> {
    let out = deen(root, &["hadith", "fetch", "--json"]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    events(&out)
}

#[test]
fn fetch_joins_the_snapshot_a_collection_at_a_time_and_the_corpus_answers_a_search() {
    let root = scratch("full");
    let total = snapshot(&root.join("deen-data"));
    assert_eq!(state(&root), "missing");

    let ev = fetched(&root);
    assert_eq!(ev[0], json!({ "event": "progress", "done": 0, "total": total, "rate": 0 }));
    let progress: Vec<&Value> = ev.iter().filter(|e| e["event"] == "progress").collect();
    assert_eq!(progress.len(), 1 + COLLECTIONS.len());
    assert!(progress.windows(2).all(|w| w[0]["done"].as_u64() < w[1]["done"].as_u64()));
    assert_eq!(progress.last().unwrap()["done"], total);
    assert_eq!(ev.last().unwrap(), &json!({ "event": "done", "total": total }));
    assert_eq!(state(&root), "ready");

    let found: Value = serde_json::from_slice(&deen(&root, &["hadith", "search", "intentions"]).stdout).unwrap();
    assert_eq!(found["total"], 1);
    assert_eq!(found["hits"][0]["collection"], "bukhari");
    assert_eq!(found["hits"][0]["hadith"]["n"], "1");
    assert_eq!(found["hits"][0]["hadith"]["bn"], "কাজের ফল নিয়তের উপর নির্ভরশীল");
}

#[test]
fn a_missing_bangla_edition_is_not_an_error() {
    let root = scratch("noben");
    let ben = root.join("deen-data/hadith/ben-bukhari.min.json");
    let total = snapshot(&root.join("deen-data")) - fs::metadata(&ben).unwrap().len();
    fs::remove_file(&ben).unwrap();

    assert_eq!(fetched(&root).last().unwrap(), &json!({ "event": "done", "total": total }));
    let shown: Value = serde_json::from_slice(&deen(&root, &["hadith", "show", "bukhari:1"]).stdout).unwrap();
    assert!(shown["hit"]["hadith"]["en"].as_str().unwrap().contains("intentions"));
    assert!(shown["hit"]["hadith"].get("bn").is_none());
}

#[test]
fn no_snapshot_is_said_plainly() {
    let root = scratch("nosnapshot");
    let out = deen(&root, &["hadith", "fetch", "--json"]);
    assert_eq!(out.status.code(), Some(1));
    let error = events(&out).pop().unwrap();
    assert_eq!(error["event"], "error");
    let message = error["message"].as_str().unwrap();
    assert!(message.starts_with("cannot read the hadith editions at "), "{message}");
    assert_eq!(state(&root), "missing");
}

#[test]
fn a_corpus_from_another_snapshot_or_an_older_deen_reads_missing() {
    let root = scratch("stale");
    snapshot(&root.join("deen-data"));
    fetched(&root);
    assert_eq!(state(&root), "ready");

    let manifest = root.join("deen-data/manifest.toml");
    fs::write(&manifest, "# the next snapshot\n").unwrap();
    assert_eq!(state(&root), "missing");
    fetched(&root);
    assert_eq!(state(&root), "ready");

    let collections = root.join("hadith/collections.json");
    let mut m: Value = serde_json::from_slice(&fs::read(&collections).unwrap()).unwrap();
    m["source"]["format"] = json!(1);
    fs::write(&collections, serde_json::to_vec(&m).unwrap()).unwrap();
    assert_eq!(state(&root), "missing");

    // With no snapshot on this machine to compare against, a current corpus is kept.
    fetched(&root);
    fs::remove_dir_all(root.join("deen-data")).unwrap();
    assert_eq!(state(&root), "ready");
}
