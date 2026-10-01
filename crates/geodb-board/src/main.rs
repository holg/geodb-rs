//! Host tools for the geodb board (STM32F769I-DISCO) over Ethernet, replacing the two Python scripts.
//!
//! ```text
//! geodb-board serve [--data FILE]     answer the board's "who is here?" questions (UDP 7878) and
//!                                     print its log lines
//! geodb-board test [--host IP]        info, 200 pings (round trip, loss), a 1000 x 512 byte blast
//! geodb-board info | ping [N] | blast [N] | reset   [--host IP]
//! ```
//!
//! The board is found by listening for its state packets (UDP 7881), else by broadcast; `--host`
//! names it directly. The board asks `?LAT,LON` (degrees x 1e5) and gets
//! `=LAT,LON|Name|State, Country` (ASCII) from the full dataset.

use serde::Deserialize;
use std::collections::HashMap;
use std::io::Read;
use std::net::{IpAddr, SocketAddr, UdpSocket};
use std::time::{Duration, Instant};

const QUESTION_PORT: u16 = 7878;
const COMMAND_PORT: u16 = 7880;
const STATE_PORT: u16 = 7881;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let flag = |name: &str| {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1).cloned())
    };
    let cmd = args
        .iter()
        .find(|a| !a.starts_with("--") && a.parse::<IpAddr>().is_err())
        .map_or("test", String::as_str);
    let count: Option<usize> = args.iter().skip(1).find_map(|a| a.parse().ok());
    match cmd {
        "serve" => serve(&flag("--data").unwrap_or_else(default_data)),
        "test" | "info" | "ping" | "blast" | "reset" => {
            client(cmd, flag("--host").and_then(|h| h.parse().ok()), count)
        }
        _ => {
            eprintln!("usage: geodb-board serve [--data FILE] | test|info|ping [N]|blast [N]|reset [--host IP]");
            std::process::exit(2);
        }
    }
}

fn default_data() -> String {
    concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../geodb-core/data/countries+states+cities.json.gz"
    )
    .into()
}

// ------------------------------------------------------------------------------------ the server

#[derive(Deserialize)]
struct Country {
    name: String,
    states: Option<Vec<State>>,
}

#[derive(Deserialize)]
struct State {
    name: String,
    cities: Option<Vec<City>>,
}

#[derive(Deserialize)]
struct City {
    name: String,
    latitude: Option<String>,
    longitude: Option<String>,
}

struct Place {
    lat: f64,
    lon: f64,
    name: String,
    detail: String,
}

/// 0.02 degree cells -> the places in them.
type Grid = HashMap<(i32, i32), Vec<Place>>;

fn ascii_of(s: &str, n: usize) -> String {
    deunicode::deunicode(s)
        .chars()
        .filter(|c| (' '..'\x7f').contains(c))
        .collect::<String>()
        .trim()
        .chars()
        .take(n)
        .collect()
}

fn cell(lat: f64, lon: f64) -> (i32, i32) {
    ((lat * 50.0).round() as i32, (lon * 50.0).round() as i32)
}

fn load(path: &str) -> Grid {
    let mut text = String::new();
    flate2::read::GzDecoder::new(
        std::fs::File::open(path).unwrap_or_else(|e| panic!("{path}: {e}")),
    )
    .read_to_string(&mut text)
    .expect("gunzip the dataset");
    let countries: Vec<Country> = serde_json::from_str(&text).expect("the dataset's json");
    let mut grid = Grid::new();
    let mut count = 0usize;
    for country in &countries {
        let country_name = ascii_of(&country.name, 18);
        for state in country.states.iter().flatten() {
            let detail = format!("{}, {country_name}", ascii_of(&state.name, 18));
            for city in state.cities.iter().flatten() {
                let (Some(lat), Some(lon)) = (
                    city.latitude.as_deref().and_then(|v| v.parse::<f64>().ok()),
                    city.longitude
                        .as_deref()
                        .and_then(|v| v.parse::<f64>().ok()),
                ) else {
                    continue;
                };
                grid.entry(cell(lat, lon)).or_default().push(Place {
                    lat,
                    lon,
                    name: ascii_of(&city.name, 23),
                    detail: detail.clone(),
                });
                count += 1;
            }
        }
    }
    println!("{count} cities loaded");
    grid
}

