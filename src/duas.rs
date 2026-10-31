//! Hisn al-Muslim, on disk.
//!
//! 268 duas in 132 chapters, built once by `scripts/fetch-duas` and never
//! refreshed, for the same reason the Quran file is not: a book does not
//! change. Nothing in this module composes or selects religious content — it
//! reads what the book says and hands it over with the reference the book
//! prints beneath it.

use std::{fs, path::PathBuf};

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Chapter {
    /// Chapter number, 1-132.
    pub n: u16,
    /// English chapter title, e.g. "Chapter: When waking up".
    pub en: String,
    /// Arabic chapter title.
    pub ar: String,
    /// The Bangla Hisnul Muslim's chapter name, from deen-data's
    /// dua-join.toml. Absent in data built before it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bn: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Dua {
    /// The book's own number — a string, because a few are lettered (75a).
    /// This is the citation, not an index.
    pub id: String,
    pub chapter: u16,
    /// Arabic, as the book prints it.
    pub ar: String,
    /// Transliteration. 255 of the 268 have one.
    pub tr: String,
    /// English translation.
    pub en: String,
    /// The book's reference footnote, shown verbatim wherever the dua is.
    /// Every dua has one but the chapter opener, whose footnote is "ibid.".
    #[serde(rename = "ref")]
    pub reference: String,
    /// The Bangla Hisnul Muslim's meaning, for the 230 duas it covers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bn: Option<String>,
    /// How the Arabic is read, in Bangla letters: that book's own where it has
    /// one, else written by `pron` when the data is built. Absent only in data
    /// built before it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bn_pron: Option<String>,
    /// That book's reference line, for the duas it covers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bn_ref: Option<String>,
    /// How many times to say it, when the joined dataset says more than once.
    /// Absent means the book did not put a number on it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub count: Option<u32>,
    /// Key for `deen audio dua:<id>`. Absent for the 18 duas whose recitation
    /// could not be matched to this text, which get no play button rather than
    /// a guessed one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio: Option<String>,
}

/// Where the duas came from, carried in the file so a screen can show it.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Source {
    pub book: String,
    pub author: String,
    pub url: String,
    /// The second copy of the page every field was checked against at install.
    pub checked_against: String,
    pub counts_from: String,
    pub duas: usize,
    pub with_count_or_audio: usize,
}

#[derive(Debug, Deserialize)]
pub struct Book {
    pub source: Source,
    pub chapters: Vec<Chapter>,
    pub duas: Vec<Dua>,
}

/// Where the book lives. `DEEN_DUAS` overrides, which is what the tests and a
/// terminal use; otherwise the XDG data dir the install script writes to.
pub fn data_path() -> PathBuf {
    if let Ok(p) = std::env::var("DEEN_DUAS") {
        return PathBuf::from(p);
    }
    let base = std::env::var("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".local/share")
        });
    base.join("deen/duas.json")
}

impl Book {
    pub fn load() -> Result<Self> {
        let path = data_path();
        let raw = fs::read_to_string(&path).with_context(|| {
            format!(
                "cannot read the duas at {} — run the installer, or set DEEN_DUAS",
                path.display()
            )
        })?;
        serde_json::from_str(&raw)
            .with_context(|| format!("{} is not valid deen dua data", path.display()))
    }

    pub fn dua(&self, id: &str) -> Result<&Dua> {
        let id = id.trim();
        self.duas
            .iter()
            .find(|d| d.id == id)
            .ok_or_else(|| anyhow!("no dua {id:?} — Hisn al-Muslim numbers them 1 to 267, some lettered"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
        "source": {"book":"Hisn al-Muslim","author":"al-Qahtani","url":"u",
                   "checked_against":"a","counts_from":"c","duas":2,"with_count_or_audio":1},
        "chapters": [{"n":1,"en":"Chapter: When waking up","ar":"أذكار الاستيقاظ"}],
        "duas": [
            {"id":"1","chapter":1,"ar":"الحمد لله","tr":"Alhamdu lillah","en":"Praise is to Allah",
             "ref":"Al-Bukhari 11/113","audio":"1"},
            {"id":"75a","chapter":27,"ar":"الحمد لله وحده","tr":"","en":"All praise","ref":"ibid."}
        ]
    }"#;

    fn book() -> Book {
        serde_json::from_str(SAMPLE).unwrap()
    }

    #[test]
    fn finds_a_dua_by_the_books_number() {
        let b = book();
        assert_eq!(b.dua("1").unwrap().en, "Praise is to Allah");
        // Lettered ids are ids, not indices — a numeric parse would lose them.
        assert_eq!(b.dua("75a").unwrap().chapter, 27);
        assert!(b.dua("75").is_err());
        assert!(b.dua("999").is_err());
    }

    #[test]
    fn a_dua_without_audio_says_so_rather_than_guessing() {
        let b = book();
        assert_eq!(b.dua("1").unwrap().audio.as_deref(), Some("1"));
        assert!(b.dua("75a").unwrap().audio.is_none());
        assert!(b.dua("1").unwrap().count.is_none());
    }
}
