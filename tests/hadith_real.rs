//! The corpus joined from the deen-data checkout's real editions (DEEN_SOURCES,
//! else ../deen-data), held to the narrations Deen is known to show.

use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::OnceLock,
};

use serde_json::Value;

const COLLECTIONS: &str = "bukhari muslim abudawud tirmidhi nasai ibnmajah malik nawawi qudsi dehlawi";

/// Joined once for every test here; it takes a few seconds.
fn corpus() -> &'static Path {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| {
        let sources = std::env::var_os("DEEN_SOURCES").map_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../deen-data"), PathBuf::from);
        assert!(sources.join("hadith/ben-bukhari.min.json").exists(), "no hadith editions in a deen-data checkout at {}: set DEEN_SOURCES", sources.display());
        let out = Path::new(env!("CARGO_TARGET_TMPDIR")).join("hadith-real");
        let _ = std::fs::remove_dir_all(&out);
        std::fs::create_dir_all(&out).unwrap();
        let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/join-hadith");
        let run = Command::new("python3").arg("-I").arg(script).arg(&sources).arg(&out).arg("2").arg(COLLECTIONS).output().unwrap();
        assert!(run.status.success(), "{}", String::from_utf8_lossy(&run.stderr));
        out
    })
}

fn deen(args: &[&str]) -> Value {
    let run = Command::new(env!("CARGO_BIN_EXE_deen")).args(args).env("DEEN_HADITH", corpus()).output().unwrap();
    assert!(run.status.success(), "{}", String::from_utf8_lossy(&run.stderr));
    serde_json::from_slice(&run.stdout).unwrap()
}

/// The editions type য় and ড় as one code point, which an editor's NFC splits,
/// so both sides are compared split.
fn split(s: &str) -> String {
    s.replace('\u{9DF}', "\u{9AF}\u{9BC}").replace('\u{9DC}', "\u{9A1}\u{9BC}").replace('\u{9DD}', "\u{9A2}\u{9BC}")
}

fn bn(reference: &str) -> String {
    split(deen(&["hadith", "show", reference])["hit"]["hadith"]["bn"].as_str().unwrap())
}

#[test]
fn bukhari_6114_is_its_narration_alone() {
    assert_eq!(
        bn("bukhari:6114"),
        split("আবূ হুরাইরাহ হতে বর্ণিত। তিনি বলেন, রাসূলুল্লাহ সাল্লাল্লাহু আলাইহি ওয়াসাল্লাম বলেছেনঃ প্রকৃত বীর সে নয়, যে কাউকে কুস্তিতে হারিয়ে দেয়। বরং সেই আসল বীর, যে রাগের সময় নিজেকে নিয়ন্ত্রণে রাখতে পারে।")
    );
}

#[test]
fn a_heading_full_of_quran_quotes_goes() {
    // bukhari:1426's raw opens with 2,291 characters of heading.
    let text = bn("bukhari:1426");
    assert!(text.starts_with(&split("আবূ হুরাইরাহ্ (রাঃ)-এর সূত্রে")), "{text}");
    assert!(!text.chars().any(|c| ('\u{0600}'..='\u{06FF}').contains(&c)), "{text}");
}

#[test]
fn muslims_chain_stub_and_publisher_go() {
    let text = bn("muslim:6602");
    assert!(text.starts_with(&split("নবী সাল্লাল্লাহু আলাইহি ওয়াসাল্লাম এর সহধর্মিণী আয়িশাহ")), "{text}");
    assert!(!text.contains("(ইসলামিক ফাউন্ডেশন"), "{text}");
}

#[test]
fn a_trailing_chain_note_goes() {
    let text = bn("bukhari:6412");
    assert!(!text.contains("....."), "{text}");
    assert!(text.ends_with(&split("সুস্থতা আর অবসর।")), "{text}");
}

#[test]
fn a_grade_remark_stays() {
    assert!(bn("tirmidhi:2000").contains(&split("হাসান গারীব")));
}

