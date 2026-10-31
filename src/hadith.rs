//! The hadith corpus, on disk, searched by streaming rather than by parsing.
//!
//! Ten collections in Arabic, English and Bangla — about 36,000 narrations and
//! 118 MB — installed once by `scripts/fetch-hadith` as one JSONL file per
//! collection. THE SIZE IS WHAT SHAPES THIS MODULE. The Quran file and the dua
//! file are read whole and answered in one call; that pattern dies here, so
//! there is no `hadith list` and nothing ever deserialises the corpus. A search
//! streams the bytes, matches on them, and parses only the lines that hit.
//!
//! Nothing here grades a hadith or decides which narration answers a question.
//! It finds text, and hands back the collection, the number, every grader's
//! verdict under that grader's name, and a sunnah.com URL that settles it.

use std::{
    collections::HashMap,
    fs::File,
    io::{BufRead, BufReader},
    path::PathBuf,
};

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::normalise;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Grade {
    /// The scholar who graded it. Never dropped: the graders disagree, and a
    /// verdict without its author is an opinion pretending to be a fact.
    pub by: String,
    pub grade: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Hadith {
    /// The book's number, as a string — a few are fractional (`402.2`).
    pub n: String,
    pub ar: String,
    pub en: String,
    /// The Bangla cleaned of the edition's heading, chain stubs and
    /// references; `bn_raw` is the edition's text as it came.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub bn: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub bn_raw: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub grades: Vec<Grade>,
    /// Section number within the collection, and the number within that
    /// section. Passed through as-is rather than typed, because they are the
    /// upstream's shape and this program has no opinion about them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub book: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub in_book: Option<Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Collection {
    pub slug: String,
    pub name: String,
    pub hadiths: usize,
    /// How many have a Bangla translation, and how many were dropped for not
    /// having one — an entry with no Bengali script in it is a chapter heading,
    /// not a translation.
    pub bengali: usize,
    pub bengali_untranslated: usize,
    pub graded: usize,
    #[serde(default)]
    pub sections: HashMap<String, String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Manifest {
    pub source: Value,
    pub collections: Vec<Collection>,
}

#[derive(Debug, Serialize)]
pub struct Hit {
    pub collection: String,
    pub name: String,
    /// The section title this narration sits under, when the collection has
    /// them — context a bare number does not give.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub section: String,
    /// Where anyone can check it, in one click.
    pub url: String,
    pub hadith: Hadith,
}

#[derive(Debug, Serialize)]
pub struct Search {
    pub query: String,
    /// Everything that matched, which is usually more than was returned.
    pub total: usize,
    pub hits: Vec<Hit>,
}

/// Where the corpus lives. `DEEN_HADITH` overrides.
pub fn dir() -> PathBuf {
    if let Ok(p) = std::env::var("DEEN_HADITH") {
        return PathBuf::from(p);
    }
    let base = std::env::var("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".local/share")
        });
    base.join("deen/hadith")
}

pub fn manifest() -> Result<Manifest> {
    let path = dir().join("collections.json");
    let raw = std::fs::read_to_string(&path).with_context(|| {
        format!(
            "cannot read the hadith corpus at {} — run the installer, or set DEEN_HADITH",
            path.display()
        )
    })?;
    serde_json::from_str(&raw).with_context(|| format!("{} is not valid deen hadith data", path.display()))
}

impl Manifest {
    pub fn collection(&self, slug: &str) -> Result<&Collection> {
        self.collections
            .iter()
            .find(|c| c.slug == slug)
            .ok_or_else(|| {
                let names: Vec<&str> = self.collections.iter().map(|c| c.slug.as_str()).collect();
                anyhow!("no collection {slug:?} — have {}", names.join(", "))
            })
    }
}

impl Collection {
    fn section_of(&self, h: &Hadith) -> String {
        h.book
            .as_ref()
            .map(|b| b.to_string().trim_matches('"').to_string())
            .and_then(|k| self.sections.get(&k).cloned())
            .unwrap_or_default()
    }

    fn hit(&self, h: Hadith) -> Hit {
        Hit {
            url: format!("https://sunnah.com/{}:{}", self.slug, h.n),
            section: self.section_of(&h),
            collection: self.slug.clone(),
            name: self.name.clone(),
            hadith: h,
        }
    }
}

/// Which alphabet the query is in, which is all the language detection a
/// three-language corpus needs: type Bangla, search the Bangla.
enum Script {
    Arabic,
    Bengali,
    Latin,
}

fn script_of(s: &str) -> Script {
    if s.chars().any(|c| ('\u{0600}'..='\u{06FF}').contains(&c)) {
        Script::Arabic
    } else if s.chars().any(|c| ('\u{0980}'..='\u{09FF}').contains(&c)) {
        Script::Bengali
    } else {
        Script::Latin
    }
}

/// The one field a query in this script could possibly match.
///
/// An Arabic query cannot hit the English or the Bangla, and normalising them
/// anyway is most of the work: cutting the haystack to the Arabic took a
/// three-word Arabic phrase from 1.5 s to well under a second over 36,000
/// narrations. Same reasoning for the other two, and it is more correct as
/// well — an English word that happens to appear inside a Bangla transliteration
/// is not a hit in the English text.
///
/// COUPLED TO THE WRITER: `scripts/join-hadith` emits `n`, `ar`, `en`, then an
/// optional `bn` and `bn_raw`, and only then the metadata.
fn field_span<'a>(line: &'a str, key: &str) -> &'a str {
    let open = format!("\"{key}\":\"");
    let Some(start) = line.find(&open).map(|i| i + open.len()) else {
        return "";
    };
    let rest = &line[start..];
    let end = ["\",\"ar\":\"", "\",\"en\":\"", "\",\"bn\":\"", "\",\"bn_raw\":\"", "\",\"grades\":", "\",\"book\":", "\",\"in_book\":"]
        .iter()
        .filter_map(|k| rest.find(k))
        .min()
        .unwrap_or_else(|| rest.rfind("\"}").unwrap_or(rest.len()));
    &rest[..end]
}

