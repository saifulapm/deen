use serde_json::{json, Value};
use std::{
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::{Child, Command, Output, Stdio},
};

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("deen-prayer-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

struct Fake {
    child: Child,
    port: u16,
    log: PathBuf,
}

impl Fake {
    fn start(dir: &Path) -> Fake {
        let bin = Path::new(env!("CARGO_BIN_EXE_deen"))
            .parent()
            .unwrap()
            .join("examples/fake_aladhan");
        let log = dir.join("requests.log");
        let body = format!("{}/tests/fixtures/aladhan-2026-10.json", env!("CARGO_MANIFEST_DIR"));
        let mut child = Command::new(bin)
            .args([body.as_str(), log.to_str().unwrap()])
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let mut line = String::new();
        BufReader::new(child.stdout.take().unwrap()).read_line(&mut line).unwrap();
        Fake { child, port: line.trim().parse().unwrap(), log }
    }
    fn url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }
    fn requests(&self) -> Vec<String> {
        std::fs::read_to_string(&self.log)
            .unwrap_or_default()
            .lines()
            .map(String::from)
            .collect()
    }
    fn stop(self) -> String {
        self.url()
    }
}

// A failing assertion must not leave the server running, or the test run
// never ends.
impl Drop for Fake {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn deen(dir: &Path, url: &str, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_deen"))
        .arg("prayer")
        .args(args)
        .env("XDG_CACHE_HOME", dir.join("cache"))
        .env("DEEN_CONFIG", dir.join("config.toml"))
        .env("DEEN_ALADHAN_URL", url)
        .output()
        .unwrap()
}

fn ok(o: Output) -> Value {
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    serde_json::from_slice(&o.stdout).unwrap()
}

fn dhaka_23(cached: bool) -> Value {
    json!({
        "city": "Dhaka", "latitude": 23.8103, "longitude": 90.4125,
        "method": {"id": 1, "name": "University of Islamic Sciences, Karachi"},
        "school": "Hanafi", "date": "2026-10-23",
        "times": {"fajr": "04:43", "sunrise": "05:59", "dhuhr": "11:43",
                  "asr": "15:50", "maghrib": "17:26", "isha": "18:42"},
        "cached": cached,
    })
}

#[test]
fn fetches_the_month_once_then_answers_offline() {
    let d = tmp("month");
    let fake = Fake::start(&d);
    let url = fake.url();

    assert_eq!(ok(deen(&d, &url, &["--date", "2026-10-23"])), dhaka_23(false));
    let reqs = fake.requests();
    assert_eq!(reqs.len(), 1);
    assert!(reqs[0].contains("/v1/calendar/2026/10?"), "{}", reqs[0]);
    assert!(reqs[0].contains("school=1"), "{}", reqs[0]);
    assert!(reqs[0].contains("method=1"), "{}", reqs[0]);
    assert!(reqs[0].contains("latitude=23.8103"), "{}", reqs[0]);

    let log = fake.log.clone();
    let url = fake.stop();
    assert_eq!(ok(deen(&d, &url, &["--date", "2026-10-23"])), dhaka_23(true));

    let next = ok(deen(&d, &url, &["--date", "2026-10-24"]));
    assert_eq!(next["date"], "2026-10-24");
    assert_eq!(next["cached"], true);
    assert_eq!(next["times"]["asr"].as_str().unwrap().len(), 5);
    assert_eq!(std::fs::read_to_string(log).unwrap().lines().count(), 1);
}

#[test]
fn refresh_fetches_again_and_keeps_the_cache_when_the_fetch_fails() {
    let d = tmp("refresh");
    let fake = Fake::start(&d);
    let url = fake.url();
    ok(deen(&d, &url, &["--date", "2026-10-23"]));

    assert_eq!(ok(deen(&d, &url, &["--date", "2026-10-23", "--refresh"])), dhaka_23(false));
    assert_eq!(fake.requests().len(), 2);

    let url = fake.stop();
    assert_eq!(ok(deen(&d, &url, &["--date", "2026-10-23", "--refresh"])), dhaka_23(true));
}

#[test]
fn another_city_sends_its_coordinates_and_caches_apart() {
    let d = tmp("city");
    let fake = Fake::start(&d);
    let url = fake.url();
    ok(deen(&d, &url, &["--date", "2026-10-23"]));
    std::fs::write(
        d.join("config.toml"),
        "city = \"Makkah\"\nlatitude = 21.4225\nlongitude = 39.8262\nmethod = 4\n",
    )
    .unwrap();

    let v = ok(deen(&d, &url, &["--date", "2026-10-23"]));
    assert_eq!(v["city"], "Makkah");
    assert_eq!(v["latitude"], 21.4225);
    assert_eq!(v["method"]["id"], 4);
    assert_eq!(v["cached"], false);
    let reqs = fake.requests();
    assert_eq!(reqs.len(), 2);
    assert!(reqs[1].contains("latitude=21.4225"), "{}", reqs[1]);
    assert!(reqs[1].contains("longitude=39.8262"), "{}", reqs[1]);
    assert!(reqs[1].contains("method=4"), "{}", reqs[1]);
    assert!(reqs[1].contains("school=1"), "{}", reqs[1]);
    let files = std::fs::read_dir(d.join("cache/deen/prayer")).unwrap().count();
    assert_eq!(files, 2);
    fake.stop();
}

#[test]
fn no_cache_and_no_server_is_an_error() {
    let d = tmp("offline");
    let url = Fake::start(&d).stop();
    let o = deen(&d, &url, &["--date", "2026-10-23"]);
    assert_eq!(o.status.code(), Some(1));
    let v: Value = serde_json::from_slice(&o.stdout).unwrap();
    assert!(!v["error"].as_str().unwrap().is_empty());
}

#[test]
fn a_malformed_config_names_its_file() {
    let d = tmp("badconf");
    std::fs::write(d.join("config.toml"), "latitude = north\n").unwrap();
    let o = deen(&d, "http://127.0.0.1:9", &["--date", "2026-10-23"]);
    assert_eq!(o.status.code(), Some(1));
    let v: Value = serde_json::from_slice(&o.stdout).unwrap();
    assert!(v["error"].as_str().unwrap().contains("config.toml"), "{v}");
}
