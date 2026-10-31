//! Splitting an ayah into words, and an ayah's tajweed spans into per-word runs.
//!
//! Two consumers index into the same text and both are position-sensitive, so
//! this is the one place that decides what a "word" is:
//!
//!   * the word-by-word audio CDN, keyed `SSS_AAA_WWW` by word position
//!   * the tajweed annotations, which are codepoint offsets into the ayah
//!
//! `scripts/build-data` explains why the text has to be Tanzil's for either to
//! line up. Here the only rule is: words are whitespace-separated, counted from
//! one, and nothing is dropped — dropping a token would silently shift every
//! audio key after it.

use std::{collections::HashMap, fs};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// A run of characters within a word that share a tajweed rule (or none).
/// Kept as text rather than as offsets because the caller renders it: Arabic
/// shaping depends on neighbouring letters, so the consumer has to lay a whole
/// word out in ONE text run and colour inside it, never as separate items.
#[derive(Debug, Clone, Serialize)]
pub struct Segment {
    pub t: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Word {
    /// 1-based position, which is also the word's audio key.
    pub i: usize,
    pub text: String,
    /// `001_001_001` — resolve against `https://audio.qurancdn.com/wbw/`.
    pub audio: String,
    pub segments: Vec<Segment>,
    /// English and Bangla meaning, transliteration and root letters ("س م و"),
    /// from words.json. All null without it; `root` is also null for a particle.
    pub en: Option<String>,
    pub bn: Option<String>,
    pub tr: Option<String>,
    pub root: Option<String>,
    /// The word read alone, in Bangla letters.
    pub bn_pron: Option<String>,
}

/// One word as `scripts/build-data` stores it: as `scripts/fetch-words` built
/// it, plus its Bangla meaning.
#[derive(Debug, Deserialize)]
pub struct Extra {
    pub text: String,
    pub en: Option<String>,
    #[serde(default)]
    pub bn: Option<String>,
    pub tr: Option<String>,
    pub root: Option<String>,
    pub audio: String,
    #[serde(default)]
    pub bn_pron: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AyahExtra {
    pub juz: Option<u8>,
    pub words: Vec<Extra>,
}

/// words.json keyed `"s:a"`. A missing file is not an error: the reader then
/// shows v1's tokens without meanings.
pub fn load_extras() -> Result<HashMap<String, AyahExtra>> {
    let path = crate::data::words_path();
    let Ok(raw) = fs::read_to_string(&path) else {
        return Ok(HashMap::new());
    };
    serde_json::from_str(&raw).with_context(|| format!("{} is not valid words data", path.display()))
}

/// Add meanings to v1's tokens. v1's tokens (with their tajweed runs) stay
/// when the counts agree, since the two sources split words alike then. When
/// they differ, an index join would shift every meaning, so words.json's own
/// words replace them.
pub fn merge(v1: Vec<Word>, extra: Option<&AyahExtra>) -> Vec<Word> {
    let Some(extra) = extra else { return v1 };
    if v1.len() == extra.words.len() {
        return v1
            .into_iter()
            .zip(&extra.words)
            .map(|(w, e)| Word {
                en: e.en.clone(),
                bn: e.bn.clone(),
                tr: e.tr.clone(),
                root: e.root.clone(),
                bn_pron: e.bn_pron.clone(),
                ..w
            })
            .collect();
    }
    extra
        .words
        .iter()
        .enumerate()
        .map(|(k, e)| Word {
            i: k + 1,
            text: e.text.clone(),
            audio: e.audio.clone(),
            segments: vec![Segment { t: e.text.clone(), r: None }],
            en: e.en.clone(),
            bn: e.bn.clone(),
            tr: e.tr.clone(),
            root: e.root.clone(),
            bn_pron: e.bn_pron.clone(),
        })
        .collect()
}

/// An ayah as the reader gets it: the stored fields plus its juz.
pub fn ayah_json(ayah: &crate::data::Ayah, extra: Option<&AyahExtra>) -> Value {
    let mut v = json!(ayah);
    v["juz"] = json!(extra.and_then(|e| e.juz));
    v
}

/// `[start, end, rule_index]` triples as stored, plus the rule dictionary.
pub fn split(surah: u16, ayah: u16, text: &str, taj: &[[usize; 3]], rules: &[String]) -> Vec<Word> {
    // A per-character rule map is the simplest thing that survives overlapping
    // and adjacent annotations; later spans win, which matches how the source
    // orders them from broad to narrow.
    let chars: Vec<char> = text.chars().collect();
    let mut marks: Vec<Option<usize>> = vec![None; chars.len()];
    for span in taj {
        let (start, end, rule) = (span[0], span[1].min(chars.len()), span[2]);
        for slot in marks.iter_mut().take(end).skip(start) {
            *slot = Some(rule);
        }
    }

    let mut out = Vec::new();
    let mut i = 0usize; // char cursor
    let mut n = 0usize; // word number
    while i < chars.len() {
        if chars[i].is_whitespace() {
            i += 1;
            continue;
        }
        let start = i;
        while i < chars.len() && !chars[i].is_whitespace() {
            i += 1;
        }
        n += 1;

        // Run-length encode the word's rule map into segments.
        let mut segments: Vec<Segment> = Vec::new();
        for pos in start..i {
            let rule = marks[pos].map(|r| rules[r].clone());
            match segments.last_mut() {
                Some(last) if last.r == rule => last.t.push(chars[pos]),
                _ => segments.push(Segment {
                    t: chars[pos].to_string(),
                    r: rule,
                }),
            }
        }

        out.push(Word {
            i: n,
            text: chars[start..i].iter().collect(),
            audio: format!("{surah:03}_{ayah:03}_{n:03}"),
            segments,
            en: None,
            bn: None,
            tr: None,
            root: None,
            bn_pron: None,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules() -> Vec<String> {
        vec!["hamzat_wasl".into(), "madd_2".into()]
    }

    #[test]
    fn counts_words_from_one_and_keys_the_audio() {
        let w = split(1, 1, "بِسْمِ ٱللَّهِ", &[], &rules());
        assert_eq!(w.len(), 2);
        assert_eq!(w[0].i, 1);
        assert_eq!(w[0].audio, "001_001_001");
        assert_eq!(w[1].audio, "001_001_002");
    }

    #[test]
    fn pads_the_key_to_three_digits() {
        let w = split(114, 6, "مِنَ", &[], &rules());
        assert_eq!(w[0].audio, "114_006_001");
    }

    #[test]
    fn a_span_splits_its_word_into_runs() {
        // "ٱللَّهِ" with hamzat_wasl on the leading alef only.
        let text = "ٱللَّهِ";
        let w = split(1, 1, text, &[[0, 1, 0]], &rules());
        assert_eq!(w.len(), 1);
        assert_eq!(w[0].segments.len(), 2);
        assert_eq!(w[0].segments[0].t, "ٱ");
        assert_eq!(w[0].segments[0].r.as_deref(), Some("hamzat_wasl"));
        assert_eq!(w[0].segments[1].r, None);
    }

    #[test]
    fn segments_reassemble_into_the_word() {
        let text = "بِسْمِ ٱللَّهِ";
        let w = split(1, 1, text, &[[7, 9, 1]], &rules());
        for word in &w {
            let joined: String = word.segments.iter().map(|s| s.t.as_str()).collect();
            assert_eq!(joined, word.text, "segments must lose nothing");
        }
    }

    #[test]
    fn whitespace_runs_do_not_create_empty_words() {
        let w = split(1, 1, "  بِسْمِ   ٱللَّهِ  ", &[], &rules());
        assert_eq!(w.len(), 2);
        assert_eq!(w[1].i, 2);
    }
}
