//! Recitation audio, cached on disk.
//!
//! Three sources, all plain files over HTTPS with no key and no auth:
//!
//!   * per-word, `audio.qurancdn.com/wbw/SSS_AAA_WWW.mp3` — the point of the
//!     mushaf screen, since a word you cannot read is one tap from hearing it
//!   * per-ayah, `everyayah.com/data/<reciter>/SSSAAA.mp3`
//!   * per-dua, an mp3 per entry of Hisn al-Muslim, from the pinned repo the
//!     repeat counts come from
//!
//! Fetched with `curl` rather than an HTTP crate, for the same reason
//! transcription is `voxtype` and recording is `pw-record`: this program's job
//! is to know what to ask for, not to carry a TLS stack to ask with.
//!
//! Nothing here ever deletes: the cache is small, additive, and the whole point
//! is that the second time you tap a word it is instant and offline.

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{bail, Context, Result};

/// Husary, murattal — slow and clearly articulated, which is what someone
/// relearning to read needs. Override with `--reciter`.
pub const DEFAULT_RECITER: &str = "Husary_128kbps";

fn cache_root() -> PathBuf {
    std::env::var("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".cache")
        })
        .join("deen/audio")
}

/// Download `url` to `dest` unless it is already there. Returns `dest`.
fn ensure(url: &str, dest: &Path) -> Result<PathBuf> {
    if dest.is_file() && dest.metadata().map(|m| m.len() > 0).unwrap_or(false) {
        return Ok(dest.to_path_buf());
    }
    let dir = dest.parent().context("audio path has no parent")?;
    fs::create_dir_all(dir).with_context(|| format!("cannot create {}", dir.display()))?;

    // Download beside the target and rename: a curl killed halfway must not
    // leave a truncated mp3 that every later run treats as cached.
    let tmp = dest.with_extension("part");
    let status = Command::new("curl")
        .args(["-fsSL", "--max-time", "60", "-o"])
        .arg(&tmp)
        .arg(url)
        .status()
        .context("cannot run curl")?;
    if !status.success() {
        let _ = fs::remove_file(&tmp);
        bail!("could not fetch {url}");
    }
    fs::rename(&tmp, dest).with_context(|| format!("cannot place {}", dest.display()))?;
    Ok(dest.to_path_buf())
}

/// One word: `1:1:1`.
pub fn word(surah: u16, ayah: u16, index: usize) -> Result<PathBuf> {
    let key = format!("{surah:03}_{ayah:03}_{index:03}");
    let dest = cache_root().join("wbw").join(format!("{key}.mp3"));
    ensure(&format!("https://audio.qurancdn.com/wbw/{key}.mp3"), &dest)
}

/// One dua, by the audio key `scripts/fetch-duas` matched to it.
///
/// Pinned to the commit that supplied the key: the file names are positions in
/// somebody else's repo, and a repo that renumbers them would otherwise start
/// answering a different dua with the same request.
pub fn dua(key: &str) -> Result<PathBuf> {
    const REPO: &str = "YousefAsalya/Islamic-Pro-azkar-API";
    const SHA: &str = "d793023e3b69c91674023f692425950c8ebbb64d";
    let dest = cache_root().join("dua").join(format!("{key}.mp3"));
    ensure(
        &format!("https://raw.githubusercontent.com/{REPO}/{SHA}/audio/{key}.mp3"),
        &dest,
    )
}

/// One ayah, from a reciter.
pub fn ayah(surah: u16, ayah: u16, reciter: &str) -> Result<PathBuf> {
    let key = format!("{surah:03}{ayah:03}");
    let dest = cache_root()
        .join("ayah")
        .join(reciter)
        .join(format!("{key}.mp3"));
    ensure(
        &format!("https://everyayah.com/data/{reciter}/{key}.mp3"),
        &dest,
    )
}
