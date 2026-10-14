//! `geodb-board compare`: the same queries (`geodb_fw_core::query::COMPARE`) on
//!
//! - **board**: the chip itself (216 MHz Cortex-M7, the image in flash), over the network
//!   (`!query`) or the ST-LINK (`--probe`)
//! - **fw**: `geodb-fw-core` on the same `geodb.fw`, here on the host (the web simulator runs this
//!   code in wasm: its Compare button)
//! - **globe-fw**: `geodb-core`'s `CompactGlobeDb` on the globe-fw data (`make_fw_assets`: the
//!   same 52,088 cities in the web globe's format), f64 haversine
//!
//! and checks they agree: the count within the radius, and the ten nearest (by geoid; the
//! distances within metres, f32 against f64).

use crate::bridge::Via;
use crate::{ask, COMMAND_PORT, STATE_PORT};
use geodb_core::globe_db::CompactGlobeDb;
use geodb_core::globe_layers::Positions;
use geodb_fw_core::query::COMPARE;
use geodb_fw_core::{Answer, FwImage};
use std::net::{IpAddr, SocketAddr, UdpSocket};
use std::time::{Duration, Instant};

/// Host timings: the best of this many runs (the board: of 3).
const RUNS: usize = 20;

pub fn default_fw() -> String {
    concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../firmware/stm32f769i-disco/geodb.fw"
    )
    .into()
}

pub fn default_globe() -> String {
    concat!(env!("CARGO_MANIFEST_DIR"), "/../geodb-globe/assets/fw").into()
}

enum Board {
    Net(UdpSocket, SocketAddr),
    #[cfg(feature = "probe")]
    Probe(Box<crate::probe::Probe>),
}

impl Board {
    fn query(&mut self, lat: f32, lon: f32, km: f32) -> Result<Answer, String> {
        match self {
            Board::Net(sock, to) => {
                let (reply, _) =
                    ask(sock, *to, &format!("!query {lat} {lon} {km}"), 3).ok_or("no answer")?;
                reply
                    .strip_prefix("!query ")
                    .and_then(Answer::parse)
                    .ok_or_else(|| format!("unexpected reply: {reply} (firmware without !query?)"))
            }
            #[cfg(feature = "probe")]
            Board::Probe(p) => p.query(lat, lon, km),
        }
    }
}

/// The board on the network: `--host`, its state packets, or a broadcast; `None` when silent.
fn on_network(host: Option<IpAddr>) -> Option<Board> {
    let sock = UdpSocket::bind(("0.0.0.0", 0)).ok()?;
    sock.set_broadcast(true).ok();
    sock.set_read_timeout(Some(Duration::from_millis(1500)))
        .ok();
    let ip = host
        .or_else(|| {
            let listen = UdpSocket::bind(("0.0.0.0", STATE_PORT)).ok()?;
            listen.set_read_timeout(Some(Duration::from_secs(2))).ok();
            let mut buf = [0u8; 64];
            listen.recv_from(&mut buf).ok().map(|(_, from)| from.ip())
        })
        .or_else(|| {
            ask(
                &sock,
                ([255, 255, 255, 255], COMMAND_PORT).into(),
                "!info",
                2,
            )
            .map(|r| r.1.ip())
        })?;
    let to = SocketAddr::new(ip, COMMAND_PORT);
    let (info, _) = ask(&sock, to, "!info", 3)?;
    println!("board at {ip}: {}", info.trim());
    Some(Board::Net(sock, to))
}

fn board(via: Via) -> Option<Board> {
    match via {
        Via::Net(host) => {
            let b = on_network(host);
            if b.is_none() {
                println!("board: no answer on the network (--host IP, --probe, or --no-board)");
            }
            b
        }
        #[cfg(feature = "probe")]
        Via::Probe => match crate::probe::Probe::attach() {
            Ok(p) => {
                println!("board through the probe {}", p.name);
                Some(Board::Probe(Box::new(p)))
            }
            Err(e) => {
                println!("board: {e}");
                None
            }
        },
        #[cfg(not(feature = "probe"))]
        Via::Probe => {
            println!("board: --probe needs the probe feature (--features probe)");
            None
        }
    }
}

/// The best time of `runs` runs of `f` (µs), and its last result.
fn best<T>(runs: usize, mut f: impl FnMut() -> T) -> (f64, T) {
    let mut t_best = f64::MAX;
    let mut out = None;
    for _ in 0..runs {
        let t = Instant::now();
        let r = f();
        t_best = t_best.min(t.elapsed().as_secs_f64() * 1e6);
        out = Some(r);
    }
    (t_best, out.unwrap())
}

fn us(v: f64) -> String {
    if v >= 1000.0 {
        format!("{:.1} ms", v / 1000.0)
    } else {
        format!("{v:.0} µs")
    }
}

