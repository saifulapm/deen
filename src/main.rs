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

use anyhow::Result;
use clap::{Parser, Subcommand};
use serde_json::json;

mod align;
mod data;
mod normalise;
mod recite;

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
        ApiCmd::Surah { n } => json!({
            "surah": quran.surah(n)?,
            "ayahs": quran.ayat_of(n),
        }),
        ApiCmd::Ayah { reference } => {
            let (s, a) = data::parse_ref(&reference)?;
            json!({ "ayah": quran.ayah(s, a)? })
        }
    };
    println!("{out}");
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
