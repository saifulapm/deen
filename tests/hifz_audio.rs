use serde_json::Value;
use std::process::{Command, Output};

fn fixture(name: &str) -> String {
    format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"))
}

fn deen(quran: &str, env: &[(&str, &str)], args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_deen"))
        .args(args)
        .env("DEEN_DATA", fixture(quran))
        .envs(env.iter().copied())
        .output()
        .unwrap()
}

fn json(o: Output) -> Value {
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    serde_json::from_slice(&o.stdout).unwrap()
}

fn tmp(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("deen-t2-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn due_reports_next_per_grade_and_counts_new_cards() {
    let d = tmp("due");
    let state = d.join("hifz.json");
    let env = [("DEEN_STATE", state.to_str().unwrap())];
    json(deen("quran.json", &env, &["hifz", "add", "1"]));
    // Learn one card so only two stay at reps 0.
    json(deen("quran.json", &env, &["hifz", "grade", "1:1", "good"]));
    let v = json(deen("quran.json", &env, &["hifz", "due"]));
    // 1:1 is now a day away; 1:2 and 1:3 are new and due.
    assert_eq!(v["total_due"], 2);
    assert_eq!(v["new_due"], 2);
    let next = &v["due"][0]["next"];
    assert_eq!(next["again"], 600);
    assert_eq!(next["good"], 86400);
    assert_eq!(next["hard"], 86400);
    assert_eq!(next["easy"], 86400);
}

#[test]
fn new_due_ignores_learned_cards_that_are_due() {
    let d = tmp("newdue");
    let state = d.join("hifz.json");
    // An old deck, with last_accuracy, one learned card overdue and one new.
    std::fs::write(
        &state,
        r#"{"cards":{"1:1":{"ease":2.5,"reps":2,"interval":6.0,"due":1,"lapses":0,"last_accuracy":0.9},
"1:2":{"ease":2.5,"reps":0,"interval":0.0,"due":1,"lapses":0}}}"#,
    )
    .unwrap();
    let env = [("DEEN_STATE", state.to_str().unwrap())];
    let v = json(deen("quran.json", &env, &["hifz", "due"]));
    assert_eq!(v["total_due"], 2);
    assert_eq!(v["new_due"], 1);
}

#[test]
fn grade_again_is_due_in_ten_minutes() {
    let d = tmp("again");
    let state = d.join("hifz.json");
    let env = [("DEEN_STATE", state.to_str().unwrap())];
    json(deen("quran.json", &env, &["hifz", "add", "1:1"]));
    let v = json(deen("quran.json", &env, &["hifz", "grade", "1:1", "again"]));
    let due_in = v["due_in_days"].as_f64().unwrap();
    assert!((due_in - 600.0 / 86400.0).abs() < 1e-3, "{due_in}");
}

#[test]
fn audio_to_end_lists_urls_then_prefers_cached_paths() {
    let cache = tmp("audio");
    let env = [("XDG_CACHE_HOME", cache.to_str().unwrap())];
    let v = json(deen("quran7.json", &env, &["audio", "1:5", "--to-end"]));
    let q = v["queue"].as_array().unwrap();
    assert_eq!(q.len(), 3);
    assert_eq!(q[0], "https://everyayah.com/data/Husary_128kbps/001005.mp3");
    assert_eq!(q[2], "https://everyayah.com/data/Husary_128kbps/001007.mp3");

    let dir = cache.join("deen/audio/ayah/Husary_128kbps");
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("001006.mp3");
    std::fs::write(&file, b"mp3").unwrap();
    let v = json(deen("quran7.json", &env, &["audio", "1:5", "--to-end"]));
    let q = v["queue"].as_array().unwrap();
    assert_eq!(q.len(), 3);
    assert!(q[0].as_str().unwrap().starts_with("https://"));
    assert_eq!(q[1], file.to_str().unwrap());
}

#[test]
fn recite_is_an_unknown_subcommand() {
    let o = deen("quran.json", &[], &["recite", "1:1"]);
    assert!(!o.status.success());
    assert!(String::from_utf8_lossy(&o.stderr).contains("unrecognized subcommand"));
}
