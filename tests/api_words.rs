use serde_json::Value;
use std::process::Command;

fn fixture(name: &str) -> String {
    format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"))
}

fn run(words: &str, args: &[&str]) -> Value {
    let out = Command::new(env!("CARGO_BIN_EXE_deen"))
        .args(args)
        .env("DEEN_DATA", fixture("quran.json"))
        .env("DEEN_WORDS", words)
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    serde_json::from_slice(&out.stdout).unwrap()
}

#[test]
fn words_json_adds_meaning_transliteration_root_and_juz() {
    let v = run(&fixture("words.json"), &["api", "surah", "1"]);
    let w = &v["words"][0][0];
    assert_eq!(w["en"], "In (the) name");
    assert_eq!(w["tr"], "bis'mi");
    assert_eq!(w["root"], "س م و");
    assert_eq!(v["ayahs"][0]["juz"], 1);
    // v1's tokens are kept when the counts agree.
    assert_eq!(w["text"], "بِسْمِ");
    assert_eq!(w["audio"], "001_001_001");
    assert_eq!(w["segments"][0]["t"], "بِسْمِ");
}

#[test]
fn a_differing_count_takes_words_json_words() {
    let v = run(&fixture("words.json"), &["api", "surah", "1"]);
    let ws = v["words"][1].as_array().unwrap();
    assert_eq!(ws.len(), 2);
    assert_eq!(ws[1]["i"], 2);
    assert_eq!(ws[1]["text"], "باء");
    assert_eq!(ws[1]["audio"], "001_002_002");
    assert_eq!(ws[1]["segments"][0]["t"], "باء");
    assert_eq!(ws[1]["root"], "ب");
    assert!(ws[0]["root"].is_null());
}

#[test]
fn an_ayah_without_roots_keeps_null_roots() {
    let v = run(&fixture("words.json"), &["api", "ayah", "1:3"]);
    let ws = v["words"].as_array().unwrap();
    assert_eq!(ws.len(), 2);
    assert!(ws.iter().all(|w| w["root"].is_null()));
    assert_eq!(ws[0]["en"], "the Most Gracious");
    assert_eq!(v["ayah"]["juz"], 1);
}

#[test]
fn no_words_json_gives_nulls() {
    let v = run("/nonexistent/words.json", &["api", "surah", "1"]);
    let w = &v["words"][0][0];
    assert!(w["en"].is_null() && w["tr"].is_null() && w["root"].is_null());
    assert!(v["ayahs"][0]["juz"].is_null());
    assert_eq!(w["text"], "بِسْمِ");
    assert_eq!(v["words"][1].as_array().unwrap().len(), 4);
}
