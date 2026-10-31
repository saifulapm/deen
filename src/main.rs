//! deen — the Quran, duas and hadith, and a memorisation deck.
//!
//! Every command answers with exactly one JSON object on stdout and puts
//! diagnostics on stderr, so the caller can parse stdout blind. That contract
//! is what lets the shell's hub be a front end for this binary and nothing
//! else, and it is the same contract dekho answers on.
//!
//! It is an ordinary terminal program too:
//!
//!     deen api ayah 1:6 | jq -r .ar
//!     deen hifz due | jq '.due[0].next'

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use serde_json::json;

mod audio;
mod daily;
mod data;
mod duas;
mod fetch;
mod hadith;
mod hifz;
mod normalise;
mod pron;
mod prayer;
mod words;

#[derive(Parser)]
#[command(name = "deen", version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Read the text.
    Api {
        #[command(subcommand)]
        what: ApiCmd,
    },
    /// Fetch recitation audio and print where it landed.
    ///
    /// Caches under ~/.cache/deen/audio, so the second tap on a word is
    /// instant and works offline.
    Audio {
        /// `1:1:1` for one word, `1:1` for a whole ayah, `dua:75` for a dua.
        reference: String,
        /// everyayah.com reciter directory, for whole-ayah audio.
        #[arg(long, default_value = audio::DEFAULT_RECITER)]
        reciter: String,
        /// With `1:5`, print the queue from that ayah to the surah's last:
        /// cached paths where there are any, else URLs. Downloads nothing.
        #[arg(long)]
        to_end: bool,
    },
    /// Daily adhkar, from Hisn al-Muslim, each with the book's reference.
    Dua {
        #[command(subcommand)]
        what: DuaCmd,
    },
    /// The hadith corpus: find a narration, or show one by its number.
    Hadith {
        #[command(subcommand)]
        what: HadithCmd,
    },
    /// Memorisation: what is due, and how it went.
    Hifz {
        #[command(subcommand)]
        what: HifzCmd,
    },
    /// Give every ayah, word and dua in a data directory its Bangla pronunciation.
    ///
    /// `scripts/build-data` runs it on the quran.json, words.json and
    /// duas.json it has just written; it stops, naming the ayah or the dua
    /// and the character, on a code point the pronunciation has no rule for.
    Pron {
        dir: std::path::PathBuf,
        /// Print a draft of deen-data's surahs-bn.toml from the surahs'
        /// Arabic names instead, for a person to correct.
        #[arg(long)]
        names: bool,
    },
    /// Today's prayer times (Hanafi Asr) for the city in the config.
    ///
    /// Caches the month under ~/.cache/deen/prayer, so it works offline.
    Prayer {
        /// YYYY-MM-DD; defaults to today, local time.
        #[arg(long)]
        date: Option<String>,
        /// Fetch the month again even when it is cached.
        #[arg(long)]
        refresh: bool,
    },
}

#[derive(Subcommand)]
enum DuaCmd {
    /// The whole book: where it came from, its 132 chapters, its 268 duas.
    ///
    /// One call rather than one per chapter, for the reason `api surah` sends
    /// the words with the ayat: a screen that browses this shows a chapter the
    /// moment it is picked, and 250 KB read once beats a process per tap.
    List,
    /// One dua by the book's own number, e.g. `27` or `75a`.
    Show { id: String },
}

#[derive(Subcommand)]
enum HadithCmd {
    /// The collections, with their section titles and what each one carries.
    ///
    /// There is no `list` beside it and there will not be: 36,000 narrations
    /// is past the size where a screen can be handed the whole book, which is
    /// the pattern `dua list` uses and this one cannot.
    Collections,
    /// Search the corpus. The script you type in picks the language.
    Search {
        /// Words, unquoted — they are joined back into one phrase.
        #[arg(required = true, num_args = 1..)]
        query: Vec<String>,
        /// Restrict to one collection, e.g. `bukhari`.
        #[arg(long)]
        collection: Option<String>,
        #[arg(long, default_value_t = 40)]
        limit: usize,
    },
    /// One narration: `bukhari:5010`.
    Show { reference: String },
    /// Download the editions and build the corpus, resuming a cut download.
    ///
    /// Prints one JSON event per line: progress, converting, then done or
    /// error. The output is the same with or without --json.
    Fetch {
        #[arg(long)]
        json: bool,
    },
    /// Whether the corpus is missing, partial or ready, without the network.
    Status,
}

