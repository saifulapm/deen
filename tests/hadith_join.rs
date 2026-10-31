//! scripts/join-hadith on hand-made editions: what it keeps of each
//! narration's Bangla as `bn`, and the raw text it keeps beside it.

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};

use serde_json::{json, Value};

fn scratch(name: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("hadith-join-{name}"));
    let _ = fs::remove_dir_all(&p);
    fs::create_dir_all(&p).unwrap();
    p
}

/// Bukhari in three editions, one narration per (number, Bangla) pair; the
/// Arabic and English are placeholders.
fn join(name: &str, bangla: &[(u32, &str)]) -> PathBuf {
    let root = scratch(name);
    let (ed, out) = (root.join("editions"), root.join("hadith"));
    fs::create_dir_all(&ed).unwrap();
    fs::create_dir_all(&out).unwrap();
    let edition = |text: &dyn Fn(u32, &str) -> String| {
        let hadiths: Vec<Value> = bangla
            .iter()
            .map(|&(n, bn)| json!({ "hadithnumber": n, "text": text(n, bn), "grades": [], "reference": { "book": 1, "hadith": n } }))
            .collect();
        serde_json::to_vec(&json!({ "metadata": { "name": "Sahih al Bukhari", "sections": { "1": "Revelation" } }, "hadiths": hadiths })).unwrap()
    };
    fs::write(ed.join("ara-bukhari.min.json"), edition(&|n, _| format!("حديث {n}"))).unwrap();
    fs::write(ed.join("eng-bukhari.min.json"), edition(&|n, _| format!("Narration {n}"))).unwrap();
    fs::write(ed.join("ben-bukhari.min.json"), edition(&|_, bn| bn.to_string())).unwrap();
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/join-hadith");
    let run = Command::new("python3").arg("-I").arg(script).arg(&ed).arg(&out).arg("0123456789ab").arg("bukhari").output().unwrap();
    assert!(run.status.success(), "{}", String::from_utf8_lossy(&run.stderr));
    out
}

fn rows(out: &Path) -> Vec<Value> {
    fs::read_to_string(out.join("bukhari.jsonl")).unwrap().lines().map(|l| serde_json::from_str(l).unwrap()).collect()
}

fn cleaned(raw: &str) -> Option<String> {
    // Tests run at once, so each join gets its own directory.
    static RUNS: AtomicUsize = AtomicUsize::new(0);
    let out = join(&format!("one-{}", RUNS.fetch_add(1, Ordering::Relaxed)), &[(5, raw)]);
    let row = &rows(&out)[0];
    assert_eq!(row["bn_raw"], raw.trim(), "the raw text is kept as it came");
    row.get("bn").map(|b| b.as_str().unwrap().to_string())
}

#[test]
fn the_heading_up_to_the_narrations_own_number_goes() {
    assert_eq!(
        cleaned("وَقَوْلُ اللهِ (إِنَّا أَوْحَيْنَا) আল্লাহর বাণীঃ ওয়াহী। (সূরাহ্ আন-নিসা ৪/১৬৩) ৫. আবূ হুরাইরাহ হতে বর্ণিত। তিনি বলেন।").as_deref(),
        Some("আবূ হুরাইরাহ হতে বর্ণিত। তিনি বলেন।")
    );
    // A heading in Bangla alone, and Malik's word before the number.
    assert_eq!(cleaned("এ বিষয়ে হাদীস বর্ণিত হয়েছে। ৫. আনাস হতে বর্ণিত।").as_deref(), Some("আনাস হতে বর্ণিত।"));
    assert_eq!(cleaned("রেওয়ায়ত ৫. নাফি’ (রহঃ) হইতে বর্ণিত।").as_deref(), Some("নাফি’ (রহঃ) হইতে বর্ণিত।"));
}

#[test]
fn another_narrations_number_in_the_text_is_not_cut() {
    assert_eq!(cleaned("আনাস হতে বর্ণিত। তিনি ১৫. বার বললেন।").as_deref(), Some("আনাস হতে বর্ণিত। তিনি ১৫. বার বললেন।"));
    assert_eq!(cleaned("আনাস হতে বর্ণিত। ৫৫. তিনি বললেন।").as_deref(), Some("আনাস হতে বর্ণিত। ৫৫. তিনি বললেন।"));
}

#[test]
fn a_leading_chain_stub_goes_and_so_does_a_bare_danda() {
    assert_eq!(
        cleaned("উবাইদুল্লাহ (রহঃ) ..... আয়িশাহ (রাযিঃ) থেকে বর্ণিত। নম্রতা সুন্দর। (ইসলামিক ফাউন্ডেশন ৬৩৬৬, ইসলামিক সেন্টার)").as_deref(),
        Some("আয়িশাহ (রাযিঃ) থেকে বর্ণিত। নম্রতা সুন্দর।")
    );
    assert_eq!(cleaned("। মুগীরাহ (রাঃ) সূত্রে বর্ণিত।[1] হাসান সহীহ।").as_deref(), Some("মুগীরাহ (রাঃ) সূত্রে বর্ণিত। হাসান সহীহ।"));
    // The ruling names five dots; Nasai's three-dot stub stays.
    assert_eq!(cleaned("কুতায়বা (রহঃ) ... আবূ হুরায়রা (রাঃ) থেকে বর্ণিত।").as_deref(), Some("কুতায়বা (রহঃ) ... আবূ হুরায়রা (রাঃ) থেকে বর্ণিত।"));
}

