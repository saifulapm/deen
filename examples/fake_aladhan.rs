//! A stand-in for api.aladhan.com for the prayer tests.
//!
//!     fake_aladhan <body.json> <request-log>
//!
//! Prints the port it listens on, answers every request with the body, and
//! appends each request line to the log. The test stops it by killing it.

use std::{
    fs::OpenOptions,
    io::{BufRead, BufReader, Write},
    net::TcpListener,
};

fn main() {
    let mut args = std::env::args().skip(1);
    let body = std::fs::read(args.next().expect("body file")).unwrap();
    let log = args.next().expect("log file");
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    println!("{}", listener.local_addr().unwrap().port());
    std::io::stdout().flush().unwrap();
    for stream in listener.incoming().flatten() {
        let mut line = String::new();
        let mut reader = BufReader::new(&stream);
        if reader.read_line(&mut line).is_err() {
            continue;
        }
        let mut f = OpenOptions::new().create(true).append(true).open(&log).unwrap();
        f.write_all(line.as_bytes()).unwrap();
        let mut stream = &stream;
        let head = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        let _ = stream.write_all(head.as_bytes());
        let _ = stream.write_all(&body);
    }
}