#[test]
fn the_raw_text_is_kept() {
    let h = &deen(&["hadith", "show", "bukhari:6114"])["hit"]["hadith"];
    assert!(split(h["bn_raw"].as_str().unwrap()).contains(&split("(আধুনিক প্রকাশনী- ৫৬৭৪, ইসলামিক ফাউন্ডেশন)")));
}

#[test]
fn bukhari_has_97_named_books_then_the_rest() {
    let books = deen(&["hadith", "books", "bukhari"]);
    let books = books["books"].as_array().unwrap();
    assert_eq!(books.len(), 98);
    assert_eq!(books[0], serde_json::json!({ "n": 1, "bn": "ওহীর সূচনা", "en": "Revelation", "count": 7 }));
    // 311 sit in book 0, 4 of them empty in every language.
    assert_eq!(books[97], serde_json::json!({ "n": 0, "bn": "অন্যান্য", "en": "", "count": 307 }));
}

#[test]
fn nasais_book_35_is_oaths_and_vows() {
    let books = deen(&["hadith", "books", "nasai"]);
    let b35 = books["books"].as_array().unwrap().iter().find(|b| b["n"] == 35).unwrap().clone();
    assert_eq!(split(b35["bn"].as_str().unwrap()), split("মানত ও কসম"));
    // Its book 0's narrations are empty in every language upstream, so it lists nothing.
    assert!(books["books"].as_array().unwrap().iter().all(|b| b["n"] != 0));
}

#[test]
fn bukhari_1_lists_narrations_1_to_7() {
    let list = deen(&["hadith", "list", "bukhari", "1"]);
    let ns: Vec<&str> = list["hadiths"].as_array().unwrap().iter().map(|h| h["n"].as_str().unwrap()).collect();
    assert_eq!(ns, ["1", "2", "3", "4", "5", "6", "7"]);
}

#[test]
fn muslims_empty_narrations_are_left_out_of_lists_and_search() {
    let list = deen(&["hadith", "list", "muslim", "0"]);
    let ns: Vec<&str> = list["hadiths"].as_array().unwrap().iter().map(|h| h["n"].as_str().unwrap()).collect();
    assert!(!ns.is_empty());
    assert!(!ns.contains(&"1") && !ns.contains(&"2"), "{ns:?}");
    let collections = deen(&["hadith", "collections"]);
    let muslim = collections["collections"].as_array().unwrap().iter().find(|c| c["slug"] == "muslim").unwrap().clone();
    assert_eq!(muslim["count"], 7563 - 167);
    let found = deen(&["hadith", "search", "--limit", "100000", "নিয়ত"]);
    assert!(found["hits"].as_array().unwrap().iter().all(|h| h["collection"] != "muslim" || (h["hadith"]["n"] != "1" && h["hadith"]["n"] != "2")));
}

#[test]
fn headings_numbered_with_a_danda_or_comma_go() {
    for (r, start) in [("bukhari:284", "আনাস ইব্\u{200C}নু মালিক"), ("bukhari:509", "আবূ")] {
        let text = bn(r);
        assert!(text.starts_with(&split(start)), "{r}: {text}");
    }
}

#[test]
fn references_with_parentheses_inside_and_publisher_initials_go() {
    assert!(!bn("bukhari:141").contains(&split("আহমাদ ১৯০৮")));
    assert!(!bn("bukhari:305").contains(&split("আ.প্র.")));
    assert!(bn("ibnmajah:446").ends_with(&split("খিলাল করেন।")));
}

#[test]
fn a_grade_in_brackets_stays() {
    assert!(bn("nawawi:12").contains(&split("হাসান")));
}

#[test]
fn bangla_search_finds_both_spellings() {
    let one = deen(&["hadith", "search", "--limit", "1", "নি\u{9DF}ত"])["total"].clone();
    let split_ = deen(&["hadith", "search", "--limit", "1", "নি\u{9AF}\u{9BC}ত"])["total"].clone();
    assert_eq!(one, split_);
    assert!(one.as_u64().unwrap() > 38, "{one}");
}
