//! Voweled Arabic written in Bangla letters, the way the Bangla Hisnul Muslim
//! writes it: ث ص স, ض দ, ق ক্ব, ط ত্ব, ذ ز ظ য, ع ‘, a hamza ’ inside a word
//! and nothing at its start, a hyphen after long ā, no hasanta on a plain
//! sukun, and the words of an ayah fused where hamzat al-wasl joins them.
//!
//! `ayah` writes a whole ayah as it is recited, ending in its pause form;
//! `word` writes one word as it reads alone, for Read's word line; `surah_name`
//! drafts a surah's Bangla name from its Arabic one. Each refuses a code point
//! it has no rule for, rather than writing an ayah with a letter missing.

use std::{fmt, fs, path::Path};

use anyhow::{anyhow, Context, Result};
use serde_json::Value;

/// A code point the engine has no rule for.
#[derive(Debug, PartialEq)]
pub struct Unknown(pub char);

impl fmt::Display for Unknown {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "U+{:04X}", self.0 as u32)
    }
}

const FATHA: u16 = 1;
const DAMMA: u16 = 1 << 1;
const KASRA: u16 = 1 << 2;
const FATHATAN: u16 = 1 << 3;
const DAMMATAN: u16 = 1 << 4;
const KASRATAN: u16 = 1 << 5;
const SHADDA: u16 = 1 << 6;
const SUKUN: u16 = 1 << 7;
const MADDAH: u16 = 1 << 8;
const HAMZA_ABOVE: u16 = 1 << 9;
/// The dagger alif, a long ā written above its letter.
const DAGGER: u16 = 1 << 10;
/// The small rounded and rectangular zeros: the letter is not read.
const SILENT: u16 = 1 << 11;
/// The small waw and ya after a pronoun's ه, read long only mid-ayah.
const SMALL_WAW: u16 = 1 << 12;
const SMALL_YA: u16 = 1 << 13;
/// The small high noon of 21:88's نُۨجِى, an extra noon read after its letter.
const SMALL_NOON: u16 = 1 << 14;

const VOWELS: u16 = FATHA | DAMMA | KASRA;
const TANWEEN: u16 = FATHATAN | DAMMATAN | KASRATAN;
const HARAKAT: u16 = VOWELS | TANWEEN | SHADDA | SUKUN;

fn mark(c: char) -> Option<u16> {
    Some(match c {
        '\u{064B}' => FATHATAN,
        '\u{064C}' => DAMMATAN,
        '\u{064D}' => KASRATAN,
        '\u{064E}' => FATHA,
        '\u{064F}' => DAMMA,
        '\u{0650}' => KASRA,
        '\u{0651}' => SHADDA,
        // alquran.cloud's surah names write sukun as U+06E1.
        '\u{0652}' | '\u{06E1}' => SUKUN,
        '\u{0653}' => MADDAH,
        '\u{0654}' => HAMZA_ABOVE,
        '\u{0670}' => DAGGER,
        '\u{06DF}' | '\u{06E0}' => SILENT,
        '\u{06E5}' => SMALL_WAW,
        // quran.com's words carry the small ya on a tatweel (إِبْرَٰهِـۧمَ).
        '\u{06E6}' | '\u{06E7}' => SMALL_YA,
        '\u{06E8}' => SMALL_NOON,
        _ => return None,
    })
}

/// Marks that change nothing a Bangla reader is shown: the pause and section
/// signs quran.com's words keep, the iqlab meem (iqlab is read from the
/// letters themselves), the small seens of 2:245 and 52:37, and the imala,
/// ishmam and tas-hil signs of 11:41, 12:11 and 41:44, which books skip.
fn ignored(c: char) -> bool {
    matches!(c, '\u{06D6}'..='\u{06DC}' | '\u{06DE}' | '\u{06E2}' | '\u{06E3}' | '\u{06E9}' | '\u{06EA}'..='\u{06ED}' | '\u{200F}')
}

fn letter(c: char) -> bool {
    matches!(c, '\u{0621}'..='\u{063A}' | '\u{0641}'..='\u{064A}' | '\u{0671}' | '\u{0640}')
}

#[derive(Clone, Copy, Debug)]
struct Unit {
    ch: char,
    m: u16,
}

impl Unit {
    fn has(&self, m: u16) -> bool {
        self.m & m != 0
    }

