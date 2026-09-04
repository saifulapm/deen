//! Memorisation scheduling: which ayat to review, and when next.
//!
//! SM-2, the algorithm behind Anki and SuperMemo, over ayat instead of
//! flashcards. What makes it worth having here rather than as a separate app is
//! that the REVIEW IS THE RECITATION: you are shown a reference, you recite
//! from memory, and `deen recite` scores it. There is no self-report step where
//! you decide whether you knew it, which is the step people lie to themselves
//! at.
//!
//! DUE DATES ARE ABSOLUTE TIMESTAMPS, NOT CALENDAR DAYS. A day boundary needs a
//! timezone, a timezone needs a database, and none of it would buy anything: an
//! ayah reviewed at nine in the evening comes back at nine in the evening some
//! days later, which is what a person actually wants and what a `u64` of
//! seconds already says.

use std::{
    collections::BTreeMap,
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

pub const DAY: u64 = 86_400;

/// How well an attempt went. Anki's four buttons rather than SM-2's 0..5: a
/// person can tell these apart, and the six-point scale asks for a precision
/// nobody has about their own recall.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Grade {
    Again,
    Hard,
    Good,
    Easy,
}

impl Grade {
    pub fn parse(s: &str) -> Result<Self> {
        Ok(match s.to_ascii_lowercase().as_str() {
            "again" | "0" => Grade::Again,
            "hard" | "1" => Grade::Hard,
            "good" | "2" => Grade::Good,
            "easy" | "3" => Grade::Easy,
            other => anyhow::bail!("grade must be again, hard, good or easy — got {other:?}"),
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Grade::Again => "again",
            Grade::Hard => "hard",
            Grade::Good => "good",
            Grade::Easy => "easy",
        }
    }

    /// SM-2's quality score.
    fn quality(self) -> f64 {
        match self {
            Grade::Again => 2.0,
            Grade::Hard => 3.0,
            Grade::Good => 4.0,
            Grade::Easy => 5.0,
        }
    }

    /// What the recitation itself suggests, before the reciter overrides it.
    ///
    /// Deliberately generous at the top. The checker's own error rate is about
    /// one word in thirty on clean audio and worse on a learner's, so demanding
    /// a literal 100% to call something Easy would punish people for the tool's
    /// mistakes. Below half, though, the words were not there.
    pub fn from_accuracy(accuracy: f64) -> Self {
        if accuracy >= 0.97 {
            Grade::Easy
        } else if accuracy >= 0.85 {
            Grade::Good
        } else if accuracy >= 0.5 {
            Grade::Hard
        } else {
            Grade::Again
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Card {
    /// SM-2 easiness. Never below 1.3, or intervals collapse to nothing.
    pub ease: f64,
    /// Consecutive successful reviews. Reset by an `Again`.
    pub reps: u32,
    /// Current spacing, in days.
    pub interval: f64,
    /// Unix seconds when this comes back.
    pub due: u64,
    /// How many times it has been forgotten after being learned.
    pub lapses: u32,
    /// The last recitation's accuracy, for the UI to show a trend.
    #[serde(default)]
    pub last_accuracy: Option<f64>,
}

impl Card {
    fn new(now: u64) -> Self {
        Card {
            ease: 2.5,
            reps: 0,
            interval: 0.0,
            due: now,
            lapses: 0,
            last_accuracy: None,
        }
    }

    fn review(&mut self, grade: Grade, now: u64) {
        let q = grade.quality();
        if grade == Grade::Again {
            // Forgotten. Back to the start of the ladder, but the ease it was
            // learned at is kept — an ayah is not harder because you had a bad
            // morning, and resetting ease as well double-punishes it.
            if self.reps > 0 {
                self.lapses += 1;
            }
            self.reps = 0;
            self.interval = 0.0;
            self.due = now + DAY / 6; // ten minutes, not a day: it is still today's problem
        } else {
            self.interval = match self.reps {
                0 => 1.0,
                1 => 6.0,
                _ => (self.interval * self.ease).max(1.0),
            };
            self.reps += 1;
            self.due = now + (self.interval * DAY as f64) as u64;
        }
        // SM-2's ease update, clamped. Applies on every grade including Again,
        // which is what makes a repeatedly-missed ayah come back faster for
        // ever after.
        self.ease = (self.ease + (0.1 - (5.0 - q) * (0.08 + (5.0 - q) * 0.02))).max(1.3);
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Deck {
    #[serde(default)]
    pub cards: BTreeMap<String, Card>,
}

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn state_path() -> PathBuf {
    if let Ok(p) = std::env::var("DEEN_STATE") {
        return PathBuf::from(p);
    }
    std::env::var("XDG_STATE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".local/state")
        })
        .join("deen/hifz.json")
}

impl Deck {
    pub fn load() -> Result<Self> {
        let path = state_path();
        match fs::read_to_string(&path) {
            Ok(raw) => serde_json::from_str(&raw)
                .with_context(|| format!("{} is not a deen deck", path.display())),
            // No deck yet is the ordinary first-run state, not a failure.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Deck::default()),
            Err(e) => Err(e).with_context(|| format!("cannot read {}", path.display())),
        }
    }

    pub fn save(&self) -> Result<()> {
        let path = state_path();
        let dir = path.parent().context("state path has no parent")?;
        fs::create_dir_all(dir).with_context(|| format!("cannot create {}", dir.display()))?;
        // Write beside and rename: a deck is months of work and a half-written
        // file would lose all of it.
        let tmp = path.with_extension("tmp");
        fs::write(&tmp, serde_json::to_string(self)?)
            .with_context(|| format!("cannot write {}", tmp.display()))?;
        fs::rename(&tmp, &path).with_context(|| format!("cannot place {}", path.display()))?;
        Ok(())
    }

    /// Enrol an ayah. Already-enrolled ayat keep their history.
    pub fn add(&mut self, reference: &str, now: u64) -> bool {
        if self.cards.contains_key(reference) {
            return false;
        }
        self.cards.insert(reference.to_string(), Card::new(now));
        true
    }

    pub fn remove(&mut self, reference: &str) -> bool {
        self.cards.remove(reference).is_some()
    }

    /// Everything due, oldest first — the ayah you have been putting off
    /// longest is the one you most need.
    pub fn due(&self, now: u64) -> Vec<(&String, &Card)> {
        let mut out: Vec<_> = self.cards.iter().filter(|(_, c)| c.due <= now).collect();
        out.sort_by_key(|(_, c)| c.due);
        out
    }

    pub fn review(&mut self, reference: &str, grade: Grade, accuracy: Option<f64>, now: u64) -> Result<&Card> {
        let card = self
            .cards
            .entry(reference.to_string())
            .or_insert_with(|| Card::new(now));
        card.review(grade, now);
        card.last_accuracy = accuracy;
        Ok(card)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const T: u64 = 1_700_000_000;

    #[test]
    fn a_new_card_is_due_at_once() {
        let mut d = Deck::default();
        assert!(d.add("112:1", T));
        assert!(!d.add("112:1", T), "adding twice must not reset it");
        assert_eq!(d.due(T).len(), 1);
    }

    #[test]
    fn the_ladder_is_one_day_then_six() {
        let mut d = Deck::default();
        d.add("112:1", T);
        d.review("112:1", Grade::Good, Some(1.0), T).unwrap();
        assert_eq!(d.cards["112:1"].interval, 1.0);
        d.review("112:1", Grade::Good, Some(1.0), T).unwrap();
        assert_eq!(d.cards["112:1"].interval, 6.0);
        d.review("112:1", Grade::Good, Some(1.0), T).unwrap();
        assert!(d.cards["112:1"].interval > 6.0, "then it multiplies by ease");
    }

    #[test]
    fn again_returns_it_today_and_counts_a_lapse() {
        let mut d = Deck::default();
        d.add("112:1", T);
        d.review("112:1", Grade::Good, Some(1.0), T).unwrap();
        d.review("112:1", Grade::Again, Some(0.2), T).unwrap();
        let c = &d.cards["112:1"];
        assert_eq!(c.reps, 0);
        assert_eq!(c.lapses, 1);
        assert!(c.due < T + DAY, "a forgotten ayah is still today's problem");
    }

    #[test]
    fn a_first_attempt_that_fails_is_not_a_lapse() {
        let mut d = Deck::default();
        d.add("112:1", T);
        d.review("112:1", Grade::Again, Some(0.0), T).unwrap();
        assert_eq!(d.cards["112:1"].lapses, 0, "you cannot forget what you never had");
    }

    #[test]
    fn ease_has_a_floor() {
        let mut d = Deck::default();
        d.add("112:1", T);
        for _ in 0..20 {
            d.review("112:1", Grade::Again, Some(0.0), T).unwrap();
        }
        assert!(d.cards["112:1"].ease >= 1.3);
    }

    #[test]
    fn accuracy_maps_generously_at_the_top() {
        // The checker's own error rate must not cost the reciter a grade.
        assert_eq!(Grade::from_accuracy(1.0), Grade::Easy);
        assert_eq!(Grade::from_accuracy(0.97), Grade::Easy);
        assert_eq!(Grade::from_accuracy(0.90), Grade::Good);
        assert_eq!(Grade::from_accuracy(0.60), Grade::Hard);
        assert_eq!(Grade::from_accuracy(0.20), Grade::Again);
    }

    #[test]
    fn due_is_oldest_first() {
        let mut d = Deck::default();
        d.add("1:1", T);
        d.add("2:1", T);
        d.cards.get_mut("1:1").unwrap().due = T - 100;
        d.cards.get_mut("2:1").unwrap().due = T - 900;
        let due = d.due(T);
        assert_eq!(due[0].0, "2:1", "the one put off longest comes first");
    }
}