/// Reduce a query to the runs of text that must all appear.
///
/// EVERY WORD MUST APPEAR, not the whole string as one run. A corpus search
/// where `forgiveness patience` finds nothing because no translator happened to
/// write those two words side by side makes you guess the translator's
/// phrasing — and these are translations, so there is no phrasing to guess.
/// Quoting the query asks for the exact run instead.
fn terms_of(query: &str, script: &Script) -> Vec<String> {
    let prepare = |s: &str| match script {
        // Marks stripped and carriers folded on both sides, so an Arabic phrase
        // typed without harakat finds the vowelled text it was copied from.
        Script::Arabic => normalise::phrase(s),
        Script::Latin => s.to_lowercase(),
        Script::Bengali => s.to_string(),
    };
    let quoted = query.len() > 1 && query.starts_with('"') && query.ends_with('"');
    let raw = if quoted {
        vec![prepare(query.trim_matches('"'))]
    } else {
        prepare(query).split_whitespace().map(str::to_string).collect()
    };
    raw.into_iter().filter(|t| !t.is_empty()).collect()
}

/// The nth narration across the given collections, in the order they are named.
///
/// For the daily pick, and it reads one line rather than a file: the count is in
/// the manifest, so which collection holds `index` is arithmetic, and the line
/// is taken by position without parsing the ones before it.
pub fn nth(slugs: &[&str], index: usize) -> Result<Hit> {
    let m = manifest()?;
    let mut remaining = index;
    for slug in slugs {
        let c = m.collection(slug)?;
        if remaining >= c.hadiths {
            remaining -= c.hadiths;
            continue;
        }
        let path = dir().join(format!("{slug}.jsonl"));
        let file = File::open(&path).with_context(|| format!("cannot read {}", path.display()))?;
        let line = BufReader::new(file)
            .lines()
            .nth(remaining)
            .transpose()?
            .ok_or_else(|| anyhow!("{slug} has fewer than {} lines", remaining + 1))?;
        let h: Hadith = serde_json::from_str(&line)
            .with_context(|| format!("line {} of {slug} is malformed", remaining + 1))?;
        return Ok(c.hit(h));
    }
    bail!("index {index} is past the end of {}", slugs.join(", "))
}

/// How many narrations those collections hold between them.
pub fn count_of(slugs: &[&str]) -> Result<usize> {
    let m = manifest()?;
    slugs.iter().map(|s| m.collection(s).map(|c| c.hadiths)).sum()
}

/// One hadith by collection and number: `bukhari:5010`.
pub fn show(slug: &str, n: &str) -> Result<Hit> {
    let m = manifest()?;
    let c = m.collection(slug)?;
    let path = dir().join(format!("{slug}.jsonl"));
    let file = File::open(&path).with_context(|| format!("cannot read {}", path.display()))?;

    // The number is the first key on every line, so the line can be found
    // without parsing any of them.
    let prefix = format!("{{\"n\":\"{n}\",");
    for line in BufReader::new(file).lines() {
        let line = line?;
        if line.starts_with(&prefix) {
            let h: Hadith = serde_json::from_str(&line)
                .with_context(|| format!("{slug}:{n} is not a valid record"))?;
            return Ok(c.hit(h));
        }
    }
    bail!("no hadith {slug}:{n} — {} has {} of them", c.name, c.hadiths)
}

