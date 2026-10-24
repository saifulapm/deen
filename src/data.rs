//! The Quran, on disk.
//!
//! One JSON file holding all 114 surahs and all 6236 ayat in Uthmani script
//! with Bangla and English translations, built once by the install script and
//! never refreshed — unlike a film catalogue, the text does not change. That
//! is why there is no HTTP client in this crate and why every command works
//! with the network down.

use std::{fs, path::PathBuf};

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Surah {
    /// Surah number, 1-114.
    pub n: u16,
    /// Arabic name.
    pub ar: String,
    /// Transliterated name, e.g. "Al-Faatiha".
    pub en: String,
    /// Translated name, e.g. "The Opening".
    pub tr: String,
    /// How many ayat it has.
    pub count: u16,
    /// "Meccan" or "Medinan".
    #[serde(rename = "type")]
    pub kind: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Ayah {
    pub s: u16,
    pub a: u16,
    /// Uthmani script, fully vowelled — Tanzil's, and it has to be: see
    /// `scripts/fetch-data` for the two things that index into it.
    pub ar: String,
    /// Bangla — Muhiuddin Khan.
    pub bn: String,
    /// English — Sahih International.
    pub en: String,
    /// Tajweed spans as `[start, end, rule_index]` over `ar`'s codepoints,
    /// indexing into `Quran::rules`.
    #[serde(default)]
    pub taj: Vec<[usize; 3]>,
}

#[derive(Debug, Deserialize)]
pub struct Quran {
    pub surahs: Vec<Surah>,
    /// The tajweed rule names `Ayah::taj` indexes into.
    #[serde(default)]
    pub rules: Vec<String>,
    /// The Basmala, carried once. It heads every surah but Al-Fatiha (where it
    /// is ayah 1) and At-Tawbah (which has none), and `scripts/fetch-data`
    /// strips it out of those ayat so it is not counted as part of them.
    #[serde(default)]
    pub basmala: String,
    pub ayahs: Vec<Ayah>,
}

/// Where the text lives. `DEEN_DATA` overrides, which is what the tests and a
/// terminal use; otherwise the XDG data dir the install script writes to.
pub fn data_path() -> PathBuf {
    if let Ok(p) = std::env::var("DEEN_DATA") {
        return PathBuf::from(p);
    }
    let base = std::env::var("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".local/share")
        });
    base.join("deen/quran.json")
}

/// Where the per-word meanings live. `DEEN_WORDS` overrides; otherwise it sits
/// beside quran.json.
pub fn words_path() -> PathBuf {
    if let Ok(p) = std::env::var("DEEN_WORDS") {
        return PathBuf::from(p);
    }
    data_path().with_file_name("words.json")
}

impl Quran {
    pub fn load() -> Result<Self> {
        let path = data_path();
        let raw = fs::read_to_string(&path).with_context(|| {
            format!(
                "cannot read the Quran text at {} — run the installer, or set DEEN_DATA",
                path.display()
            )
        })?;
        serde_json::from_str(&raw).with_context(|| format!("{} is not valid deen data", path.display()))
    }

    pub fn surah(&self, n: u16) -> Result<&Surah> {
        self.surahs
            .iter()
            .find(|s| s.n == n)
            .ok_or_else(|| anyhow!("no surah {n} — the Quran has 114"))
    }

    pub fn ayat_of(&self, n: u16) -> Vec<&Ayah> {
        self.ayahs.iter().filter(|a| a.s == n).collect()
    }

    pub fn ayah(&self, s: u16, a: u16) -> Result<&Ayah> {
        self.ayahs
            .iter()
            .find(|x| x.s == s && x.a == a)
            .ok_or_else(|| anyhow!("no ayah {s}:{a}"))
    }
}

/// Parse a `surah:ayah` reference such as `1:6`.
pub fn parse_ref(s: &str) -> Result<(u16, u16)> {
    let (a, b) = s
        .split_once(':')
        .ok_or_else(|| anyhow!("expected an ayah reference like 1:6, got {s:?}"))?;
    Ok((
        a.trim()
            .parse()
            .with_context(|| format!("bad surah number in {s:?}"))?,
        b.trim()
            .parse()
            .with_context(|| format!("bad ayah number in {s:?}"))?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_references() {
        assert_eq!(parse_ref("1:6").unwrap(), (1, 6));
        assert_eq!(parse_ref("114:1").unwrap(), (114, 1));
        assert!(parse_ref("1").is_err());
        assert!(parse_ref("x:1").is_err());
    }
}
