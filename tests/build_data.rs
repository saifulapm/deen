//! `scripts/build-data` against a sources directory laid out as the install
//! hands it over, with every surah's real ayah count and made-up text, and
//! then deen reading what it built.

use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

use serde_json::{json, Value};

const COUNTS: [u16; 114] = [
    7, 286, 200, 176, 120, 165, 206, 75, 129, 109, 123, 111, 43, 52, 99, 128, 111, 110, 98, 135,
    112, 78, 118, 64, 77, 227, 93, 88, 69, 60, 34, 30, 73, 54, 45, 83, 182, 88, 75, 85, 54, 53, 89,
    59, 37, 35, 38, 29, 18, 45, 60, 49, 62, 55, 78, 96, 29, 22, 24, 13, 14, 11, 11, 18, 12, 12, 30,
    52, 52, 44, 28, 28, 20, 56, 40, 31, 50, 40, 46, 42, 29, 19, 36, 25, 22, 17, 19, 26, 30, 20, 15,
    21, 11, 8, 8, 19, 5, 8, 8, 11, 11, 8, 3, 9, 5, 4, 7, 3, 6, 3, 5, 4, 5, 6,
];
const BASMALA: &str = "بِسْمِ ٱللَّهِ ٱلرَّحْمَٰنِ ٱلرَّحِيمِ";

fn scratch(name: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("build-data-{name}"));
    let _ = fs::remove_dir_all(&p);
    fs::create_dir_all(&p).unwrap();
    p
}

/// A number written in Arabic letters, one a digit, since the pronunciation
/// stops on a digit inside an ayah.
fn letters(n: u16) -> String {
    n.to_string().chars().map(|d| "بتثجحخدذرز".chars().nth(d as usize - '0' as usize).unwrap()).collect()
}

/// A number in Bangla digits.
fn letters_bn(n: u16) -> String {
    n.to_string().chars().map(|d| char::from_u32('০' as u32 + (d as u32 - '0' as u32)).unwrap()).collect()
}

fn ayat() -> impl Iterator<Item = (u16, u16)> {
    (1..=114u16).flat_map(|s| (1..=COUNTS[s as usize - 1]).map(move |a| (s, a)))
}

/// Two words an ayah, the Basmala glued onto each first ayah as Tanzil does,
/// and a Bangla meaning per ayah; `bn` overrides the meaning of chosen ayat.
fn sources(dir: &Path, bn: &[((u16, u16), &str)]) {
    for d in ["quran", "words", "duas"] {
        fs::create_dir_all(dir.join(d)).unwrap();
    }
    let mut tanzil = String::new();
    let (mut zakaria, mut words) = (Vec::new(), serde_json::Map::new());
    let mut wbw = serde_json::Map::new();
    for (s, a) in ayat() {
        let text = format!("كَلِمَة{} آيَة{}", letters(s), letters(a));
        let glued = if a == 1 && s != 1 && s != 9 { format!("{BASMALA} {text}") } else { text.clone() };
        tanzil.push_str(&format!("{s}|{a}|{glued}\n"));
        let meaning = bn.iter().find(|(r, _)| *r == (s, a)).map_or(format!("অর্থ {s}:{a}"), |(_, t)| t.to_string());
        zakaria.push(json!({ "chapter": s, "verse": a, "text": meaning }));
        words.insert(format!("{s}:{a}"), json!({ "juz": 1, "words": text.split(' ').enumerate().map(|(k, w)| json!({
            "text": w, "en": format!("word {}", k + 1), "tr": "tr", "root": null,
            "audio": format!("{s:03}_{a:03}_{:03}", k + 1),
        })).collect::<Vec<_>>() }));
        wbw.entry(s.to_string()).or_insert_with(|| json!({}))[a.to_string()] = json!([["শব্দ ১", "শব্দ ২"]]);
    }
    tanzil.push_str("# a licence footer\n");
    fs::write(dir.join("quran/uthmani.txt"), tanzil).unwrap();
    fs::write(dir.join("quran/tajweed.json"), json!([{ "surah": 2, "ayah": 1, "annotations": [{ "start": 40, "end": 41, "rule": "madd_2" }] }]).to_string()).unwrap();
    fs::write(dir.join("quran/ben-abubakrzakaria.json"), json!({ "quran": zakaria }).to_string()).unwrap();
    let surahs: Vec<_> = (1..=114u16).map(|s| json!({
        "number": s, "name": format!("سُورَة {s}"), "englishName": format!("Surah-{s}"),
        "englishNameTranslation": "The Name", "revelationType": "Meccan",
        "ayahs": (1..=COUNTS[s as usize - 1]).map(|a| json!({ "numberInSurah": a, "text": format!("English {s}:{a}") })).collect::<Vec<_>>(),
    })).collect();
    fs::write(dir.join("quran/en.sahih.json"), json!({ "code": 200, "data": { "surahs": surahs } }).to_string()).unwrap();
    let names: String = (1..=114u16).map(|s| format!("{s} = \"সূরা {}\"\n", letters_bn(s))).collect();
    fs::write(dir.join("surahs-bn.toml"), format!("[surahs]\n{names}")).unwrap();
    fs::write(dir.join("words/words.json"), Value::Object(words).to_string()).unwrap();
    fs::write(dir.join("words/quranwbw-bn.json"), Value::Object(wbw).to_string()).unwrap();
    duas(dir);
}

