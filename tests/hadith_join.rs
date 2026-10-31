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

/// One narration of the editions: its number, book, Arabic, English, Bangla.
type Row<'a> = (u32, u32, &'a str, &'a str, &'a str);

const NAMES: &str = "[collections]\nbukhari = \"সহীহ বুখারী\"\n\n[books.bukhari]\n\"1\" = \"ওহীর সূচনা\"\n\"2\" = \"ঈমান\"\n\"3\" = \"ইলম\"\n";

/// Bukhari's three editions, sections 1 to 3 titled, joined with `names` as
/// the snapshot's hadith-books-bn.toml.
fn join_rows(name: &str, rows: &[Row], names: &str) -> PathBuf {
    let root = scratch(name);
    let (src, out) = (root.join("deen-data"), root.join("hadith"));
    let ed = src.join("hadith");
    fs::create_dir_all(&ed).unwrap();
    fs::write(src.join("manifest.toml"), "").unwrap();
    fs::write(src.join("hadith-books-bn.toml"), names).unwrap();
    fs::create_dir_all(&out).unwrap();
    let edition = |text: &dyn Fn(&Row) -> String| {
        let hadiths: Vec<Value> = rows
            .iter()
            .map(|r| json!({ "hadithnumber": r.0, "text": text(r), "grades": [], "reference": { "book": r.1, "hadith": r.0 } }))
            .collect();
        let sections = json!({ "0": "", "1": "Revelation", "2": "Belief", "3": "Knowledge" });
        serde_json::to_vec(&json!({ "metadata": { "name": "Sahih al Bukhari", "sections": sections }, "hadiths": hadiths })).unwrap()
    };
    fs::write(ed.join("ara-bukhari.min.json"), edition(&|r| r.2.to_string())).unwrap();
    fs::write(ed.join("eng-bukhari.min.json"), edition(&|r| r.3.to_string())).unwrap();
    fs::write(ed.join("ben-bukhari.min.json"), edition(&|r| r.4.to_string())).unwrap();
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/join-hadith");
    let run = Command::new("python3").arg("-I").arg(script).arg(&src).arg(&out).arg("2").arg("bukhari").output().unwrap();
    assert!(run.status.success(), "{}", String::from_utf8_lossy(&run.stderr));
    out
}

/// Bukhari's book 1, one narration per (number, Bangla) pair; the Arabic and
/// English are placeholders.
fn join(name: &str, bangla: &[(u32, &str)]) -> PathBuf {
    let ar: Vec<String> = bangla.iter().map(|(n, _)| format!("حديث {n}")).collect();
    let en: Vec<String> = bangla.iter().map(|(n, _)| format!("Narration {n}")).collect();
    let rows: Vec<Row> = bangla.iter().enumerate().map(|(i, &(n, bn))| (n, 1, ar[i].as_str(), en[i].as_str(), bn)).collect();
    join_rows(name, &rows, NAMES)
}

fn deen(out: &Path, args: &[&str]) -> Value {
    let run = Command::new(env!("CARGO_BIN_EXE_deen")).args(args).env("DEEN_HADITH", out).output().unwrap();
    assert!(run.status.success(), "{}", String::from_utf8_lossy(&run.stderr));
    serde_json::from_slice(&run.stdout).unwrap()
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
    let search = |q: &str| deen(&out, &["hadith", "search", q]);
    let hits = search("নিয়ত");
    assert_eq!(hits["total"], 1, "{hits}");
    assert_eq!(hits["hits"][0]["hadith"]["n"], "2");
    assert_eq!(search("অনুচ্ছেদঃ")["total"], 0);
}

/// Book 1 holds two narrations and one empty in every language, book 2 one
/// with English alone, book 3 only an empty one, and book 0 one more.
const BOOKS: [Row; 6] = [
    (1, 1, "حديث ١", "Narration 1", "প্রথম হাদীস।"),
    (2, 1, "", "", ""),
    (3, 1, "حديث ٣", "Narration 3", "তৃতীয় হাদীস।"),
    (4, 2, "حديث ٤", "Narration 4", ""),
    (5, 3, "", "", ""),
    (6, 0, "حديث ٦", "Narration 6", "ষষ্ঠ হাদীস।"),
];

#[test]
fn books_are_named_in_bangla_and_count_the_narrations_with_text() {
    let out = join_rows("books", &BOOKS, NAMES);
    let books = deen(&out, &["hadith", "books", "bukhari"]);
    assert_eq!(books["collection"], "bukhari");
    assert_eq!(books["bn"], "সহীহ বুখারী");
    assert_eq!(
        books["books"],
        json!([
            { "n": 1, "bn": "ওহীর সূচনা", "en": "Revelation", "count": 2 },
            { "n": 2, "bn": "ঈমান", "en": "Belief", "count": 1 },
            { "n": 0, "bn": "অন্যান্য", "en": "", "count": 1 },
        ])
    );
    let collection = &deen(&out, &["hadith", "collections"])["collections"][0];
    assert_eq!(collection["bn"], "সহীহ বুখারী");
    assert_eq!(collection["count"], 4);
    // The narrations stay on disk, so the daily pick's arithmetic over them holds.
    assert_eq!(collection["hadiths"], 6);
}

#[test]
fn the_names_file_may_name_book_0() {
    let out = join_rows("book0", &BOOKS, &format!("{NAMES}\"0\" = \"বর্গাচাষ\"\n"));
    let books = deen(&out, &["hadith", "books", "bukhari"]);
    assert_eq!(books["books"][2], json!({ "n": 0, "bn": "বর্গাচাষ", "en": "", "count": 1 }));
}

#[test]
fn a_book_lists_its_narrations_with_text_in_order() {
    let out = join_rows("list", &BOOKS, NAMES);
    let list = deen(&out, &["hadith", "list", "bukhari", "1"]);
    assert_eq!(list["collection"], "bukhari");
    assert_eq!(list["bn"], "সহীহ বুখারী");
    assert_eq!(list["book"], json!({ "n": 1, "bn": "ওহীর সূচনা", "en": "Revelation", "count": 2 }));
    let ns: Vec<&str> = list["hadiths"].as_array().unwrap().iter().map(|h| h["n"].as_str().unwrap()).collect();
    assert_eq!(ns, ["1", "3"]);
    assert_eq!(list["hadiths"][1]["bn"], "তৃতীয় হাদীস।");
    let zero = deen(&out, &["hadith", "list", "bukhari", "0"]);
    assert_eq!(zero["hadiths"][0]["n"], "6");
}

#[test]
fn a_hit_carries_its_collections_and_books_bangla_names() {
    let out = join_rows("hit", &BOOKS, NAMES);
    let hit = &deen(&out, &["hadith", "show", "bukhari:3"])["hit"];
    assert_eq!(hit["bn"], "সহীহ বুখারী");
    assert_eq!(hit["book"], json!({ "n": 1, "bn": "ওহীর সূচনা", "en": "Revelation", "count": 2 }));
    let found = deen(&out, &["hadith", "search", "ষষ্ঠ"]);
    assert_eq!(found["hits"][0]["book"]["bn"], "অন্যান্য");
}