pub fn search(query: &str, only: Option<&str>, limit: usize) -> Result<Search> {
    let query = query.trim();
    let script = script_of(query);
    let terms = terms_of(query, &script);
    if terms.is_empty() {
        bail!("nothing to search for");
    }

    let m = manifest()?;
    let mut hits: Vec<(usize, usize, Hit)> = Vec::new();
    let mut total = 0;

    for (order, c) in m.collections.iter().enumerate() {
        if only.is_some_and(|s| s != c.slug) {
            continue;
        }
        let path = dir().join(format!("{}.jsonl", c.slug));
        let file = File::open(&path).with_context(|| format!("cannot read {}", path.display()))?;
        for line in BufReader::new(file).lines() {
            let line = line?;
            let hay = match script {
                Script::Arabic => normalise::phrase(field_span(&line, "ar")),
                Script::Latin => field_span(&line, "en").to_lowercase(),
                Script::Bengali => field_span(&line, "bn").to_string(),
            };
            // Sum the mentions, but only once every term is present: a
            // narration that says "patience" nine times and never says
            // "forgiveness" is not an answer to both.
            let mut score = 0;
            let mut all = true;
            for t in &terms {
                let n = hay.matches(t.as_str()).count();
                if n == 0 {
                    all = false;
                    break;
                }
                score += n;
            }
            if !all {
                continue;
            }
            total += 1;
            // Only now is anything parsed, which is what keeps a search over
            // 36,000 narrations off the JSON parser entirely.
            let h: Hadith = serde_json::from_str(&line)
                .with_context(|| format!("a record in {} is malformed", c.slug))?;
            hits.push((score, order, c.hit(h)));
        }
    }

    // Most mentions first; then the collections in the order the manifest lists
    // them, which is the order they are conventionally cited in.
    hits.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    hits.truncate(limit);

    Ok(Search {
        query: query.to_string(),
        total,
        hits: hits.into_iter().map(|(_, _, h)| h).collect(),
    })
}

/// Parse `bukhari:5010`.
pub fn parse_ref(s: &str) -> Result<(String, String)> {
    let (slug, n) = s
        .split_once(':')
        .ok_or_else(|| anyhow!("expected a reference like bukhari:5010, got {s:?}"))?;
    if slug.trim().is_empty() || n.trim().is_empty() {
        bail!("expected a reference like bukhari:5010, got {s:?}");
    }
    Ok((slug.trim().to_string(), n.trim().to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_references() {
        assert_eq!(parse_ref("bukhari:5010").unwrap(), ("bukhari".into(), "5010".into()));
        // Fractional numbers are real and must survive as text.
        assert_eq!(parse_ref("bukhari:402.2").unwrap(), ("bukhari".into(), "402.2".into()));
        assert!(parse_ref("bukhari").is_err());
        assert!(parse_ref("bukhari:").is_err());
    }

    #[test]
    fn a_query_is_all_of_its_words_unless_it_is_quoted() {
        assert_eq!(terms_of("Seeking Forgiveness", &Script::Latin), vec!["seeking", "forgiveness"]);
        assert_eq!(terms_of("\"seeking forgiveness\"", &Script::Latin), vec!["seeking forgiveness"]);
        // Harakat come off both sides, so a phrase pasted from the reader finds
        // the corpus copy of it.
        // Note the alef survives: alef-blindness is a rule for comparing a
        // recitation against the Quran, not for searching written text, and
        // both sides here are written.
        assert_eq!(terms_of("إِنَّمَا الأَعْمَالُ", &Script::Arabic), vec!["انما", "الاعمال"]);
        assert!(terms_of("   ", &Script::Latin).is_empty());
    }

    #[test]
    fn a_field_is_read_without_its_neighbours() {
        let line = r#"{"n":"1","ar":"عربي","en":"the text","bn":"বাংলা","grades":[{"by":"Al-Albani","grade":"Sahih"}],"book":42}"#;
        assert_eq!(field_span(line, "ar"), "عربي");
        assert_eq!(field_span(line, "en"), "the text");
        assert_eq!(field_span(line, "bn"), "বাংলা");
        // The raw Bangla after the cleaned is not part of it.
        let raw = r#"{"n":"1","ar":"عربي","en":"the text","bn":"বাংলা","bn_raw":"শিরোনাম ১. বাংলা","book":1}"#;
        assert_eq!(field_span(raw, "bn"), "বাংলা");
        // A collection with no Bangla answers nothing rather than the next field.
        let no_bn = r#"{"n":"1","ar":"عربي","en":"the text"}"#;
        assert_eq!(field_span(no_bn, "bn"), "");
        assert_eq!(field_span(no_bn, "en"), "the text");
    }

}