/// Hisn al-Muslim in two chapters: dua 1 joins one segment of the Bangla
/// book, dua 2 two segments, the second with no pronunciation of its own,
/// and dua 3 none. The Bangla is made up, laid out as dua-api's CSV is.
fn duas(dir: &Path) {
    let source = json!({ "book": "Hisn al-Muslim", "author": "al-Qahtani", "url": "u", "checked_against": "a",
                         "counts_from": "c", "duas": 3, "with_count_or_audio": 0 });
    let dua = |id: &str, chapter: u16, ar: &str| json!({ "id": id, "chapter": chapter, "ar": ar, "tr": "tr", "en": format!("English {id}"), "ref": format!("Ref {id}") });
    let book = json!({
        "source": source,
        "chapters": [{ "n": 1, "en": "When waking up", "ar": "باب" }, { "n": 2, "en": "When leaving home", "ar": "باب" }],
        "duas": [dua("1", 1, "بِسْمِ ٱللَّهِ"), dua("2", 2, "قُلْ هُوَ ٱللَّهُ أَحَدٌ"), dua("3", 2, "ٱلْحَمْدُ لِلَّهِ")],
    });
    fs::write(dir.join("duas/hisn.json"), book.to_string()).unwrap();
    fs::write(dir.join("dua-join.toml"), "[chapters]\n1 = \"প্রথম অধ্যায়\"\n2 = \"দ্বিতীয় অধ্যায়\"\n\n\
        [dua.1]\nchapter = 1\ntier = \"exact\"\nsegments = [[10, 0]]\n\n\
        [dua.2]\nchapter = 2\ntier = \"close\"\nsegments = [[11, 0], [11, 1]]\n\n\
        [dua.3]\nchapter = 2\ntier = \"none\"\nsegments = []\n").unwrap();
    fs::create_dir_all(dir.join("duas/dua-api")).unwrap();
    let row = |g: u16, seg: &str, ar: &str, tr: &str, bn: &str, bottom: &str, reference: &str| {
        [ "1", &g.to_string(), "0", seg, "", "", ar, tr, bn, bottom, reference, "" ].map(|f| format!("\"{}\"", f.replace('"', "\"\""))).join(",")
    };
    let csv = [
        "\"book_id\",\"dua_global_id\",\"ID\",\"dua_segment_id\",\"top\",\"arabic_diacless\",\"arabic\",\"transliteration\",\"translations\",\"bottom\",\"reference\",\"app_reference\"".to_string(),
        row(10, "", "بِسْمِ اللّٰهِ", "পাঠ এক", "আল্লাহ্\u{200c}র\u{a0}নামে\nশুরু  [১] [ব্যাখ্যা]", "", "[১] সূত্র এক ১/২\n[২] সূত্র দুই"),
        row(11, "1", "قُلْ", "পাঠ দুই", "বলো", "", ""),
        row(11, "2", "هُوَ اللّٰهُ اَحَدٌ", "", "তিনি এক [২]", "", ""),
        row(11, "3", "قُلْ", "পাঠ তিন", "পরের অংশ", "", "[১] প্রথম সূত্র\n[২] দ্বিতীয় সূত্র [৩]"),
    ];
    fs::write(dir.join("duas/dua-api/duadetails.csv"), csv.join("\n") + "\n").unwrap();
}

