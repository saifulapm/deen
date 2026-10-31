//! The corpus joined from the deen-data checkout's real editions (DEEN_SOURCES,
//! else ../deen-data), held to the narrations the plan names.

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
        let run = Command::new("python3").arg("-I").arg(script).arg(sources.join("hadith")).arg(&out).arg("df57907be35291c91ad6a6691180e22ca9920784").arg(COLLECTIONS).output().unwrap();
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
