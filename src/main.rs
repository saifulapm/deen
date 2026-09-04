//! deen — check your Quran recitation against the text.
//!
//! Every command answers with exactly one JSON object on stdout and puts
//! diagnostics on stderr, so the caller can parse stdout blind. That contract
//! is what lets the shell's hub be a front end for this binary and nothing
//! else, and it is the same contract dekho answers on.
//!
//! It is an ordinary terminal program too:
//!
//!     deen api ayah 1:6 | jq -r .ar
//!     deen recite 1:6 --wav recitation.wav | jq .words

use std::{path::PathBuf, time::Duration};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use serde_json::json;

mod align;
mod audio;
mod data;
mod normalise;
mod recite;
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
        /// `1:1:1` for one word, `1:1` for a whole ayah.
        reference: String,
        /// everyayah.com reciter directory, for whole-ayah audio.
        #[arg(long, default_value = audio::DEFAULT_RECITER)]
        reciter: String,
    },
    /// Recite an ayah and be told which words were right.
    ///
    /// With --wav, scores an existing 16 kHz mono recording. Without it,
    /// records from the microphone until stdin closes — so a caller can stop
    /// the recording by closing the pipe, and a person can stop it by pressing
    /// enter.
    Recite {
        /// Ayah reference, e.g. 1:6
        ayah: String,
        /// Score this recording instead of the microphone.
        #[arg(long)]
        wav: Option<PathBuf>,
        /// Ceiling on a microphone recording, so a caller that never closes
        /// stdin cannot hold the microphone open indefinitely.
        #[arg(long, default_value_t = 120)]
        max_seconds: u64,
    },
}

#[derive(Subcommand)]
enum ApiCmd {
    /// Every surah, with its name and length.
    Surahs,
    /// One surah and all of its ayat.
    Surah { n: u16 },
    /// One ayah.
    Ayah { reference: String },
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
        Cmd::Audio { reference, reciter } => do_audio(&reference, &reciter),
        Cmd::Recite {
            ayah,
            wav,
            max_seconds,
        } => do_recite(&ayah, wav, max_seconds),
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
            let words: Vec<_> = ayahs
                .iter()
                .map(|a| words::split(a.s, a.a, &a.ar, &a.taj, &quran.rules))
                .collect();
            json!({
                "surah": quran.surah(n)?,
                "ayahs": ayahs,
                "words": words,
                // Al-Fatiha opens with it as ayah 1 and At-Tawbah has none, so
                // those two are the only surahs a reader must not head with it.
                "basmala": if n == 1 || n == 9 { "" } else { &quran.basmala },
            })
        }
        ApiCmd::Ayah { reference } => {
            let (s, a) = data::parse_ref(&reference)?;
            let ayah = quran.ayah(s, a)?;
            json!({
                "ayah": ayah,
                // Words carry the audio key and the tajweed runs, so a reader
                // never has to re-split the text and risk disagreeing with the
                // CDN about which word is the fifth one.
                "words": words::split(s, a, &ayah.ar, &ayah.taj, &quran.rules),
            })
        }
    };
    println!("{out}");
    Ok(())
}

fn do_audio(reference: &str, reciter: &str) -> Result<()> {
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

fn do_recite(reference: &str, wav: Option<PathBuf>, max_seconds: u64) -> Result<()> {
    let (s, a) = data::parse_ref(reference)?;
    let quran = data::Quran::load()?;
    let ayah = quran.ayah(s, a)?;

    // A recording we made is scratch and gets removed; one handed to us with
    // --wav belongs to the caller and is left alone.
    let (path, ours) = match wav {
        Some(p) => (p, false),
        None => {
            let p = recite::scratch_wav()?;
            recite::record(&p, Duration::from_secs(max_seconds))?;
            (p, true)
        }
    };

    let heard = recite::transcribe(&path);
    if ours {
        let _ = std::fs::remove_file(&path);
    }
    let heard = heard?;

    let words = align::align(&normalise::tokens(&ayah.ar), &normalise::tokens(&heard));
    let (correct, total, wer) = align::score(&words);

    println!(
        "{}",
        json!({
            "ayah": ayah,
            "heard": heard,
            "words": words,
            "correct": correct,
            "total": total,
            // Bounded 0..1 and the only figure fit to show a person. `wer`
            // is unbounded above — reciting the wrong ayah entirely scores
            // 3.0, because every inserted word is an error against a short
            // reference — which is fine as a diagnostic and no way to tell
            // someone how they did.
            "accuracy": if total == 0 { 0.0 } else { correct as f64 / total as f64 },
            "wer": wer,
        })
    );
    Ok(())
}