fn build(src: &Path, out: &Path) -> Output {
    Command::new(concat!(env!("CARGO_MANIFEST_DIR"), "/scripts/build-data"))
        .env("DEEN_BIN", env!("CARGO_BIN_EXE_deen"))
        .arg(src)
        .arg(out)
        .output()
        .unwrap()
}

fn deen(out: &Path, args: &[&str]) -> Value {
    let o = Command::new(env!("CARGO_BIN_EXE_deen"))
        .args(args)
        .env("DEEN_DATA", out.join("quran.json"))
        .env("DEEN_WORDS", out.join("words.json"))
        .env("DEEN_DUAS", out.join("duas.json"))
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    serde_json::from_slice(&o.stdout).unwrap()
}

#[test]
fn builds_the_quran_with_its_bangla_meaning_footnote_marks_stripped() {
    let dir = scratch("strip");
    let (src, out) = (dir.join("src"), dir.join("out"));
    sources(&src, &[
        ((94, 6), "প্রথম কথার সাথেই দ্বিতীয় কথা আছে [১]।"),
        ((1, 2), "এক ‘শব্‌দ’ [১] কথা [২] , দুই"),
        ((2, 5), "এক দুই[১]।"),
        ((2, 245), "এক দুই [১।] আর তিন"),
        ((3, 96), "এক দুই [\u{200c}১] তিন"),
        ((14, 17), "এক না [২। আর দুই"),
        ((5, 3), "এক <b>দুই</b> তিন"),
        ((4, 77), "এক না[ ৩]?’"),
    ]);
    let r = build(&src, &out);
    assert!(r.status.success(), "{}", String::from_utf8_lossy(&r.stderr));

    let v = deen(&out, &["api", "ayah", "94:6"]);
    assert_eq!(v["ayah"]["bn"], "প্রথম কথার সাথেই দ্বিতীয় কথা আছে।");
    assert_eq!(v["ayah"]["en"], "English 94:6");
    let bn = |r: &str| deen(&out, &["api", "ayah", r])["ayah"]["bn"].as_str().unwrap().to_string();
    assert_eq!(bn("1:2"), "এক ‘শব্‌দ’ কথা , দুই");
    assert_eq!(bn("2:5"), "এক দুই।");
    assert_eq!(bn("2:245"), "এক দুই। আর তিন");
    assert_eq!(bn("3:96"), "এক দুই তিন");
    assert_eq!(bn("14:17"), "এক না। আর দুই");
    assert_eq!(bn("5:3"), "এক দুই তিন");
    assert_eq!(bn("4:77"), "এক না?’");

    let quran: Value = serde_json::from_str(&fs::read_to_string(out.join("quran.json")).unwrap()).unwrap();
    assert_eq!(quran["ayahs"].as_array().unwrap().len(), 6236);
    // The Basmala is cut from each first ayah and carried once, as before.
    assert_eq!(quran["basmala"], "كَلِمَةت آيَةت");
    assert_eq!(deen(&out, &["api", "ayah", "2:1"])["ayah"]["ar"], "كَلِمَةث آيَةت");
    assert_eq!(quran["surahs"][93]["en"], "Surah-94");
    let surahs = deen(&out, &["api", "surahs"]);
    assert_eq!(surahs["surahs"][93]["bn"], "সূরা ৯৪");
    assert_eq!(deen(&out, &["api", "surah", "94"])["surah"]["bn"], "সূরা ৯৪");
    // 2:1's span sat 40 code points in, past the 38 of the Basmala and its space.
    assert_eq!(quran["ayahs"][7]["taj"], json!([[1, 2, 0]]));
}

#[test]
fn each_word_gains_its_bangla_meaning() {
    let dir = scratch("words");
    let (src, out) = (dir.join("src"), dir.join("out"));
    sources(&src, &[]);
    let r = build(&src, &out);
    assert!(r.status.success(), "{}", String::from_utf8_lossy(&r.stderr));
    let v = deen(&out, &["api", "ayah", "94:6"]);
    assert_eq!(v["words"][0]["bn"], "শব্দ ১");
    assert_eq!(v["words"][1]["bn"], "শব্দ ২");
    assert_eq!(v["words"][1]["en"], "word 2");
    // 94:6 is كَلِمَةذح آيَةد in these sources.
    assert_eq!(v["ayah"]["bn_pron"], "কালিমাতযহ আ-য়াতদ");
    assert_eq!(v["words"][0]["bn_pron"], "কালিমাতযহ");
}