    fn vowel(&self) -> Option<V> {
        if self.has(FATHA | FATHATAN) {
            Some(V::A)
        } else if self.has(DAMMA | DAMMATAN) {
            Some(V::U)
        } else if self.has(KASRA | KASRATAN) {
            Some(V::I)
        } else {
            None
        }
    }

    fn hamza(&self) -> bool {
        matches!(self.ch, 'ء' | 'أ' | 'إ' | 'ؤ' | 'ئ')
    }
}

/// Words of letters, each letter with the marks written on it.
fn parse(text: &str) -> Result<Vec<Vec<Unit>>, Unknown> {
    let mut words: Vec<Vec<Unit>> = Vec::new();
    let mut cur: Vec<Unit> = Vec::new();
    for c in text.chars() {
        if c.is_whitespace() {
            if !cur.is_empty() {
                words.push(std::mem::take(&mut cur));
            }
        } else if let Some(m) = mark(c) {
            if let Some(u) = cur.last_mut() {
                u.m |= m;
            }
        } else if c == 'آ' {
            // An alif madda after a fatha is that fatha's long ā; anywhere
            // else it is a hamza and a long ā.
            if cur.last().is_some_and(|u| u.has(FATHA)) {
                cur.push(Unit { ch: 'ا', m: MADDAH });
            } else {
                cur.push(Unit { ch: 'ء', m: FATHA });
                cur.push(Unit { ch: 'ا', m: 0 });
            }
        } else if letter(c) {
            cur.push(Unit { ch: c, m: 0 });
        } else if !ignored(c) {
            return Err(Unknown(c));
        }
    }
    if !cur.is_empty() {
        words.push(cur);
    }
    for w in &mut words {
        // A tatweel carrying a hamza is that hamza (شَيْـًٔا); one carrying
        // anything else lends it to the letter before (غِشَـٰوَةٌ).
        let mut k = 0;
        while k < w.len() {
            if w[k].has(HAMZA_ABOVE) {
                w[k] = Unit { ch: 'ء', m: w[k].m & !HAMZA_ABOVE };
            } else if w[k].ch == 'ا' && w[k].has(MADDAH) && (k == 0 || !w[k - 1].has(FATHA)) {
                // ا with a maddah and no fatha before it is آ written apart.
                w[k] = Unit { ch: 'ء', m: FATHA };
                w.insert(k + 1, Unit { ch: 'ا', m: 0 });
            } else if w[k].ch == '\u{0640}' {
                let m = w[k].m;
                w.remove(k);
                if k > 0 {
                    w[k - 1].m |= m;
                }
                continue;
            }
            k += 1;
        }
    }
    words.retain(|w| !w.is_empty());
    Ok(words)
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum V {
    A,
    I,
    U,
    LongA,
    LongI,
    LongU,
    Ai,
    Au,
}

impl V {
    fn long(self) -> V {
        match self {
            V::A => V::LongA,
            V::I => V::LongI,
            V::U => V::LongU,
            v => v,
        }
    }

    fn short(self) -> V {
        match self {
            V::LongA => V::A,
            V::LongI => V::I,
            V::LongU => V::U,
            v => v,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    /// A consonant.
    C(&'static str),
    /// A consonant doubled by shadda.
    D(&'static str),
    /// ي read as a consonant: ইয় at a word's start or after a consonant, য় after a vowel.
    Y,
    /// ‘ for ع, ’ for a hamza, and NUL for a hamza written as nothing.
    G(char),
    /// A vowel; `sila` marks the long vowel of a pronoun's small waw or ya.
    V { v: V, sila: bool },
    /// The noon of tanween.
    N,
    /// ة: ত, or হ in the pause form.
    T,
    /// Written as it is.
    Lit(&'static str),
    /// A muqatta'a's letter names.
    Names(String),
}

fn glyph(ch: char) -> &'static str {
    match ch {
        'ب' => "ব",
        'ت' => "ত",
        'ث' | 'س' | 'ص' => "স",
        'ج' => "জ",
        'ح' | 'ه' => "হ",
        'خ' => "খ",
        'د' | 'ض' => "দ",
        'ذ' | 'ز' | 'ظ' => "য",
        'ر' => "র",
        'ش' => "শ",
        'ط' => "ত্ব",
        'غ' => "গ",
        'ف' => "ফ",
        'ق' => "ক্ব",
        'ك' => "ক",
        'ل' => "ল",
        'م' => "ম",
        'ن' => "ন",
        'و' => "ওয়",
        _ => "",
    }
}

/// The letter names a muqatta'a is read as (الٓمٓ: আলিফ লা-ম মী-ম).
fn letter_name(ch: char) -> Option<&'static str> {
    Some(match ch {
        'ا' => "আলিফ",
        'ل' => "লা-ম",
        'م' => "মী-ম",
        'ص' => "সা-দ",
        'ر' => "রা",
        'ك' => "কা-ফ",
        'ه' => "হা",
        'ي' => "ইয়া",
        'ع' => "‘আইন",
        'ط' => "ত্বা",
        'س' => "সী-ন",
        'ح' => "হা",
        'ق' => "ক্বা-ফ",
        'ن' => "নূ-ন",
        _ => return None,
    })
}

/// A word of bare letters, every one a letter name, with a maddah on one of
/// them or one of the short openings no maddah marks: a muqatta'a.
fn muqattaa(w: &[Unit]) -> Option<String> {
    if w.iter().any(|u| u.has(HARAKAT | DAGGER)) || w.iter().any(|u| letter_name(u.ch).is_none()) {
        return None;
    }
    let bare: String = w.iter().map(|u| u.ch).collect();
    if !w.iter().any(|u| u.has(MADDAH)) && !matches!(bare.as_str(), "طه" | "يس" | "حم" | "ص" | "ق" | "ن") {
        return None;
    }
    Some(w.iter().map(|u| letter_name(u.ch).unwrap()).collect::<Vec<_>>().join(" "))
}

/// How a word's first ٱ is read when it is voiced: আ before the article's
/// lam, উ when the letter after the next is read with a damma, else ই.
fn wasl_vowel(w: &[Unit]) -> V {
    if w.len() > 1 && w[1].ch == 'ل' && !w[1].has(VOWELS) {
        V::A
    } else if w.len() > 2 && w[2].has(DAMMA) {
        V::U
    } else {
        V::I
    }
}

/// A word's first letter is a hamzat al-wasl: ٱ, or a bare alif in texts
/// that do not write the wasla (the surah names).
fn starts_wasl(w: &[Unit]) -> bool {
    w.first().is_some_and(|u| u.ch == 'ٱ' || (u.ch == 'ا' && !u.has(HARAKAT | DAGGER | MADDAH)))
}

/// The lam of اللّٰه, read long: the shadda'd lam after another lam, its ه
/// the word's last letter or followed only by the meem of اللّٰهُمَّ. The ه
/// takes no sukun, so ٱللَّهْوِ is not it.
fn allah_lam(w: &[Unit], i: usize) -> bool {
    i >= 1
        && w[i - 1].ch == 'ل'
        && w[i].ch == 'ل'
        && w[i].has(SHADDA)
        && w[i].has(FATHA)
        && i + 1 < w.len()
        && w[i + 1].ch == 'ه'
        && !w[i + 1].has(SUKUN)
        && (i + 2 == w.len() || (i + 3 == w.len() && w[i + 2].ch == 'م' && w[i + 2].has(SHADDA)))
}

/// One word's tokens. `voiced` reads a first ٱ (an ayah's first word, or a
/// word alone); otherwise it is silent and the word joins the one before.
fn tokens(w: &[Unit], voiced: bool) -> Vec<Tok> {
    let mut out: Vec<Tok> = Vec::new();
    // The vowel just written, None after a consonant with none.
    let mut pv: Option<V> = None;
    let lengthen = |out: &mut Vec<Tok>, pv: &mut Option<V>, to: V| {
        if let Some(Tok::V { v, .. }) = out.iter_mut().rev().find(|t| matches!(t, Tok::V { .. })) {
            *v = to;
        }
        *pv = Some(to);
    };
    for i in 0..w.len() {
        let u = w[i];
        let next = w.get(i + 1);
        if u.has(SILENT) {
            continue;
        }
        if i == 0 && starts_wasl(w) {
            if voiced {
                let v = wasl_vowel(w);
                out.push(Tok::V { v, sila: false });
                pv = Some(v);
            }
            continue;
        }
        match u.ch {
            // ٱ inside a word, after a particle (وَٱلشَّمْسِ), is silent.
            'ٱ' => continue,
            'ا' if !u.has(VOWELS | TANWEEN) => {
                if pv == Some(V::A) || u.has(DAGGER) {
                    lengthen(&mut out, &mut pv, V::LongA);
                }
                continue;
            }
            'ى' | 'ي' | 'و' if !u.has(VOWELS | TANWEEN | SHADDA) => {
                if u.has(DAGGER) {
                    // The carrier of a dagger alif (ٱلصَّلَوٰةَ, عَلَىٰ).
                    lengthen(&mut out, &mut pv, V::LongA);
                    continue;
                }
                match (u.ch, pv) {
                    ('و', Some(V::U)) | ('ى' | 'ي', Some(V::I)) => {
                        let to = pv.unwrap().long();
                        lengthen(&mut out, &mut pv, to);
                        continue;
                    }
                    ('ى', Some(V::A)) if !u.has(SUKUN) => {
                        lengthen(&mut out, &mut pv, V::LongA);
                        continue;
                    }
                    ('ى', _) if i > 0 && w[i - 1].has(FATHATAN) => continue,
                    ('ي' | 'ى', Some(V::A)) if u.has(SUKUN) => {
                        lengthen(&mut out, &mut pv, V::Ai);
                        pv = None;
                        continue;
                    }
                    ('و', Some(V::A)) if u.has(SUKUN) => {
                        lengthen(&mut out, &mut pv, V::Au);
                        pv = None;
                        continue;
                    }
                    _ => {}
                }
            }
            _ => {}
        }
        // A letter with no mark before one with a shadda is not read: the
        // lam of the article before a sun letter, or a letter merged into
        // the next (عَبَدتُّمْ).
        if !u.has(HARAKAT | DAGGER | SMALL_WAW | SMALL_YA) && next.is_some_and(|n| n.has(SHADDA)) && !u.hamza() && u.ch != 'ع' {
            continue;
        }
        // Whether a shadda shows as a doubled letter is the renderer's call:
        // a word's first letter is not doubled unless the word is joined on.
        let doubled = u.has(SHADDA);
        let tok = match u.ch {
            'ع' => Tok::G('‘'),
            // No ’ for a hamza right after a one-letter particle (وَإِيَّاكَ,
            // فَإِنَّ, بِإِذْنِهِۦ): books read it as the word's start.
            _ if u.hamza() && i == 1 && matches!(w[0].ch, 'و' | 'ف' | 'ب' | 'ل' | 'ك') && w[0].has(FATHA | KASRA) => Tok::G('\0'),
            _ if u.hamza() => Tok::G('’'),
            'ة' => Tok::T,
            'ي' | 'ى' if doubled && !out.is_empty() => Tok::Lit(if matches!(pv, Some(V::I | V::LongI)) { "য়্য" } else { "ইয়্য" }),
            'ي' | 'ى' => Tok::Y,
            'و' if doubled && !out.is_empty() => Tok::Lit(match pv {
                Some(V::U) => "ওয়",
                Some(V::A) => "উওয়",
                _ => "ওয়্ওয়",
            }),
            // Iqlab: a noon with no vowel before ب is read م.
            'ن' if !u.has(VOWELS | TANWEEN | SHADDA) && next.is_some_and(|n| n.ch == 'ب') => Tok::C("ম"),
            c if doubled => Tok::D(glyph(c)),
            c => Tok::C(glyph(c)),
        };
        out.push(tok);
        pv = None;
        if u.has(SMALL_NOON) {
            out.push(Tok::C("ন"));
        }
        let mut v = u.vowel();
        if allah_lam(w, i) {
            v = Some(V::LongA);
        }
        if u.has(DAGGER) && !u.has(TANWEEN) {
            v = Some(V::LongA);
        }
        let sila = u.has(SMALL_WAW | SMALL_YA);
        if v.is_none() && u.has(SMALL_WAW) {
            v = Some(V::U);
        }
        if v.is_none() && u.has(SMALL_YA) {
            v = Some(V::I);
        }
        if let Some(mut v) = v {
            if sila {
                v = v.long();
            }
            out.push(Tok::V { v, sila });
            pv = Some(v);
            if u.has(TANWEEN) {
                // Iqlab on tanween is read across the word's end, in `ayah`.
                out.push(Tok::N);
                pv = None;
            }
        }
    }
    out
}

/// The pause form at an ayah's end: a final short vowel, tanween's vowel
/// and a pronoun's small waw or ya go, ـًا is read ā, ة is read হ.
fn pause(toks: &mut Vec<Tok>) {
    match toks.as_slice() {
        [.., Tok::T, Tok::V { .. }, Tok::N] => {
            toks.truncate(toks.len() - 2);
        }
        [.., Tok::V { v: V::A, .. }, Tok::N] => {
            toks.truncate(toks.len() - 2);
            toks.push(Tok::V { v: V::LongA, sila: false });
        }
        [.., Tok::V { .. }, Tok::N] => {
            toks.truncate(toks.len() - 2);
        }
        [.., Tok::V { v: V::A | V::I | V::U, .. }] | [.., Tok::V { sila: true, .. }] => {
            toks.pop();
        }
        _ => {}
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Last {
    Start,
    Cons,
    Vowel,
    Gutt,
}

fn base(g: &str) -> &str {
    match g {
        "ক্ব" => "ক",
        "ত্ব" => "ত",
        g => g,
    }
}

fn render(toks: &[Tok], mut last: Last, pausal: bool, out: &mut String) {
    let mut skip = false;
    for (k, t) in toks.iter().enumerate() {
        if std::mem::take(&mut skip) {
            continue;
        }
        match t {
            Tok::C(g) => {
                out.push_str(g);
                last = Last::Cons;
            }
            Tok::N => {
                out.push('ন');
                last = Last::Cons;
            }
            Tok::T => {
                out.push(if pausal && k == toks.len() - 1 { 'হ' } else { 'ত' });
                last = Last::Cons;
            }
            Tok::D(g) => {
                if last != Last::Start {
                    out.push_str(base(g));
                    out.push('্');
                }
                out.push_str(g);
                last = Last::Cons;
            }
            Tok::Y => {
                let after_vowel = matches!(last, Last::Vowel | Last::Gutt);
                if !after_vowel && matches!(toks.get(k + 1), Some(Tok::V { v: V::U | V::LongU, .. })) {
                    // yu and yū are both written ইউ (ইউসরা-, ইউলাদ).
                    out.push_str("ইউ");
                    skip = true;
                    last = Last::Vowel;
                } else {
                    out.push_str(if after_vowel { "য়" } else { "ইয়" });
                    last = Last::Cons;
                }
            }
            Tok::G('\0') => last = Last::Gutt,
            Tok::G(g) => {
                if *g == '’' && last == Last::Start {
                    continue;
                }
                out.push(*g);
                last = Last::Gutt;
            }
            Tok::Lit(s) => {
                out.push_str(s);
                last = Last::Cons;
            }
            Tok::Names(s) => {
                out.push_str(s);
                last = Last::Vowel;
            }
            Tok::V { v, .. } => {
                let sign = last == Last::Cons;
                out.push_str(match (v, sign) {
                    (V::A | V::LongA | V::Ai | V::Au, true) => "া",
                    (V::A | V::LongA | V::Ai | V::Au, false) => "আ",
                    (V::I, true) => "ি",
                    (V::I, false) => "ই",
                    (V::U, true) => "ু",
                    (V::U, false) => "উ",
                    (V::LongI, true) => "ী",
                    (V::LongI, false) => "ঈ",
                    (V::LongU, true) => "ূ",
                    (V::LongU, false) => "ঊ",
                });
                match v {
                    V::LongA => out.push('-'),
                    V::Ai => out.push('ই'),
                    V::Au => out.push('উ'),
                    _ => {}
                }
                last = Last::Vowel;
            }
        }
    }
}

/// Traditional spellings that win over the rules, as Bangla books print them.
const LEXICON: [(&str, &str); 6] = [
    ("রাব্ব", "রব্ব"),
    ("রাহীম", "রহীম"),
    ("রাহমা-ন", "রহমা-ন"),
    ("ইবরা-হীম", "ইব্রা-হীম"),
    ("মূসা-", "মূসা"),
    ("‘ঈসা-", "ঈসা"),
];

fn lexicon(mut s: String) -> String {
    for (from, to) in LEXICON {
        if s.contains(from) {
            s = s.replace(from, to);
        }
    }
    s
}

/// A noon with no vowel or a tanween at the end of `prev`, before `next`'s
/// first letter: what it is read as (idgham and iqlab spelled as heard).
fn merged(prev: &[Unit], next: &Unit) -> Option<Tok> {
    let last = prev.iter().rev().find(|u| !(matches!(u.ch, 'ا' | 'ى') && !u.has(HARAKAT)))?;
    let noon = (last.ch == 'ن' && !last.has(VOWELS | TANWEEN | SHADDA)) || last.has(TANWEEN);
    if noon {
        return Some(match next.ch {
            'ب' => Tok::C("ম"),
            'ي' => Tok::Lit("ই"),
            'و' => Tok::Lit("উ"),
            'ر' | 'ل' | 'م' | 'ن' => Tok::C(glyph(next.ch)),
            _ => return None,
        });
    }
    // Any other letter left unmarked before a shadda'd one merges into it
    // (قُل رَّبِّ, قَد تَّبَيَّنَ).
    if !last.has(HARAKAT | DAGGER | SMALL_WAW | SMALL_YA) && next.has(SHADDA) && !last.hamza() && glyph(next.ch) != "" {
        return Some(Tok::C(glyph(next.ch)));
    }
    None
}

/// Swap a word's last consonant (its noon, or tanween's) for `with`.
fn replace_last_noon(toks: &mut [Tok], with: Tok) {
    if let Some(t) = toks.iter_mut().rev().find(|t| matches!(t, Tok::C(_) | Tok::N | Tok::D(_))) {
        *t = with;
    }
}

/// A whole ayah as recited: words joined where hamzat al-wasl drops,
/// idgham and iqlab across words, and the pause form at its end.
pub fn ayah(text: &str) -> Result<String, Unknown> {
    let words = parse(text)?;
    let mut toks: Vec<Vec<Tok>> = Vec::new();
    let mut joined: Vec<bool> = Vec::new();
    let mut opaque: Vec<bool> = Vec::new();
    for (k, w) in words.iter().enumerate() {
        if let Some(names) = muqattaa(w) {
            toks.push(vec![Tok::Names(names)]);
            joined.push(false);
            opaque.push(true);
            continue;
        }
        let join = k > 0 && starts_wasl(w) && !opaque[k - 1];
        toks.push(tokens(w, !join));
        joined.push(join);
        opaque.push(false);
    }
    for k in 1..words.len() {
        if opaque[k] || opaque[k - 1] {
            continue;
        }
        if joined[k] {
            // A long vowel before the dropped hamza is read short, and
            // tanween takes a kasra to reach the next word (نُوحٌ ٱبْنَهُ).
            let prev = &mut toks[k - 1];
            match prev.last_mut() {
                Some(Tok::V { v, .. }) => *v = v.short(),
                Some(Tok::N) => prev.push(Tok::V { v: V::I, sila: false }),
                _ => {}
            }
            continue;
        }
        if let Some(with) = merged(&words[k - 1], &words[k][0]) {
            replace_last_noon(&mut toks[k - 1], with);
        }
    }
    if let Some(last) = toks.last_mut() {
        if !opaque.last().copied().unwrap_or(true) {
            pause(last);
        }
    }
    let mut out = String::new();
    for (k, t) in toks.iter().enumerate() {
        let start = if joined[k] {
            Last::Vowel
        } else {
            if k > 0 {
                out.push(' ');
            }
            Last::Start
        };
        render(t, start, k == toks.len() - 1, &mut out);
    }
    Ok(lexicon(out))
}

/// One word as it reads alone: a first ٱ voiced, its last vowel kept.
pub fn word(text: &str) -> Result<String, Unknown> {
    let words = parse(text)?;
    let mut out = String::new();
    for (k, w) in words.iter().enumerate() {
        if k > 0 {
            out.push(' ');
        }
        match muqattaa(w) {
            Some(names) => out.push_str(&names),
            None => render(&tokens(w, true), Last::Start, false, &mut out),
        }
    }
    Ok(lexicon(out))
}

/// A surah's Bangla name drafted from its Arabic one (سُورَةُ الشَّرۡحِ:
/// সূরা আশ-শারহ): each word alone in its pause form, the article set off
/// by a hyphen, as Bangla books name surahs.
pub fn surah_name(text: &str) -> Result<String, Unknown> {
    let mut words = parse(text)?;
    if words.first().is_some_and(|w| w.iter().map(|u| u.ch).eq("سورة".chars())) {
        words.remove(0);
    }
    let mut out = String::from("সূরা");
    for w in &words {
        out.push(' ');
        if let Some(names) = muqattaa(w) {
            out.push_str(&names);
            continue;
        }
        let rest = if starts_wasl(w) && w.len() > 2 && w[1].ch == 'ل' {
            if w[2].has(SHADDA) {
                out.push('আ');
                out.push_str(glyph(w[2].ch));
            } else {
                out.push_str("আল");
            }
            out.push('-');
            &w[2..]
        } else {
            &w[..]
        };
        let mut toks = tokens(rest, true);
        pause(&mut toks);
        render(&toks, Last::Start, true, &mut out);
    }
    Ok(lexicon(out))
}

/// Give every ayah of `dir`'s quran.json and every word of its words.json
/// their `bn_pron`, writing both files back. `scripts/build-data` runs this
/// on the files it has just written, before they replace the old ones.
pub fn write_data(dir: &Path) -> Result<()> {
    let read = |name: &str| -> Result<Value> {
        let path = dir.join(name);
        let raw = fs::read_to_string(&path).with_context(|| format!("cannot read {}", path.display()))?;
        serde_json::from_str(&raw).with_context(|| format!("{} is not JSON", path.display()))
    };
    let no_rule = |at: String, e: Unknown| anyhow!("{at} holds {e}, which the pronunciation has no rule for");

    let mut quran = read("quran.json")?;
    for a in quran["ayahs"].as_array_mut().context("quran.json has no ayahs")? {
        let at = format!("{}:{}", a["s"], a["a"]);
        let pron = ayah(a["ar"].as_str().unwrap_or_default()).map_err(|e| no_rule(at.clone(), e))?;
        if pron.is_empty() {
            return Err(anyhow!("{at} reads as nothing"));
        }
        a["bn_pron"] = Value::from(pron);
    }

    let mut words = read("words.json")?;
    for (at, ayah) in words.as_object_mut().context("words.json is not an object")? {
        for (k, w) in ayah["words"].as_array_mut().into_iter().flatten().enumerate() {
            let pron = word(w["text"].as_str().unwrap_or_default()).map_err(|e| no_rule(format!("{at} word {}", k + 1), e))?;
            if pron.is_empty() {
                return Err(anyhow!("{at} word {} reads as nothing", k + 1));
            }
            w["bn_pron"] = Value::from(pron);
        }
    }

    for (name, v) in [("quran.json", &quran), ("words.json", &words)] {
        fs::write(dir.join(name), v.to_string()).with_context(|| format!("cannot write {name}"))?;
    }
    Ok(())
}

/// A draft of deen-data's surahs-bn.toml from `dir`'s quran.json, each name
/// from `surah_name`, for a person to correct before it is kept.
pub fn draft_names(dir: &Path) -> Result<String> {
    let path = dir.join("quran.json");
    let raw = fs::read_to_string(&path).with_context(|| format!("cannot read {}", path.display()))?;
    let quran: Value = serde_json::from_str(&raw).with_context(|| format!("{} is not JSON", path.display()))?;
    let mut out = String::from("[surahs]\n");
    for s in quran["surahs"].as_array().context("quran.json has no surahs")? {
        let name = surah_name(s["ar"].as_str().unwrap_or_default()).map_err(|e| anyhow!("surah {}'s name holds {e}", s["n"]))?;
        out.push_str(&format!("{} = \"{name}\"  # {}\n", s["n"], s["ar"].as_str().unwrap_or_default()));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn muqattaat_read_as_letter_names() {
        assert_eq!(ayah("الٓمٓ").unwrap(), "আলিফ লা-ম মী-ম");
    }

    #[test]
    fn an_unknown_code_point_is_named() {
        let e = ayah("بِسْمِ ٱللَّهِ\u{08F0}").unwrap_err();
        assert_eq!(e, Unknown('\u{08F0}'));
        assert_eq!(e.to_string(), "U+08F0");
    }

    #[test]
    fn the_basmala_fuses_at_each_wasl() {
        assert_eq!(ayah("بِسْمِ ٱللَّهِ ٱلرَّحْمَٰنِ ٱلرَّحِيمِ").unwrap(), "বিসমিল্লা-হির্রহমা-নির্রহীম");
    }

    #[test]
    fn an_ayah_ends_in_its_pause_form() {
        assert_eq!(ayah("إِنَّ مَعَ ٱلْعُسْرِ يُسْرًا").unwrap(), "ইন্না মা‘আল‘উসরি ইউসরা-");
        assert_eq!(ayah("فَمَن يَعْمَلْ مِثْقَالَ ذَرَّةٍ خَيْرًا يَرَهُۥ").unwrap(), "ফামাই ইয়া‘মাল মিসক্বা-লা যার্রাতিন খাইরাই ইয়ারাহ");
    }

    #[test]
    fn hamzat_al_wasl_is_voiced_at_an_ayahs_start() {
        assert_eq!(ayah("ٱهْدِنَا ٱلصِّرَٰطَ ٱلْمُسْتَقِيمَ").unwrap(), "ইহদিনাস্সিরা-ত্বালমুসতাক্বীম");
        assert_eq!(ayah("ٱلْحَمْدُ لِلَّهِ رَبِّ ٱلْعَٰلَمِينَ").unwrap(), "আলহামদু লিল্লা-হি রব্বিল‘আ-লামীন");
    }

    #[test]
    fn a_word_alone_voices_its_wasl_and_keeps_its_vowel() {
        assert_eq!(word("ٱلْعُسْرِ").unwrap(), "আল‘উসরি");
        assert_eq!(word("يُسْرًۭا").unwrap(), "ইউসরান");
        assert_eq!(word("۞ إِنَّ").unwrap(), "ইন্না");
        assert_eq!(word("إِنَّآ").unwrap(), "ইন্না-");
        assert_eq!(word("آيَةٌ").unwrap(), "আ-য়াতুন");
    }

    #[test]
    fn a_tatweel_carrying_hamza_takes_the_vowel() {
        assert_eq!(word("شَيْـًٔا").unwrap(), "শাই’আন");
    }

    #[test]
    fn the_pronoun_reads_long_with_its_small_waw_or_ya() {
        assert_eq!(word("لَهُۥ").unwrap(), "লাহূ");
        assert_eq!(word("بِهِۦ").unwrap(), "বিহী");
    }

    #[test]
    fn a_small_alif_on_a_carrier_is_long_a() {
        assert_eq!(word("ٱلصَّلَوٰةَ").unwrap(), "আস্সালা-তা");
    }

    #[test]
    fn idgham_and_iqlab_are_spelled_as_heard() {
        assert_eq!(ayah("مِن رَّبِّهِمْ").unwrap(), "মির রব্বিহিম");
        assert_eq!(ayah("وَلَمْ يَكُن لَّهُۥ كُفُوًا أَحَدٌۢ").unwrap(), "ওয়ালাম ইয়াকুল লাহূ কুফুওয়ান আহাদ");
        assert_eq!(ayah("سَمِيعٌۢ بَصِيرٌ").unwrap(), "সামী‘উম বাসীর");
    }

    #[test]
    fn diphthongs_and_doubled_ya() {
        assert_eq!(word("عَلَيْهِمْ").unwrap(), "‘আলাইহিম");
        assert_eq!(word("إِيَّاكَ").unwrap(), "ইয়্যা-কা");
        assert_eq!(word("ٱلْقَيُّومُ").unwrap(), "আলক্বাইয়্যূমু");
        assert_eq!(word("بِشَىْءٍ").unwrap(), "বিশাই’ইন");
        assert_eq!(word("يُولَدْ").unwrap(), "ইউলাদ");
    }

    #[test]
    fn a_hamza_after_a_one_letter_particle_takes_no_mark() {
        assert_eq!(word("وَإِيَّاكَ").unwrap(), "ওয়াইয়্যা-কা");
        assert_eq!(word("سَأَلَ").unwrap(), "সা’আলা");
    }

    #[test]
    fn surah_names_set_the_article_off_with_a_hyphen() {
        assert_eq!(surah_name("سُورَةُ الشَّرۡحِ").unwrap(), "সূরা আশ-শারহ");
        assert_eq!(surah_name("سُورَةُ البَقَرَةِ").unwrap(), "সূরা আল-বাক্বারাহ");
        assert_eq!(surah_name("سُورَةُ \u{0627}\u{0653}لِ عِمۡرَانَ").unwrap(), "সূরা আ-ল ‘ইমরা-ন");
    }
}