fn km(a: (f64, f64), b: (f64, f64)) -> f64 {
    let p = std::f64::consts::PI / 180.0;
    let h = ((b.0 - a.0) * p / 2.0).sin().powi(2)
        + (a.0 * p).cos() * (b.0 * p).cos() * ((b.1 - a.1) * p / 2.0).sin().powi(2);
    12742.0 * h.sqrt().asin()
}

/// The nearest place within 2 km.
fn lookup(grid: &Grid, lat: f64, lon: f64) -> Option<&Place> {
    let (cy, cx) = cell(lat, lon);
    let mut best: Option<(f64, &Place)> = None;
    for dy in -1..=1 {
        for dx in -1..=1 {
            for p in grid.get(&(cy + dy, cx + dx)).into_iter().flatten() {
                let d = km((lat, lon), (p.lat, p.lon));
                if d < 2.0 && best.is_none_or(|(b, _)| d < b) {
                    best = Some((d, p));
                }
            }
        }
    }
    best.map(|(_, p)| p)
}

/// The reply to one datagram, or `None` for anything that is not a question.
fn answer(grid: &Grid, line: &str) -> Option<String> {
    let line = line.trim();
    if let Some(log) = line.strip_prefix('#') {
        println!("board: {log}");
        return None;
    }
    let (la, lo) = line.strip_prefix('?')?.split_once(',')?;
    let (la, lo): (i64, i64) = (la.trim().parse().ok()?, lo.trim().parse().ok()?);
    let found = lookup(grid, la as f64 / 1e5, lo as f64 / 1e5);
    println!(
        "{:.4},{:.4} -> {}",
        la as f64 / 1e5,
        lo as f64 / 1e5,
        found.map_or("-", |p| p.name.as_str())
    );
    Some(match found {
        Some(p) => format!("={la},{lo}|{}|{}\n", p.name, p.detail),
        None => format!("={la},{lo}||\n"),
    })
}

fn serve(data: &str) {
    let grid = load(data);
    let sock = UdpSocket::bind(("0.0.0.0", QUESTION_PORT))
        .expect("UDP port 7878 (another server running?)");
    println!("serving UDP on port {QUESTION_PORT} (the board broadcasts its questions)");
    let mut buf = [0u8; 512];
    loop {
        let Ok((n, peer)) = sock.recv_from(&mut buf) else {
            continue;
        };
        if let Some(reply) = answer(&grid, &String::from_utf8_lossy(&buf[..n])) {
            let _ = sock.send_to(reply.as_bytes(), peer);
        }
    }
}

// ------------------------------------------------------------------------------------ the client

fn ask(sock: &UdpSocket, to: SocketAddr, text: &str, tries: usize) -> Option<(String, SocketAddr)> {
    let mut buf = [0u8; 2048];
    for _ in 0..tries {
        let _ = sock.send_to(text.as_bytes(), to);
        if let Ok((n, from)) = sock.recv_from(&mut buf) {
            return Some((String::from_utf8_lossy(&buf[..n]).into_owned(), from));
        }
    }
    None
}

/// The board's address: its state packets (UDP 7881), else a broadcast.
fn find(sock: &UdpSocket, host: Option<IpAddr>) -> IpAddr {
    if let Some(ip) = host {
        return ip;
    }
    if let Ok(listen) = UdpSocket::bind(("0.0.0.0", STATE_PORT)) {
        listen.set_read_timeout(Some(Duration::from_secs(3))).ok();
        let mut buf = [0u8; 64];
        if let Ok((_, from)) = listen.recv_from(&mut buf) {
            return from.ip();
        }
    }
    match ask(
        sock,
        ([255, 255, 255, 255], COMMAND_PORT).into(),
        "!info",
        3,
    ) {
        Some((_, from)) => from.ip(),
        None => {
            eprintln!("no answer from the board (cable? DHCP? same network? try --host IP)");
            std::process::exit(1);
        }
    }
}

