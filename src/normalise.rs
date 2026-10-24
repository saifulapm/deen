//! Turning Uthmani script into a form a typed search can match.
//!
//! The Uthmani text carries harakat, the superscript (dagger) alef, the waqf
//! and sajda annotation signs and alef wasla, none of which a person types
//! when searching for a phrase.

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

/// A whole phrase reduced for matching, spaces and all.
///
/// Searching a corpus is a substring question, so this keeps one space where
/// the text had any run of them and drops punctuation, which is how a phrase
/// typed without harakat finds the vowelled text it came from.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_harakat_and_annotation() {
        assert_eq!(phrase("ٱلْحَمْدُ لِلَّهِ"), "الحمد لله");
    }

    #[test]
    fn strips_tatweel_and_keeps_one_space() {
        assert_eq!(phrase("بِاسْـمِكَ  اللّهُـمَّ، أَمـوتُ"), "باسمك اللهم اموت");
    }
}