#[derive(Subcommand)]
enum HifzCmd {
    /// Enrol an ayah (`1:6`) or a whole surah (`112`).
    Add { reference: String },
    /// Drop an ayah, losing its history.
    Remove { reference: String },
    /// Everything due now, oldest first, with the text to review.
    Due {
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
    /// Record how an attempt went: again | hard | good | easy.
    Grade {
        reference: String,
        grade: String,
    },
    /// Counts: enrolled, due, and how far along.
    Stats,
}

#[derive(Subcommand)]
enum ApiCmd {
    /// Every surah, with its name and length.
    Surahs,
    /// One surah and all of its ayat.
    Surah { n: u16 },
    /// One ayah.
    Ayah { reference: String },
    /// Today's ayah and today's narration, chosen by the date.
    Daily {
        /// Your LOCAL date as YYYY-MM-DD. Without it the day is UTC's, which
        /// rolls over at a strange hour east of Greenwich.
        #[arg(long)]
        date: Option<String>,
    },
}

fn main() {
    if let Err(e) = run() {
        // The object on stdout keeps the contract even on the failure path —
        // a caller that only ever parses stdout still gets JSON — while the
        // human-facing message and the exit code go where they belong.
        println!("{}", json!({ "error": e.to_string() }));
        eprintln!("deen: {e:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    match Cli::parse().command {
        Cmd::Api { what } => api(what),
        Cmd::Audio {
            reference,
            reciter,
            to_end,
        } => do_audio(&reference, &reciter, to_end),
        Cmd::Dua { what } => do_dua(what),
        Cmd::Hadith { what } => do_hadith(what),
        Cmd::Hifz { what } => do_hifz(what),
        Cmd::Prayer { date, refresh } => prayer::run(date, refresh),
        Cmd::Pron { dir, names: true } => {
            print!("{}", pron::draft_names(&dir)?);
            Ok(())
        }
        Cmd::Pron { dir, names: false } => {
            pron::write_data(&dir)?;
            println!("{}", json!({ "ok": true }));
            Ok(())
        }
    }
}

fn api(what: ApiCmd) -> Result<()> {
    let quran = data::Quran::load()?;
    let out = match what {
        ApiCmd::Surahs => json!({ "surahs": quran.surahs }),
        ApiCmd::Surah { n } => {
            // Words come with the surah rather than one `api ayah` per row:
            // a reader shows every ayah at once, and Al-Baqarah would
            // otherwise be 286 processes to paint one screen.
            let ayahs = quran.ayat_of(n);
            let extras = words::load_extras()?;
            let extra = |a: &data::Ayah| extras.get(&format!("{}:{}", a.s, a.a));
            let words: Vec<_> = ayahs
                .iter()
                .map(|a| words::merge(words::split(a.s, a.a, &a.ar, &a.taj, &quran.rules), extra(a)))
                .collect();
            let ayahs: Vec<_> = ayahs.iter().map(|a| words::ayah_json(a, extra(a))).collect();
            json!({
                "surah": quran.surah(n)?,
                "ayahs": ayahs,
                "words": words,
                // Al-Fatiha opens with it as ayah 1 and At-Tawbah has none, so
                // those two are the only surahs a reader must not head with it.
                "basmala": if n == 1 || n == 9 { "" } else { &quran.basmala },
            })
        }
        ApiCmd::Daily { date } => {
            let date = date.unwrap_or_else(daily::utc_today);
            let day = daily::day_number(&date)?;

            let ayah = &quran.ayahs[daily::index_for(day, quran.ayahs.len())];
            // THE DAILY NARRATION COMES FROM THE TWO SAHIHS ONLY. Search reaches
            // the whole corpus because you went looking for something; a
            // narration that arrives unasked, on the day's page, should not be
            // one the reader has to weigh a grading dispute about.
            const DAILY_BOOKS: [&str; 2] = ["bukhari", "muslim"];
            let hadith = hadith::count_of(&DAILY_BOOKS)
                .and_then(|n| hadith::nth(&DAILY_BOOKS, daily::index_for(day, n)))
                .ok();

            json!({
                "date": date,
                "ayah": ayah,
                "surah": quran.surah(ayah.s)?,
                // Absent rather than fatal: a machine part-way through the
                // install has the Quran and not yet the corpus, and half a page
                // is better than an error where an ayah could have been.
                "hadith": hadith,
            })
        }
        ApiCmd::Ayah { reference } => {
            let (s, a) = data::parse_ref(&reference)?;
            let ayah = quran.ayah(s, a)?;
            let extras = words::load_extras()?;
            let extra = extras.get(&format!("{s}:{a}"));
            json!({
                "ayah": words::ayah_json(ayah, extra),
                // Words carry the audio key and the tajweed runs, so a reader
                // never has to re-split the text and risk disagreeing with the
                // CDN about which word is the fifth one.
                "words": words::merge(words::split(s, a, &ayah.ar, &ayah.taj, &quran.rules), extra),
            })
        }
    };
    println!("{out}");
    Ok(())
}

fn do_dua(what: DuaCmd) -> Result<()> {
    let book = duas::Book::load()?;
    // The source travels with the content, always — a dua on a screen without
    // the book it is from is this project asserting, which it does not do.
    let out = match what {
        DuaCmd::List => json!({
            "source": book.source,
            "chapters": book.chapters,
            "duas": book.duas,
        }),
        DuaCmd::Show { id } => json!({ "source": book.source, "dua": book.dua(&id)? }),
    };
    println!("{out}");
    Ok(())
}

fn do_hadith(what: HadithCmd) -> Result<()> {
    let out = match what {
        HadithCmd::Collections => {
            let m = hadith::manifest()?;
            json!({ "source": m.source, "collections": m.collections })
        }
        HadithCmd::Search {
            query,
            collection,
            limit,
        } => serde_json::to_value(hadith::search(&query.join(" "), collection.as_deref(), limit)?)?,
        HadithCmd::Show { reference } => {
            let (slug, n) = hadith::parse_ref(&reference)?;
            json!({ "hit": hadith::show(&slug, &n)? })
        }
        // NDJSON rather than one object, so fetch prints its own lines.
        HadithCmd::Fetch { json: _ } => {
            fetch::fetch();
            return Ok(());
        }
        HadithCmd::Status => fetch::status(),
    };
    println!("{out}");
    Ok(())
}

fn do_hifz(what: HifzCmd) -> Result<()> {
    let quran = data::Quran::load()?;
    let mut deck = hifz::Deck::load()?;
    let now = hifz::now();

    let out = match what {
        HifzCmd::Add { reference } => {
            // A bare number is a whole surah — enrolling Al-Ikhlas one ayah at
            // a time is four commands nobody should have to type.
            let refs: Vec<String> = if reference.contains(':') {
                let (s, a) = data::parse_ref(&reference)?;
                quran.ayah(s, a)?;
                vec![format!("{s}:{a}")]
            } else {
                let n: u16 = reference
                    .trim()
                    .parse()
                    .with_context(|| format!("expected an ayah like 1:6 or a surah like 112, got {reference:?}"))?;
                quran.surah(n)?;
                quran.ayat_of(n).iter().map(|a| format!("{}:{}", a.s, a.a)).collect()
            };
            let added = refs.iter().filter(|r| deck.add(r, now)).count();
            deck.save()?;
            json!({ "added": added, "already": refs.len() - added, "enrolled": deck.cards.len() })
        }
        HifzCmd::Remove { reference } => {
            let removed = deck.remove(&reference);
            deck.save()?;
            json!({ "removed": removed, "enrolled": deck.cards.len() })
        }
        HifzCmd::Due { limit } => {
            let due: Vec<_> = deck
                .due(now)
                .into_iter()
                .take(limit)
                .map(|(reference, card)| {
                    // The text rides along so a reviewer does not need a second
                    // call per card to know what it is asking about.
                    let ayah = data::parse_ref(reference)
                        .ok()
                        .and_then(|(s, a)| quran.ayah(s, a).ok());
                    let next: serde_json::Map<String, serde_json::Value> = hifz::GRADES
                        .iter()
                        .map(|g| (g.name().to_string(), card.next_in(*g, now).into()))
                        .collect();
                    json!({ "reference": reference, "card": card, "ayah": ayah, "next": next })
                })
                .collect();
            let all = deck.due(now);
            json!({
                "due": due,
                "total_due": all.len(),
                "new_due": all.iter().filter(|(_, c)| c.reps == 0).count(),
                "enrolled": deck.cards.len(),
            })
        }
        HifzCmd::Grade { reference, grade } => {
            let g = hifz::Grade::parse(&grade)?;
            let card = deck.review(&reference, g, now)?.clone();
            deck.save()?;
            json!({ "reference": reference, "card": card, "due_in_days": (card.due.saturating_sub(now)) as f64 / hifz::DAY as f64 })
        }
        HifzCmd::Stats => {
            let learned = deck.cards.values().filter(|c| c.reps > 0).count();
            let young = deck.cards.values().filter(|c| c.reps > 0 && c.interval < 21.0).count();
            json!({
                "enrolled": deck.cards.len(),
                "due": deck.due(now).len(),
                "learned": learned,
                "young": young,
                "mature": learned - young,
                "lapses": deck.cards.values().map(|c| c.lapses).sum::<u32>(),
            })
        }
    };
    println!("{out}");
    Ok(())
}

fn do_audio(reference: &str, reciter: &str, to_end: bool) -> Result<()> {
    if to_end {
        let (s, a) = data::parse_ref(reference)?;
        let last = data::Quran::load()?.surah(s)?.count;
        let queue: Vec<String> = (a..=last).map(|n| audio::ayah_source(s, n, reciter)).collect();
        println!("{}", json!({ "queue": queue }));
        return Ok(());
    }

    // A dua is asked for by name because its key is a position in the book,
    // not a coordinate: `dua:75a` has no second number to parse.
    if let Some(id) = reference.strip_prefix("dua:") {
        let book = duas::Book::load()?;
        let dua = book.dua(id)?;
        let key = dua
            .audio
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("no recitation is matched to dua {id} — it has none rather than a guessed one"))?;
        let path = audio::dua(key)?;
        println!("{}", json!({ "path": path.to_string_lossy() }));
        return Ok(());
    }

    // `1:1:1` is a word, `1:1` is an ayah — the third field is the only thing
    // that distinguishes them, and it is the word's audio key.
    let parts: Vec<&str> = reference.split(':').collect();
    let path = match parts.len() {
        3 => {
            let (s, a) = data::parse_ref(&parts[..2].join(":"))?;
            let w: usize = parts[2]
                .trim()
                .parse()
                .with_context(|| format!("bad word number in {reference:?}"))?;
            audio::word(s, a, w)?
        }
        2 => {
            let (s, a) = data::parse_ref(reference)?;
            audio::ayah(s, a, reciter)?
        }
        _ => anyhow::bail!("expected 1:1 for an ayah or 1:1:1 for a word, got {reference:?}"),
    };
    println!("{}", json!({ "path": path.to_string_lossy() }));
    Ok(())
}