#[test]
fn a_trailing_chain_note_goes() {
    assert_eq!(
        cleaned("ইবনু ‘আব্বাস (রাঃ) হতে বর্ণিত। সুস্থতা আর অবসর। ‘আব্বাস (রহ.) .....সা‘ঈদ (রহ.) থেকে এ রকমই হাদীস বর্ণনা করেছেন। (আধুনিক প্রকাশনী- ৫৯৬৪, ইসলামিক ফাউন্ডেশন)").as_deref(),
        Some("ইবনু ‘আব্বাস (রাঃ) হতে বর্ণিত। সুস্থতা আর অবসর।")
    );
}

#[test]
fn dots_inside_the_narration_mark_a_passage_left_out_and_stay() {
    assert_eq!(
        cleaned("। আনাস (রাঃ) সূত্রে বর্ণিত। তিনি পড়লেনঃ ‘‘কাল-কাওসার .....’’ সূরাটির শেষ পর্যন্ত। তিনি বললেন, তা একটি নহর।[1] হাসান : মুসলিম।").as_deref(),
        Some("আনাস (রাঃ) সূত্রে বর্ণিত। তিনি পড়লেনঃ ‘‘কাল-কাওসার .....’’ সূরাটির শেষ পর্যন্ত। তিনি বললেন, তা একটি নহর। হাসান : মুসলিম।")
    );
}

#[test]
fn dots_after_someone_speaks_leave_out_a_passage_and_are_no_chain() {
    assert_eq!(
        cleaned("আবু হুরাইরা (রাঃ) হতে বর্ণিত আছে, তিনি বলেন, ...... উপরের হাদীসের অনুরূপ।").as_deref(),
        Some("আবু হুরাইরা (রাঃ) হতে বর্ণিত আছে, তিনি বলেন, ...... উপরের হাদীসের অনুরূপ।")
    );
    assert_eq!(
        cleaned("উবাইদুল্লাহ (রহঃ) ..... যিয়াদ (রহঃ) হতে বর্ণিত। লোকটি বলছেঃ গভর্নর এসেছেন ...... রসূলুল্লাহ বলেছেনঃ আল্লাহ তার প্রতি দেখবেন না।").as_deref(),
        Some("যিয়াদ (রহঃ) হতে বর্ণিত। লোকটি বলছেঃ গভর্নর এসেছেন ...... রসূলুল্লাহ বলেছেনঃ আল্লাহ তার প্রতি দেখবেন না।")
    );
}

#[test]
fn references_go_and_grades_and_the_texts_own_brackets_stay() {
    assert_eq!(
        cleaned("। সালামা (রাঃ) হতে বর্ণিত। অহংকার। যঈফ, যঈফা (১৯১৪) আবূ ঈসা বলেনঃ এ হাদীসটি হাসান গারীব।").as_deref(),
        Some("সালামা (রাঃ) হতে বর্ণিত। অহংকার। যঈফ, যঈফা আবূ ঈসা বলেনঃ এ হাদীসটি হাসান গারীব।")
    );
    assert_eq!(
        cleaned("বীর সে নয়। [মুসলিম ৪৫/৩০, হাঃ ২৬০৯, আহমাদ ৭২২৩] (আধুনিক প্রকাশনী- ৫৬৭৪, ইসলামিক ফাউন্ডেশন)").as_deref(),
        Some("বীর সে নয়।")
    );
    assert_eq!(
        cleaned("হিজরাত করেছে।] (৫৪, ২৫২৯, ৩৮৯৮; মুসলিম ২৩/৪৫ হাঃ ১৯০৭, আহমাদ ১৬৮) ( আধুনিক প্রকাশনী- ১, ইসলামিক ফাউন্ডেশন)").as_deref(),
        Some("হিজরাত করেছে।")
    );
    assert_eq!(cleaned("দেখুন। (১২৫০ দ্রষ্টব্য) তিনি (১) বলেন।").as_deref(), Some("দেখুন। তিনি বলেন।"));
    assert_eq!(cleaned("একটি শাখা। (মুসলিম ১/১২ হাঃ ৩৫, আহমাদ ৯৩৭২)").as_deref(), Some("একটি শাখা।"));
    assert_eq!(cleaned("হাদীস। [সহীহ্ মুসলিমঃ]").as_deref(), Some("হাদীস।"));
    assert_eq!(
        cleaned("তিনি (২ বার) বললেন, [আবূ বাকর (রাযিঃ)] (৮ই যিলহাজ্জ) কাজ (এর প্রাপ্য হবে) নিয়্যাত। (সূরাহ লুকমান ৩১ঃ ৩৪)").as_deref(),
        Some("তিনি (২ বার) বললেন, [আবূ বাকর (রাযিঃ)] (৮ই যিলহাজ্জ) কাজ (এর প্রাপ্য হবে) নিয়্যাত। (সূরাহ লুকমান ৩১ঃ ৩৪)")
    );
}

#[test]
fn bangla_that_cleans_to_nothing_is_no_bangla() {
    assert_eq!(cleaned("باب ৫. [মুসলিম ১]"), None);
}

#[test]
fn bangla_search_reads_the_cleaned_text_and_not_the_raw() {
    let out = join("search", &[(1, "অনুচ্ছেদঃ নিয়তের বর্ণনা ১. আনাস হতে বর্ণিত।"), (2, "উমার হতে বর্ণিত। নিয়ত অনুযায়ী ফল।")]);
    let search = |q: &str| -> Value {
        let run = Command::new(env!("CARGO_BIN_EXE_deen")).args(["hadith", "search", q]).env("DEEN_HADITH", &out).output().unwrap();
        assert!(run.status.success(), "{}", String::from_utf8_lossy(&run.stderr));
        serde_json::from_slice(&run.stdout).unwrap()
    };
    let hits = search("নিয়ত");
    assert_eq!(hits["total"], 1, "{hits}");
    assert_eq!(hits["hits"][0]["hadith"]["n"], "2");
    assert_eq!(search("অনুচ্ছেদঃ")["total"], 0);
}
