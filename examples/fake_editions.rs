//! A stand-in for the jsDelivr editions directory, for the hadith fetch tests.
//!
//!     fake_editions <dir>
//!
//! Serves the files in <dir> over plain HTTP on 127.0.0.1 and prints the
//! bound address as its first line. HEAD answers with Content-Length, a GET
//! with `Range: bytes=N-` answers 206, a missing file answers 404.
//!
//! FAKE_CUT=<n> drops the connection once n body bytes have been served in
//! total, and drops every later GET before replying, so a client's retries
//! fail as well. FAKE_HEAD_NO_LENGTH=1 answers HEAD without Content-Length,
//! the way a jsDelivr edge does on a cold cache miss. FAKE_LOG=<file> gets
//! one line per request:
//! `<method> <path> <range or -> <body bytes served>`.

use std::{
    fs,
    io::{BufRead, BufReader, Write},
    net::{Shutdown, TcpListener, TcpStream},
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    thread,
};

struct Shared {
    dir: PathBuf,
    cut: Option<u64>,
    served: AtomicU64,
    log: Mutex<Option<fs::File>>,
}

fn main() {
    let dir = PathBuf::from(std::env::args().nth(1).expect("usage: fake_editions <dir>"));
    let cut = std::env::var("FAKE_CUT").ok().map(|s| s.parse().expect("FAKE_CUT is a number"));
    let log = std::env::var("FAKE_LOG").ok().map(|p| {
        fs::OpenOptions::new().create(true).append(true).open(p).expect("cannot open FAKE_LOG")
    });
    let shared = Arc::new(Shared { dir, cut, served: AtomicU64::new(0), log: Mutex::new(log) });

    let listener = TcpListener::bind("127.0.0.1:0").expect("cannot bind");
    println!("{}", listener.local_addr().unwrap());
    std::io::stdout().flush().unwrap();

    for stream in listener.incoming().flatten() {
        let shared = shared.clone();
        thread::spawn(move || {
            let _ = serve(stream, &shared);
        });
    }
}

fn serve(mut stream: TcpStream, shared: &Shared) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut first = String::new();
    reader.read_line(&mut first)?;
    let mut range = None;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 || line.trim().is_empty() {
            break;
        }
        if let Some((k, v)) = line.split_once(':') {
            if k.eq_ignore_ascii_case("range") {
                range = Some(v.trim().to_string());
            }
        }
    }
    let mut parts = first.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let path = parts.next().unwrap_or("").to_string();

    let file = shared.dir.join(path.trim_start_matches('/'));
    let body = match file.is_file().then(|| fs::read(&file)) {
        Some(Ok(b)) => b,
        _ => {
            let msg = b"Couldn't find the requested file.";
            write!(stream, "HTTP/1.1 404 Not Found\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", msg.len())?;
            if method != "HEAD" {
                stream.write_all(msg)?;
            }
            log(shared, &method, &path, &range, 0);
            return stream.shutdown(Shutdown::Both);
        }
    };
    let len = body.len() as u64;

    if method == "HEAD" {
        if std::env::var_os("FAKE_HEAD_NO_LENGTH").is_some() {
            write!(stream, "HTTP/1.1 200 OK\r\nAccept-Ranges: bytes\r\nConnection: close\r\n\r\n")?;
        } else {
            write!(stream, "HTTP/1.1 200 OK\r\nContent-Length: {len}\r\nAccept-Ranges: bytes\r\nConnection: close\r\n\r\n")?;
        }
        log(shared, &method, &path, &range, 0);
        return stream.shutdown(Shutdown::Both);
    }

    if shared.cut.is_some_and(|c| shared.served.load(Ordering::SeqCst) >= c) {
        log(shared, &method, &path, &range, 0);
        return stream.shutdown(Shutdown::Both);
    }

    // `bytes=S-` or `bytes=S-E`, E inclusive.
    let (start, end) = range
        .as_deref()
        .and_then(|r| r.strip_prefix("bytes="))
        .and_then(|r| r.split_once('-'))
        .map(|(s, e)| {
            let s = s.parse::<u64>().expect("only bytes=S- or bytes=S-E ranges");
            (s, e.parse::<u64>().map_or(len, |e| (e + 1).min(len)))
        })
        .unwrap_or((0, len));
    if start >= len && range.is_some() {
        write!(stream, "HTTP/1.1 416 Range Not Satisfiable\r\nContent-Range: bytes */{len}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")?;
        log(shared, &method, &path, &range, 0);
        return stream.shutdown(Shutdown::Both);
    }
    if range.is_some() {
        write!(stream, "HTTP/1.1 206 Partial Content\r\nContent-Range: bytes {start}-{}/{len}\r\n", end - 1)?;
    } else {
        write!(stream, "HTTP/1.1 200 OK\r\n")?;
    }
    write!(stream, "Content-Length: {}\r\nConnection: close\r\n\r\n", end - start)?;

    // Serve in small chunks so a cut lands inside a file, not at its end.
    let mut sent = 0u64;
    for chunk in body[start as usize..end as usize].chunks(256) {
        let mut take = chunk.len() as u64;
        if let Some(c) = shared.cut {
            let before = shared.served.load(Ordering::SeqCst);
            take = take.min(c.saturating_sub(before));
        }
        stream.write_all(&chunk[..take as usize])?;
        shared.served.fetch_add(take, Ordering::SeqCst);
        sent += take;
        if take < chunk.len() as u64 {
            break;
        }
    }
    stream.flush()?;
    log(shared, &method, &path, &range, sent);
    stream.shutdown(Shutdown::Both)
}

fn log(shared: &Shared, method: &str, path: &str, range: &Option<String>, bytes: u64) {
    if let Some(f) = shared.log.lock().unwrap().as_mut() {
        let _ = writeln!(f, "{method} {path} {} {bytes}", range.as_deref().unwrap_or("-").replace(' ', ""));
    }
}