#[test]
fn each_dua_gains_the_bangla_book_its_join_names() {
    let dir = scratch("duas");
    let (src, out) = (dir.join("src"), dir.join("out"));
    sources(&src, &[]);
    let r = build(&src, &out);
    assert!(r.status.success(), "{}", String::from_utf8_lossy(&r.stderr));
    let list = deen(&out, &["dua", "list"]);
    assert_eq!(list["chapters"][0]["bn"], "প্রথম অধ্যায়");
    assert_eq!(list["chapters"][1]["bn"], "দ্বিতীয় অধ্যায়");
    assert_eq!(list["chapters"][1]["en"], "When leaving home");
    // Its footnote mark goes, the newline and the no-break space read as
    // spaces, and the zero-width non-joiner and the aside in brackets stay.
    let one = &deen(&out, &["dua", "show", "1"])["dua"];
    assert_eq!(one["bn"], "আল্লাহ্\u{200c}র নামে শুরু [ব্যাখ্যা]");
    assert_eq!(one["bn_pron"], "পাঠ এক");
    assert_eq!(one["bn_ref"], "সূত্র এক ১/২");
    assert_eq!(one["ar"], "بِسْمِ ٱللَّهِ");
    assert_eq!(one["en"], "English 1");
    // Two segments join with a space; the second has no pronunciation of its
    // own, so the engine reads its Arabic; the reference is the line its
    // footnote names.
    let two = &deen(&out, &["dua", "show", "2"])["dua"];
    assert_eq!(two["bn"], "বলো তিনি এক");
    assert_eq!(two["bn_pron"], "পাঠ দুই হুওয়াল্লা-হু আহাদ");
    assert_eq!(two["bn_ref"], "দ্বিতীয় সূত্র");
    // A dua the book lacks reads in the engine's pronunciation alone.
    let three = &deen(&out, &["dua", "show", "3"])["dua"];
    assert_eq!(three["bn_pron"], "আলহামদু লিল্লা-হ");
    assert!(three.get("bn").is_none() && three.get("bn_ref").is_none(), "{three}");
    assert_eq!(three["ref"], "Ref 3");
}

/// Builds from `sources` changed by `edit`, and gives the build's error.
fn fails(name: &str, edit: impl FnOnce(&Path)) -> String {
    let dir = scratch(name);
    let (src, out) = (dir.join("src"), dir.join("out"));
    sources(&src, &[]);
    edit(&src);
    let r = build(&src, &out);
    assert!(!r.status.success());
    assert!(!out.join("duas.json").exists());
    String::from_utf8_lossy(&r.stderr).to_string()
}

fn replace(path: &Path, from: &str, to: &str) {
    let text = fs::read_to_string(path).unwrap();
    assert!(text.contains(from), "{} holds no {from:?}", path.display());
    fs::write(path, text.replace(from, to)).unwrap();
}

#[test]
fn sources_without_the_dua_join_stop_the_build_saying_so() {
    let err = fails("no-join", |src| fs::remove_file(src.join("dua-join.toml")).unwrap());
    assert!(err.contains("build-data: the sources have no dua-join.toml"), "{err}");
}

#[test]
fn a_join_the_bangla_book_cannot_meet_stops_the_build_naming_the_dua() {
    let err = fails("join-segment", |src| replace(&src.join("dua-join.toml"), "[[11, 0], [11, 1]]", "[[11, 0], [11, 7]]"));
    assert!(err.contains("dua 2") && err.contains("11:7"), "{err}");
    let err = fails("join-dua", |src| replace(&src.join("dua-join.toml"), "[dua.3]", "[dua.4]"));
    assert!(err.contains("dua 3"), "{err}");
    let err = fails("join-chapter", |src| replace(&src.join("dua-join.toml"), "2 = \"দ্বিতীয় অধ্যায়\"\n", ""));
    assert!(err.contains("chapter 2"), "{err}");
}

#[test]
fn a_footnote_mark_left_in_a_dua_stops_the_build() {
    let err = fails("dua-mark", |src| replace(&src.join("duas/dua-api/duadetails.csv"), "তিনি এক [২]", "তিনি এক [২।]"));
    assert!(err.contains("dua 2"), "{err}");
}

