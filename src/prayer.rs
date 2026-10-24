//! `deen prayer`: the day's prayer times from aladhan's monthly calendar.
//!
//! The month is fetched once with curl (no HTTP crate, as in audio.rs) and
//! kept as the raw answer, so a cached month answers offline for every day in
//! it. The Hanafi school is always sent: aladhan defaults to Shafi'i, which
//! puts Asr about an hour early for a Hanafi.

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Value};

const BASE_URL: &str = "https://api.aladhan.com";

struct Config {
    city: String,
    latitude: String,
    longitude: String,
    method: u32,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            city: "Dhaka".into(),
            latitude: "23.8103".into(),
            longitude: "90.4125".into(),
            method: 1,
        }
    }
}

fn config_path() -> PathBuf {
    if let Ok(p) = std::env::var("DEEN_CONFIG") {
        return PathBuf::from(p);
    }
    std::env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".config"))
        .join("deen/config.toml")
}

/// Four `key = value` lines are all the config has, so this reads just those
/// rather than pulling in a TOML crate. Coordinates stay as the text the user
/// wrote, which is what goes into the request and the cache key.
fn load_config(path: &Path) -> Result<Config> {
    let mut c = Config::default();
    let text = match fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(c),
        Err(e) => return Err(e).with_context(|| format!("cannot read {}", path.display())),
    };
    for (n, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let bad = || anyhow!("{}:{}: cannot parse `{line}`", path.display(), n + 1);
        let (key, value) = line.split_once('=').ok_or_else(bad)?;
        let value = value.trim();
        match key.trim() {
            "city" => {
                c.city = value
                    .strip_prefix('"')
                    .and_then(|v| v.strip_suffix('"'))
                    .ok_or_else(bad)?
                    .to_string()
            }
            k @ ("latitude" | "longitude") => {
                value.parse::<f64>().map_err(|_| bad())?;
                if k == "latitude" {
                    c.latitude = value.to_string();
                } else {
                    c.longitude = value.to_string();
                }
            }
            "method" => c.method = value.parse().map_err(|_| bad())?,
            _ => {}
        }
    }
    Ok(c)
}

fn cache_dir() -> PathBuf {
    std::env::var("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".cache"))
        .join("deen/prayer")
}

/// Today's local date. std has no time zones, so `date` answers it.
fn local_today() -> Result<String> {
    let out = Command::new("date").arg("+%F").output().context("cannot run date")?;
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn parse_date(date: &str) -> Result<(u32, u32)> {
    let parts: Vec<&str> = date.split('-').collect();
    match parts[..] {
        [y, m, d] if y.len() == 4 && m.len() == 2 && d.len() == 2 => {
            let (y, m, d) = (y.parse()?, m.parse()?, d.parse::<u32>()?);
            if (1..=12).contains(&m) && (1..=31).contains(&d) {
                return Ok((y, m));
            }
        }
        _ => {}
    }
    bail!("date must be YYYY-MM-DD, got `{date}`")
}

fn fetch(url: &str, dest: &Path) -> Result<()> {
    let dir = dest.parent().context("cache path has no parent")?;
    fs::create_dir_all(dir).with_context(|| format!("cannot create {}", dir.display()))?;
    // Beside the target and renamed, so a curl killed halfway never leaves a
    // truncated month that later runs treat as cached.
    // Its own name per call: Prayer can ask for two days of one uncached month at once.
    let tmp = dest.with_extension(format!("{}.part", std::process::id()));
    let ok = crate::fetch::command("curl")
        .args(["-fsS", "--max-time", "30", "--retry", "2", "-o"])
        .arg(&tmp)
        .arg(url)
        .status()
        .context("cannot run curl")?
        .success();
    let valid = ok
        && fs::read(&tmp)
            .ok()
            .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
            .map(|v| v["data"].is_array())
            .unwrap_or(false);
    if !valid {
        let _ = fs::remove_file(&tmp);
        bail!("could not fetch {url}");
    }
    fs::rename(&tmp, dest).with_context(|| format!("cannot place {}", dest.display()))
}

pub fn run(date: Option<String>, refresh: bool) -> Result<()> {
    let conf_path = config_path();
    let conf = load_config(&conf_path)?;
    let date = match date {
        Some(d) => d,
        None => local_today()?,
    };
    let (year, month) = parse_date(&date)?;

    let file = cache_dir().join(format!(
        "{year}-{month:02}-{},{}-m{}-s1.json",
        conf.latitude, conf.longitude, conf.method
    ));
    let base = std::env::var("DEEN_ALADHAN_URL").unwrap_or_else(|_| BASE_URL.into());
    let url = format!(
        "{base}/v1/calendar/{year}/{month}?latitude={}&longitude={}&method={}&school=1",
        conf.latitude, conf.longitude, conf.method
    );

    let mut cached = file.is_file();
    if refresh || !cached {
        match fetch(&url, &file) {
            Ok(()) => cached = false,
            // A failed refresh keeps the old month and says so on stderr.
            Err(e) if cached => eprintln!("deen: refresh failed, using the cache: {e:#}"),
            Err(e) => return Err(e),
        }
    }

    let month_json: Value = serde_json::from_slice(&fs::read(&file)?)
        .with_context(|| format!("{} is not valid JSON", file.display()))?;
    let want = format!("{:02}-{month:02}-{year}", date[8..].parse::<u32>()?);
    let day = month_json["data"]
        .as_array()
        .and_then(|days| days.iter().find(|d| d["date"]["gregorian"]["date"] == want))
        .with_context(|| format!("no entry for {date} in {}", file.display()))?;

    let time = |k: &str| -> String {
        // "04:43 (+06)" -> "04:43"
        day["timings"][k].as_str().unwrap_or("").split(' ').next().unwrap_or("").to_string()
    };
    let out = json!({
        "city": conf.city,
        "latitude": conf.latitude.parse::<f64>()?,
        "longitude": conf.longitude.parse::<f64>()?,
        "method": {
            "id": conf.method,
            "name": day["meta"]["method"]["name"].as_str().unwrap_or(""),
        },
        "school": "Hanafi",
        "date": date,
        "times": {
            "fajr": time("Fajr"), "sunrise": time("Sunrise"), "dhuhr": time("Dhuhr"),
            "asr": time("Asr"), "maghrib": time("Maghrib"), "isha": time("Isha"),
        },
        "cached": cached,
    });
    println!("{out}");
    Ok(())
}