pub fn compare(via: Via, fw_path: &str, globe_dir: &str, no_board: bool) {
    let bytes = std::fs::read(fw_path).unwrap_or_else(|e| {
        eprintln!("{fw_path}: {e} (cargo run --release -p geodb-fw-core --features std --example make_image)");
        std::process::exit(1);
    });
    let img = FwImage::parse(&bytes).expect("a geodb firmware image");
    let globe_path = format!("{globe_dir}/cities.globe");
    let globe = std::fs::read(&globe_path)
        .map_err(|e| e.to_string())
        .and_then(|b| CompactGlobeDb::from_bytes(&b).map_err(|e| e.to_string()))
        .unwrap_or_else(|e| {
            eprintln!(
                "{globe_path}: {e} (cargo run --release -p geodb-globe --example make_fw_assets)"
            );
            std::process::exit(1);
        });
    println!(
        "fw: {} cities in {fw_path}\nglobe-fw: {} cities in {globe_path}",
        img.len(),
        globe.cities.len()
    );
    let mut board = if no_board { None } else { board(via) };
    let clock = Instant::now();
    let clock_us = || clock.elapsed().as_micros() as u32;

    println!(
        "\n{:<16} {:>6}  {:>17}  {:>9}  {:>19}  {:>19}  {:>19}",
        "query",
        "km",
        "count b/fw/globe",
        "nearest",
        "board radius/10nn",
        "fw here",
        "globe-fw here"
    );
    let mut all_agree = true;
    for &(place, lat, lon, km) in &COMPARE {
        let (_, fw) = best(1, || img.answer(lat, lon, km, clock_us));
        let (fw_r, _) = best(RUNS, || {
            let mut n = 0;
            img.radius_index(lat, lon, km, |_, _| n += 1);
            n
        });
        let (fw_n, _) = best(RUNS, || {
            let mut out = [geodb_fw_core::Hit { index: 0, km: 0.0 }; 10];
            img.nearest(lat, lon, &mut out)
        });
        let (g_r, g_count) = best(RUNS, || {
            globe
                .radius_at(
                    f64::from(lat),
                    f64::from(lon),
                    f64::from(km),
                    Positions::Geoid,
                )
                .len()
        });
        let (g_n, g_near) = best(RUNS, || {
            globe.nearest_at(f64::from(lat), f64::from(lon), 10, Positions::Geoid)
        });
        let b = board.as_mut().map(|b| {
            let mut runs: Vec<Answer> = (0..3).filter_map(|_| b.query(lat, lon, km).ok()).collect();
            runs.sort_by_key(|a| a.radius_us + a.nearest_us);
            runs.into_iter().next()
        });

        // the nearest ten, by geoid: fw against globe-fw (and the board against fw)
        let fw_ids: Vec<u32> = fw.nearest[..fw.found]
            .iter()
            .map(|h| img.geoid(h.index as usize))
            .collect();
        let g_ids: Vec<u32> = g_near
            .iter()
            .map(|&(_, i)| (globe.cities[i].geoid >> 32) as u32)
            .collect();
        let same_set = |a: &[u32], b: &[u32]| {
            let (mut a, mut b) = (a.to_vec(), b.to_vec());
            a.sort_unstable();
            b.sort_unstable();
            a == b
        };
        let km_diff = fw.nearest[..fw.found]
            .iter()
            .zip(&g_near)
            .map(|(h, &(d, _))| (f64::from(h.km) - d).abs())
            .fold(0.0, f64::max);
        let mut near_ok = same_set(&fw_ids, &g_ids);
        let board_txt = match b {
            Some(Some(a)) => {
                let ids: Vec<u32> = a.nearest[..a.found]
                    .iter()
                    .map(|h| img.geoid(h.index as usize))
                    .collect();
                near_ok &= same_set(&ids, &fw_ids) && a.count == fw.count;
                (
                    a.count.to_string(),
                    format!(
                        "{} / {}",
                        us(f64::from(a.radius_us)),
                        us(f64::from(a.nearest_us))
                    ),
                )
            }
            Some(None) => ("err".into(), "no answer".into()),
            None => ("-".into(), "-".into()),
        };
        let counts_ok = fw.count as usize == g_count;
        all_agree &= counts_ok && near_ok;
        println!(
            "{place:<16} {km:>6}  {:>17}  {:>9}  {:>19}  {:>19}  {:>19}",
            format!("{}/{}/{}", board_txt.0, fw.count, g_count),
            if near_ok {
                format!("same {:.0}m", km_diff * 1000.0)
            } else {
                "DIFFER".into()
            },
            board_txt.1,
            format!("{} / {}", us(fw_r), us(fw_n)),
            format!("{} / {}", us(g_r), us(g_n)),
        );
    }
    println!(
        "\n{}",
        if all_agree {
            "all agree: the same cities in every radius, the same ten nearest"
        } else {
            "DIFFERENCES above (count or nearest set)"
        }
    );
    println!("(times: the best of {RUNS} runs here, of 3 on the board; nearest: the largest distance difference fw vs globe-fw)");
}