#[test]
fn a_code_point_with_no_rule_in_a_dua_stops_the_build_naming_it() {
    let err = fails("dua-unknown", |src| replace(&src.join("duas/hisn.json"), "ٱلْحَمْدُ لِلَّهِ", "ٱلْحَمْدُ\u{08F0} لِلَّهِ"));
    assert!(err.contains("dua 3") && err.contains("U+08F0"), "{err}");
}

#[test]
fn a_footnote_mark_left_after_stripping_stops_the_build() {
    let dir = scratch("left");
    let (src, out) = (dir.join("src"), dir.join("out"));
    sources(&src, &[((7, 1), "একটি [ক১] চিহ্ন [ ১২")]);
    let r = build(&src, &out);
    assert!(!r.status.success());
    assert!(String::from_utf8_lossy(&r.stderr).contains("7:1"), "{}", String::from_utf8_lossy(&r.stderr));
    assert!(!out.join("quran.json").exists());
}

#[test]
fn a_word_count_that_differs_from_words_json_stops_the_build() {
    let dir = scratch("count");
    let (src, out) = (dir.join("src"), dir.join("out"));
    sources(&src, &[]);
    let path = src.join("words/quranwbw-bn.json");
    let mut wbw: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    wbw["36"]["5"] = json!([["শুধু এক"]]);
    fs::write(&path, wbw.to_string()).unwrap();
    let r = build(&src, &out);
    assert!(!r.status.success());
    assert!(String::from_utf8_lossy(&r.stderr).contains("36:5"), "{}", String::from_utf8_lossy(&r.stderr));
}

#[test]
fn a_word_with_an_empty_bangla_meaning_stops_the_build() {
    let dir = scratch("empty");
    let (src, out) = (dir.join("src"), dir.join("out"));
    sources(&src, &[]);
    let path = src.join("words/quranwbw-bn.json");
    let mut wbw: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    wbw["36"]["5"] = json!([["শব্দ ১", " "]]);
    fs::write(&path, wbw.to_string()).unwrap();
    let r = build(&src, &out);
    assert!(!r.status.success());
    assert!(String::from_utf8_lossy(&r.stderr).contains("36:5"), "{}", String::from_utf8_lossy(&r.stderr));
}

#[test]
fn every_ayah_and_word_gains_its_pronunciation() {
    let dir = scratch("pron");
    let (src, out) = (dir.join("src"), dir.join("out"));
    sources(&src, &[]);
    let r = build(&src, &out);
    assert!(r.status.success(), "{}", String::from_utf8_lossy(&r.stderr));
    let quran: Value = serde_json::from_str(&fs::read_to_string(out.join("quran.json")).unwrap()).unwrap();
    let blank = quran["ayahs"].as_array().unwrap().iter().filter(|a| a["bn_pron"].as_str().unwrap_or("").is_empty()).count();
    assert_eq!(blank, 0);
    let words: Value = serde_json::from_str(&fs::read_to_string(out.join("words.json")).unwrap()).unwrap();
    let all: Vec<_> = words.as_object().unwrap().values().flat_map(|a| a["words"].as_array().unwrap().iter()).collect();
    assert_eq!(all.len(), 6236 * 2);
    assert!(all.iter().all(|w| !w["bn_pron"].as_str().unwrap_or("").is_empty()));
}

#[test]
fn a_code_point_with_no_rule_stops_the_build_naming_the_ayah() {
    let dir = scratch("unknown");
    let (src, out) = (dir.join("src"), dir.join("out"));
    sources(&src, &[]);
    let path = src.join("quran/uthmani.txt");
    let text = fs::read_to_string(&path).unwrap().replace("36|5|كَلِمَة", "36|5|كَلِمَة\u{08F0}");
    fs::write(&path, text).unwrap();
    let r = build(&src, &out);
    assert!(!r.status.success());
    let err = String::from_utf8_lossy(&r.stderr);
    assert!(err.contains("36:5") && err.contains("U+08F0"), "{err}");
    assert!(!out.join("quran.json").exists());
}

