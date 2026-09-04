//! Turning Uthmani script and ASR output into forms that can be compared.
//!
//! The Uthmani text carries harakat, the superscript (dagger) alef, the waqf
//! and sajda annotation signs and alef wasla — none of which any speech model
//! emits — while the model has its own orthographic habits. Compare the two
//! raw and every word is a mistake, which for a beginner is worse than having
//! no tool at all.

/// Marks that decorate the script and are never spoken as separate units.
fn is_mark(c: char) -> bool {
    matches!(c as u32,
        0x064B..=0x065F   // harakat and the rest of the diacritic block
        | 0x0670          // superscript (dagger) alef
        | 0x06D6..=0x06ED // waqf, sajda and the other Quranic annotation signs
        | 0x0640          // tatweel — pure elongation of the joining stroke
        | 0x08D3..=0x08FF // Arabic Extended-A combining marks
    )
}

/// Fold the distinctions the script makes but the tongue does not.
fn fold(c: char) -> char {
    match c {
        // Every alef carrier: hamza above/below, madda, and alef wasla (ٱ),
        // which is what the Uthmani text uses word-initially.
        'آ' | 'أ' | 'إ' | 'ٱ' | 'ٲ' | 'ٳ' | 'ٵ' => 'ا',
        'ى' => 'ي', // alef maqsura
        'ة' => 'ه', // ta marbuta — models spell it either way
        'ؤ' | 'ئ' => 'ء', // hamza carriers
        other => other,
    }
}

/// One word, reduced to its comparable form. Empty if the token was pure
/// punctuation or an ayah-end marker.
pub fn word(raw: &str) -> String {
    raw.chars()
        .filter(|&c| !is_mark(c))
        .map(fold)
        .filter(|c| c.is_alphanumeric())
        .collect()
}

/// A whole phrase reduced for matching, spaces and all.
///
/// `word` is the unit the recitation checker compares and it drops everything
/// between words; searching a corpus is a substring question, so this keeps one
/// space where the text had any run of them and drops punctuation, which is how
/// a phrase typed without harakat finds the vowelled text it came from.
pub fn phrase(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut pending_space = false;
    for c in text.chars().filter(|&c| !is_mark(c)).map(fold) {
        if c.is_whitespace() {
            pending_space = !out.is_empty();
        } else if c.is_alphanumeric() {
            if pending_space {
                out.push(' ');
                pending_space = false;
            }
            out.push(c);
        }
    }
    out
}

/// Split into (original, normalised) pairs, dropping tokens that normalise
/// away. The original is kept so the UI can render the vowelled word it is
/// talking about rather than the stripped one used for matching.
pub fn tokens(text: &str) -> Vec<(String, String)> {
    text.split_whitespace()
        .map(|t| (t.to_string(), word(t)))
        .filter(|(_, n)| !n.is_empty())
        .collect()
}

/// The word with every alef removed.
///
/// THE DAGGER ALEF IS AMBIGUOUS AGAINST MODERN ORTHOGRAPHY AND NO RULE
/// RESOLVES IT. `ٱلرَّحْمَٰن` is written `الرحمن` with no alef, but
/// `ٱلْعَٰلَمِين` and `ٱلصِّرَٰط` are written `العالمين` and `الصراط` *with*
/// one. Deleting the dagger alef is right for the first and wrong for the
/// other two; converting it to an alef is wrong for the first. Either choice
/// makes three of Al-Fatiha's seven ayat report a mistake the reciter did not
/// make — and telling someone their recitation is wrong when it is right is
/// the one failure this tool cannot afford. So neither side of the comparison
/// gets an opinion: matching ignores alefs entirely.
fn skeleton(w: &str) -> String {
    w.chars().filter(|&c| c != 'ا').collect()
}

/// Whether two normalised words count as the same recitation.
pub fn same(a: &str, b: &str) -> bool {
    a == b || skeleton(a) == skeleton(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_harakat_and_annotation() {
        assert_eq!(word("ٱلْحَمْدُ"), "الحمد");
        assert_eq!(word("لِلَّهِ"), "لله");
    }

    #[test]
    fn dagger_alef_both_directions() {
        // Written without an alef in modern spelling …
        assert!(same(&word("ٱلرَّحْمَٰنِ"), &word("الرحمن")));
        // … and with one. Both must pass, which is why matching ignores alefs.
        assert!(same(&word("ٱلْعَٰلَمِينَ"), &word("العالمين")));
        assert!(same(&word("ٱلصِّرَٰطَ"), &word("الصراط")));
    }

    #[test]
    fn real_mistakes_still_caught() {
        assert!(!same(&word("ٱلْمُسْتَقِيمَ"), &word("المستعين")));
        assert!(!same(&word("نَعْبُدُ"), &word("نستعين")));
    }

    #[test]
    fn drops_markers_but_keeps_words() {
        // The BOM rides along on 1:1 from the upstream text.
        let t = tokens("\u{feff}بِسْمِ ٱللَّهِ");
        assert_eq!(t.len(), 2);
        assert_eq!(t[0].1, "بسم");
    }
}
