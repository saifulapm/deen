//! The gate on the pronunciation: every ayah in deen-data's golden-pron.toml,
//! as Saiful corrected it, must be what the engine writes, exactly. The data
//! is built from the deen-data checkout (DEEN_SOURCES, else ../deen-data), as
//! the install builds it.

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use serde_json::Value;

fn checkout() -> PathBuf {
    let dir = std::env::var_os("DEEN_SOURCES").map_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../deen-data"), PathBuf::from);
    assert!(dir.join("golden-pron.toml").exists(), "no golden-pron.toml in a deen-data checkout at {}: set DEEN_SOURCES", dir.display());
    dir
}

/// golden-pron.toml's `"s:a" = "…"` lines; comments and the table header
/// are skipped. A line of any other shape fails, so a typo is not a skip.
fn golden(text: &str) -> Vec<(String, String)> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#') && *l != "[ayat]")
        .map(|l| {
            let (at, pron) = l.split_once(" = ").unwrap_or_else(|| panic!("golden-pron.toml: cannot read {l:?}"));
            let unquote = |s: &str| s.trim().strip_prefix('"').and_then(|s| s.strip_suffix('"')).unwrap_or_else(|| panic!("golden-pron.toml: {s:?} is not quoted")).to_string();
            (unquote(at), unquote(pron))
        })
        .collect()
}

/// য় ড় ঢ় typed as one code point or as two compare alike.
fn same_form(s: &str) -> String {
    s.replace('\u{09DF}', "\u{09AF}\u{09BC}").replace('\u{09DC}', "\u{09A1}\u{09BC}").replace('\u{09DD}', "\u{09A2}\u{09BC}")
}

#[test]
fn golden_ayat_read_as_corrected() {
    let src = checkout();
    let out = Path::new(env!("CARGO_TARGET_TMPDIR")).join("golden");
    let _ = fs::remove_dir_all(&out);
    let r = Command::new(concat!(env!("CARGO_MANIFEST_DIR"), "/scripts/build-data"))
        .env("DEEN_BIN", env!("CARGO_BIN_EXE_deen"))
        .arg(&src)
        .arg(&out)
        .output()
        .unwrap();
    assert!(r.status.success(), "{}", String::from_utf8_lossy(&r.stderr));
    let quran: Value = serde_json::from_str(&fs::read_to_string(out.join("quran.json")).unwrap()).unwrap();
    let pron = |at: &str| {
        let (s, a) = at.split_once(':').unwrap();
        quran["ayahs"]
            .as_array()
            .unwrap()
            .iter()
            .find(|x| x["s"].to_string() == s && x["a"].to_string() == a)
            .unwrap_or_else(|| panic!("golden-pron.toml names {at}, which is no ayah"))["bn_pron"]
            .as_str()
            .unwrap()
            .to_string()
    };
    let ayat = golden(&fs::read_to_string(src.join("golden-pron.toml")).unwrap());
    assert!(!ayat.is_empty(), "golden-pron.toml holds no ayah");
    let wrong: Vec<String> = ayat
        .iter()
        .filter(|(at, want)| same_form(&pron(at)) != same_form(want))
        .map(|(at, want)| format!("{at}\n  golden: {want}\n  engine: {}", pron(at)))
        .collect();
    println!("golden: {} of {} ayat match", ayat.len() - wrong.len(), ayat.len());
    assert!(wrong.is_empty(), "{} of {} golden ayat differ:\n{}", wrong.len(), ayat.len(), wrong.join("\n"));
}