/// The deen-data checkout the real-data tests build from: DEEN_SOURCES, or
/// the deen-data directory next to this crate.
fn checkout() -> PathBuf {
    let dir = std::env::var_os("DEEN_SOURCES").map_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../deen-data"), PathBuf::from);
    assert!(dir.join("manifest.toml").exists(), "no deen-data checkout at {}: set DEEN_SOURCES", dir.display());
    dir
}

#[test]
fn the_real_quran_reads_in_bangla_letters_only() {
    let out = scratch("real").join("out");
    let r = build(&checkout(), &out);
    assert!(r.status.success(), "{}", String::from_utf8_lossy(&r.stderr));
    let quran: Value = serde_json::from_str(&fs::read_to_string(out.join("quran.json")).unwrap()).unwrap();
    let ayahs = quran["ayahs"].as_array().unwrap();
    assert_eq!(ayahs.len(), 6236);
    let foreign = |c: char| c.is_ascii_alphabetic() || ('\u{0600}'..='\u{06FF}').contains(&c);
    for a in ayahs {
        let pron = a["bn_pron"].as_str().unwrap();
        assert!(!pron.is_empty() && !pron.chars().any(foreign), "{}:{} reads {pron:?}", a["s"], a["a"]);
    }
    assert_eq!(deen(&out, &["api", "ayah", "2:1"])["ayah"]["bn_pron"], "আলিফ লা-ম মী-ম");
    assert_eq!(deen(&out, &["api", "surahs"])["surahs"].as_array().unwrap().iter().filter(|s| s["bn"].as_str().is_some_and(|b| b.starts_with("সূরা "))).count(), 114);
    assert_eq!(deen(&out, &["api", "surahs"])["surahs"][93]["bn"], "সূরা আশ-শারহ");
    let words: Value = serde_json::from_str(&fs::read_to_string(out.join("words.json")).unwrap()).unwrap();
    let all: Vec<_> = words.as_object().unwrap().values().flat_map(|a| a["words"].as_array().unwrap().iter()).collect();
    assert_eq!(all.len(), 77_429);
    assert!(all.iter().all(|w| !w["bn_pron"].as_str().unwrap_or("").is_empty()));
}

#[test]
fn a_surah_with_no_bangla_name_stops_the_build() {
    let dir = scratch("names");
    let (src, out) = (dir.join("src"), dir.join("out"));
    sources(&src, &[]);
    let path = src.join("surahs-bn.toml");
    let text = fs::read_to_string(&path).unwrap().replace("57 = \"সূরা ৫৭\"\n", "");
    fs::write(&path, text).unwrap();
    let r = build(&src, &out);
    assert!(!r.status.success());
    assert!(String::from_utf8_lossy(&r.stderr).contains("surah 57"), "{}", String::from_utf8_lossy(&r.stderr));
}

#[test]
fn sources_without_the_surah_names_stop_the_build_saying_so() {
    let dir = scratch("no-names");
    let (src, out) = (dir.join("src"), dir.join("out"));
    sources(&src, &[]);
    fs::remove_file(src.join("surahs-bn.toml")).unwrap();
    let r = build(&src, &out);
    assert!(!r.status.success());
    let err = String::from_utf8_lossy(&r.stderr);
    assert!(err.contains("build-data: the sources have no surahs-bn.toml"), "{err}");
    assert!(!out.join("quran.json").exists());
}

/// One CSV record per row, its quoted fields allowed commas, newlines and
/// doubled quotes, as dua-api writes them, keyed by the header row.
fn csv(path: &Path) -> Vec<std::collections::HashMap<String, String>> {
    let text = fs::read_to_string(path).unwrap();
    let (mut rows, mut row, mut field) = (Vec::new(), Vec::new(), String::new());
    let (mut quoted, mut chars) = (false, text.chars().peekable());
    while let Some(c) = chars.next() {
        match (c, quoted) {
            ('"', true) if chars.peek() == Some(&'"') => {
                field.push('"');
                chars.next();
            }
            ('"', _) => quoted = !quoted,
            (',', false) => row.push(std::mem::take(&mut field)),
            ('\n', false) => {
                row.push(std::mem::take(&mut field));
                rows.push(std::mem::take(&mut row));
            }
            ('\r', false) => {}
            _ => field.push(c),
        }
    }
    let head = rows.remove(0);
    rows.into_iter().map(|r| head.iter().cloned().zip(r).collect()).collect()
}

