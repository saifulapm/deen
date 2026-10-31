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
        let text = format!("كَلِمَة{s} آيَة{a}");
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
    fs::write(dir.join("words/words.json"), Value::Object(words).to_string()).unwrap();
    fs::write(dir.join("words/quranwbw-bn.json"), Value::Object(wbw).to_string()).unwrap();
    fs::write(dir.join("duas/hisn.json"), json!({ "source": "Hisn al-Muslim", "chapters": [] }).to_string()).unwrap();
}

fn build(src: &Path, out: &Path) -> Output {
    Command::new(concat!(env!("CARGO_MANIFEST_DIR"), "/scripts/build-data"))
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

    let quran: Value = serde_json::from_str(&fs::read_to_string(out.join("quran.json")).unwrap()).unwrap();
    assert_eq!(quran["ayahs"].as_array().unwrap().len(), 6236);
    // The Basmala is cut from each first ayah and carried once, as before.
    assert_eq!(quran["basmala"], "كَلِمَة1 آيَة1");
    assert_eq!(deen(&out, &["api", "ayah", "2:1"])["ayah"]["ar"], "كَلِمَة2 آيَة1");
    assert_eq!(quran["surahs"][93]["en"], "Surah-94");
    // 2:1's span sat 40 code points in, past the 38 of the Basmala and its space.
    assert_eq!(quran["ayahs"][7]["taj"], json!([[1, 2, 0]]));
}

#[test]
fn each_word_gains_its_bangla_meaning_and_hisn_al_muslim_is_kept_as_built() {
    let dir = scratch("words");
    let (src, out) = (dir.join("src"), dir.join("out"));
    sources(&src, &[]);
    let r = build(&src, &out);
    assert!(r.status.success(), "{}", String::from_utf8_lossy(&r.stderr));
    let v = deen(&out, &["api", "ayah", "94:6"]);
    assert_eq!(v["words"][0]["bn"], "শব্দ ১");
    assert_eq!(v["words"][1]["bn"], "শব্দ ২");
    assert_eq!(v["words"][1]["en"], "word 2");
    assert_eq!(fs::read(out.join("duas.json")).unwrap(), fs::read(src.join("duas/hisn.json")).unwrap());
}

#[test]
fn a_footnote_mark_left_after_stripping_stops_the_build() {
    let dir = scratch("left");
    let (src, out) = (dir.join("src"), dir.join("out"));
    sources(&src, &[((7, 1), "একটি [ক১] চিহ্ন [১২")]);
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