fn ping(sock: &UdpSocket, to: SocketAddr, n: usize) {
    let (mut rtts, mut lost) = (Vec::new(), 0);
    let mut buf = [0u8; 2048];
    for i in 0..n {
        let t = Instant::now();
        let _ = sock.send_to(format!("!ping {i}").as_bytes(), to);
        let want = format!("!pong {i}");
        loop {
            match sock.recv_from(&mut buf) {
                Ok((len, _)) if String::from_utf8_lossy(&buf[..len]).trim() == want => {
                    rtts.push(t.elapsed().as_secs_f64() * 1000.0);
                    break;
                }
                Ok(_) => {}
                Err(_) => {
                    lost += 1;
                    break;
                }
            }
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    if rtts.is_empty() {
        println!("ping: all {n} lost");
        return;
    }
    rtts.sort_by(f64::total_cmp);
    let at = |q: f64| rtts[(((rtts.len() as f64 * q) as usize).max(1) - 1).min(rtts.len() - 1)];
    println!(
        "ping: {n} sent, {lost} lost, rtt ms min {:.2} median {:.2} p95 {:.2} max {:.2}",
        rtts[0],
        at(0.5),
        at(0.95),
        rtts[rtts.len() - 1]
    );
}

fn blast(sock: &UdpSocket, to: SocketAddr, n: usize) {
    sock.set_read_timeout(Some(Duration::from_secs(1))).ok();
    let _ = sock.send_to(format!("!blast {n}").as_bytes(), to);
    let mut seen = std::collections::HashSet::new();
    let (mut bytes, mut t0) = (0usize, None);
    let mut buf = [0u8; 2048];
    while let Ok((len, _)) = sock.recv_from(&mut buf) {
        t0.get_or_insert_with(Instant::now);
        let data = &buf[..len];
        if data.starts_with(b"!blast done") {
            break;
        }
        if let Some(i) = data
            .strip_prefix(b"!blast ")
            .and_then(|r| r.split(|&b| b == b' ').next())
            .and_then(|v| std::str::from_utf8(v).ok())
            .and_then(|v| v.parse::<usize>().ok())
        {
            seen.insert(i);
            bytes += len;
        }
    }
    let secs = t0.map_or(1e-6, |t| t.elapsed().as_secs_f64()).max(1e-6);
    println!(
        "blast: {} of {n} datagrams received ({} lost), {} KB in {secs:.2} s = {:.2} Mbit/s",
        seen.len(),
        n - seen.len(),
        bytes / 1024,
        bytes as f64 * 8.0 / secs / 1e6
    );
}

fn client(cmd: &str, host: Option<IpAddr>, count: Option<usize>) {
    let sock = UdpSocket::bind(("0.0.0.0", 0)).expect("socket");
    sock.set_broadcast(true).ok();
    sock.set_read_timeout(Some(Duration::from_millis(500))).ok();
    let ip = find(&sock, host);
    let to = SocketAddr::new(ip, COMMAND_PORT);
    let info = ask(&sock, to, "!info", 3).map_or("(no answer)".into(), |r| r.0);
    println!("board at {ip}: {info}");
    if matches!(cmd, "test" | "ping") {
        ping(&sock, to, count.unwrap_or(200));
    }
    if matches!(cmd, "test" | "blast") {
        blast(&sock, to, count.unwrap_or(1000));
    }
    if cmd == "test" {
        sock.set_read_timeout(Some(Duration::from_millis(500))).ok();
        println!(
            "after: {}",
            ask(&sock, to, "!info", 3).map_or("(no answer)".into(), |r| r.0)
        );
    }
    if cmd == "reset" {
        println!(
            "{}",
            ask(&sock, to, "!reset", 1).map_or("(no answer)".into(), |r| r.0)
        );
    }
}