/// The real duas carry the Bangla Hisnul Muslim as deen-data's join maps it;
/// what each should read is taken from that book's CSV here, so no line of
/// it is written into this public repo.
#[test]
fn the_real_duas_read_the_bangla_book() {
    let src = checkout();
    let out = scratch("real-duas").join("out");
    let r = build(&src, &out);
    assert!(r.status.success(), "{}", String::from_utf8_lossy(&r.stderr));
    let list = deen(&out, &["dua", "list"]);
    let duas = list["duas"].as_array().unwrap();
    assert_eq!(duas.len(), 268);
    let foreign = |c: char| c.is_ascii_alphabetic() || ('\u{0600}'..='\u{06FF}').contains(&c);
    for d in duas {
        let pron = d["bn_pron"].as_str().unwrap_or_default();
        assert!(!pron.is_empty() && !pron.chars().any(foreign), "dua {} reads {pron:?}", d["id"]);
        assert_eq!(d.get("bn").is_some(), d.get("bn_ref").is_some(), "dua {}", d["id"]);
    }
    assert_eq!(duas.iter().filter(|d| d.get("bn").is_none()).count(), 38);
    assert!(duas.iter().all(|d| !d["bn"].as_str().unwrap_or_default().contains("[১]")));

    let rows = csv(&src.join("duas/dua-api/duadetails.csv"));
    let seg = |g: &str, i: usize| rows.iter().filter(|r| r["book_id"] == "1" && r["dua_global_id"] == g).nth(i).unwrap();
    let dua = |id: &str| deen(&out, &["dua", "show", id])["dua"].clone();
    let unmarked = |s: &str| s.replace(" [১]", "");
    let first_line = |s: &str| s.lines().next().unwrap().trim_start_matches("[১] ").trim().to_string();

    // 16 is one segment, its meaning's footnote mark gone and its ZWNJ kept.
    let (book, d) = (seg("17", 0), dua("16"));
    assert_eq!(d["bn_pron"], book["transliteration"].as_str());
    assert_eq!(d["bn"], unmarked(&book["translations"]));
    assert!(d["bn"].as_str().unwrap().contains('\u{200C}'));
    assert_eq!(d["bn_ref"], first_line(&book["reference"]));
    // 22 reads the muezzin's words, then the response, in the book's order.
    let d = dua("22");
    assert_eq!(d["bn"], format!("{} {}", seg("23", 0)["translations"], unmarked(&seg("23", 1)["translations"])));
    // 25's line is the one its own footnote names, not its dua's first.
    let line = seg("23", 3)["reference"].lines().find(|l| l.starts_with("[৩]")).unwrap().trim_start_matches("[৩] ").trim().to_string();
    assert_eq!(dua("25")["bn_ref"], line);
    // 70's three surahs join with a space.
    let three: Vec<String> = (0..3).map(|i| seg("67", i)["transliteration"].trim().to_string()).collect();
    assert_eq!(dua("70")["bn_pron"], three.join(" "));
    // 236's second segment has no pronunciation, so the engine writes it
    // between the book's two.
    let d = dua("236");
    let pron = d["bn_pron"].as_str().unwrap();
    assert!(d["bn"].is_string());
    assert!(pron.starts_with(seg("225", 0)["transliteration"].trim()) && pron.ends_with(seg("225", 2)["transliteration"].trim()), "{pron}");
    assert!(pron.len() > seg("225", 0)["transliteration"].len() + seg("225", 2)["transliteration"].len() + 2, "{pron}");

    // The chapters' names, with deen's 49 and 50 swapped against dua-api's
    // and 111 under 110's.
    let names = csv(&src.join("duas/dua-api/duanames.csv"));
    let chapname = |n: &str| names.iter().find(|r| r["book_id"] == "1" && r["chap_id"] == n).unwrap()["chapname"].clone();
    let chapters = list["chapters"].as_array().unwrap();
    assert_eq!(chapters.len(), 132);
    assert!(chapters.iter().all(|c| !c["bn"].as_str().unwrap_or_default().is_empty()));
    let bn = |n: usize| chapters[n - 1]["bn"].as_str().unwrap().to_string();
    assert_eq!(bn(10), chapname("10"));
    assert_eq!((bn(49), bn(50)), (chapname("50"), chapname("49")));
    assert_eq!(bn(111), chapname("110"));
}
