//! Lining a recitation up against the ayah it was meant to be.
//!
//! Plain Levenshtein over words, with the comparison delegated to
//! `normalise::same`. The point of aligning rather than just scoring is that
//! the UI needs to colour individual words — "you missed one here" is useful,
//! "82%" is not.

use serde::Serialize;

use crate::normalise;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Op {
    /// Said correctly.
    Ok,
    /// Said, but it was a different word.
    Wrong,
    /// In the ayah, never said — a skipped word.
    Missed,
    /// Said, but not in the ayah — an insertion.
    Extra,
}

#[derive(Debug, Clone, Serialize)]
pub struct Word {
    pub op: Op,
    /// The vowelled word from the ayah, for display. Empty for `Extra`.
    pub reference: String,
    /// What the model heard. Empty for `Missed`.
    pub heard: String,
    /// Index of this word within the ayah, so the UI can map back onto the
    /// text it is already showing. `None` for `Extra`, which is nowhere.
    pub index: Option<usize>,
}

/// Align heard words against the ayah's words. Both sides are
/// `(original, normalised)` pairs from `normalise::tokens`.
pub fn align(reference: &[(String, String)], heard: &[(String, String)]) -> Vec<Word> {
    let (n, m) = (reference.len(), heard.len());
    let mut d = vec![vec![0usize; m + 1]; n + 1];
    for (i, row) in d.iter_mut().enumerate() {
        row[0] = i;
    }
    for j in 0..=m {
        d[0][j] = j;
    }
    for i in 1..=n {
        for j in 1..=m {
            let cost = usize::from(!normalise::same(&reference[i - 1].1, &heard[j - 1].1));
            d[i][j] = (d[i - 1][j] + 1)
                .min(d[i][j - 1] + 1)
                .min(d[i - 1][j - 1] + cost);
        }
    }

    let (mut i, mut j) = (n, m);
    let mut out = Vec::new();
    while i > 0 || j > 0 {
        let diagonal = i > 0 && j > 0 && {
            let cost = usize::from(!normalise::same(&reference[i - 1].1, &heard[j - 1].1));
            d[i][j] == d[i - 1][j - 1] + cost
        };
        if diagonal {
            let matched = normalise::same(&reference[i - 1].1, &heard[j - 1].1);
            out.push(Word {
                op: if matched { Op::Ok } else { Op::Wrong },
                reference: reference[i - 1].0.clone(),
                heard: heard[j - 1].0.clone(),
                index: Some(i - 1),
            });
            i -= 1;
            j -= 1;
        } else if i > 0 && d[i][j] == d[i - 1][j] + 1 {
            out.push(Word {
                op: Op::Missed,
                reference: reference[i - 1].0.clone(),
                heard: String::new(),
                index: Some(i - 1),
            });
            i -= 1;
        } else {
            out.push(Word {
                op: Op::Extra,
                reference: String::new(),
                heard: heard[j - 1].0.clone(),
                index: None,
            });
            j -= 1;
        }
    }
    out.reverse();
    out
}

/// Correct words over the ayah's length. `Extra` words are not counted in the
/// denominator — reciting an extra word does not make the ayah longer — but
/// they do count against you in `errors`.
pub fn score(words: &[Word]) -> (usize, usize, f64) {
    let correct = words.iter().filter(|w| w.op == Op::Ok).count();
    let total = words.iter().filter(|w| w.op != Op::Extra).count();
    let errors = words.iter().filter(|w| w.op != Op::Ok).count();
    let wer = if total == 0 {
        0.0
    } else {
        errors as f64 / total as f64
    };
    (correct, total, wer)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toks(s: &str) -> Vec<(String, String)> {
        normalise::tokens(s)
    }

    #[test]
    fn perfect_recitation_scores_clean() {
        let r = toks("ٱهْدِنَا ٱلصِّرَٰطَ ٱلْمُسْتَقِيمَ");
        let h = toks("اهدنا الصراط المستقيم");
        let w = align(&r, &h);
        assert!(w.iter().all(|x| x.op == Op::Ok));
        assert_eq!(score(&w).2, 0.0);
    }

    #[test]
    fn substitution_is_located_not_just_counted() {
        let r = toks("ٱهْدِنَا ٱلصِّرَٰطَ ٱلْمُسْتَقِيمَ");
        let h = toks("اهدنا الصراط المستعين");
        let w = align(&r, &h);
        assert_eq!(w[2].op, Op::Wrong);
        assert_eq!(w[2].index, Some(2));
        assert_eq!(score(&w), (2, 3, 1.0 / 3.0));
    }

    #[test]
    fn skipped_word_is_missed_not_wrong() {
        let r = toks("ٱهْدِنَا ٱلصِّرَٰطَ ٱلْمُسْتَقِيمَ");
        let h = toks("اهدنا المستقيم");
        let w = align(&r, &h);
        assert_eq!(w[1].op, Op::Missed);
        assert_eq!(w[1].heard, "");
    }

    #[test]
    fn empty_recitation_misses_everything() {
        let r = toks("ٱهْدِنَا ٱلصِّرَٰطَ");
        let w = align(&r, &[]);
        assert_eq!(w.len(), 2);
        assert!(w.iter().all(|x| x.op == Op::Missed));
        assert_eq!(score(&w).2, 1.0);
    }
}
