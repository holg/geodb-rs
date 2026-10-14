//! `geodb-board bridge`: the board for a browser. A browser cannot send UDP or reach a debug probe,
//! so this serves the web simulator (`crates/geodb-fw-web/dist`, from its `build.py`) over HTTP
//! and relays on `/ws` (WebSocket):
//!
//! - board -> page: every state packet (19 bytes, binary), `board WHERE` (text) once the board
//!   is found, `!query ...` (text) answers
//! - page -> board: `!tap X Y`, `!drag DX DY`, `!release`, `!query LAT LON KM` (text)
//!
//! Two listeners: plain HTTP and `ws://` (default port 8792), and HTTPS and `wss://` (8793, with
//! the certificates of `tls.rs`): an https page (trahe.eu) may open only the secure one. The
//! bridge serves its certificate authority as `/geodb-bridge-ca.cer` for phones to trust.
//!
//! Two ways to the board: the network (state packets on UDP 7881, commands to UDP 7880; holds
//! port 7881 like the `mirror` example: not both at once), or with `--probe` its ST-LINK
//! (probe-rs, feature `probe`: the link block in the board's RAM, see `geodb_fw_core::link`).

use std::io::{ErrorKind, Read, Write};
use std::net::{IpAddr, TcpListener, TcpStream, UdpSocket};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tungstenite::Message;

use crate::{COMMAND_PORT, STATE_PORT};

/// How the bridge reaches the board.
pub enum Via {
    /// The network; the address is learnt from the state packets unless given.
    Net(Option<IpAddr>),
    /// The ST-LINK.
    Probe,
}

/// The connected pages (one channel each) and what they are told on connecting.
#[derive(Default)]
struct Pages {
    board: Option<String>,
    out: Vec<Sender<Message>>,
}

impl Pages {
    fn send(&mut self, m: Message) {
        self.out.retain(|p| p.send(m.clone()).is_ok());
    }

    fn found(&mut self, board: String) {
        if self.board.as_deref() != Some(board.as_str()) {
            println!("board: {board}");
            self.send(Message::text(format!("board {board}")));
            self.board = Some(board);
        }
    }
}

pub fn default_web() -> String {
    concat!(env!("CARGO_MANIFEST_DIR"), "/../geodb-fw-web/dist").into()
}

pub fn bridge(port: u16, tls_port: u16, via: Via, web: &str) {
    let web = PathBuf::from(web);
    if !web.join("index.html").exists() {
        eprintln!(
            "no {}: python3 crates/geodb-fw-web/build.py",
            web.join("index.html").display()
        );
        std::process::exit(1);
    }
    let pages = Arc::new(Mutex::new(Pages::default()));
    let (cmd_tx, cmd_rx) = channel::<String>();
    match via {
        Via::Net(host) => {
            let state = UdpSocket::bind(("0.0.0.0", STATE_PORT)).unwrap_or_else(|e| {
                eprintln!("UDP port {STATE_PORT}: {e} (the mirror or another bridge running?)");
                std::process::exit(1);
            });
            let board = Arc::new(Mutex::new(host));
            let (p, b) = (pages.clone(), board.clone());
            std::thread::spawn(move || relay_state(&state, &p, &b));
            let p = pages.clone();
            std::thread::spawn(move || net_commands(&cmd_rx, &p, &board));
            println!(
                "board: {}",
                host.map_or("waiting for its state packets".into(), |h| h.to_string())
            );
        }
        Via::Probe => {
            #[cfg(feature = "probe")]
            {
                let p = pages.clone();
                std::thread::spawn(move || probe_link(&cmd_rx, &p));
            }
            #[cfg(not(feature = "probe"))]
            {
                drop(cmd_rx);
                eprintln!("--probe needs the probe feature: cargo run --release -p geodb-board --features probe -- bridge --probe");
                std::process::exit(2);
            }
        }
    }
    let tls = match crate::tls::config() {
        Ok((config, names)) => {
            println!("https / wss on port {tls_port}, for {}", names.join(", "));
            Some(config)
        }
        Err(e) => {
            eprintln!("no https / wss: {e}");
            None
        }
    };
    let listen = |port: u16| {
        TcpListener::bind(("0.0.0.0", port)).unwrap_or_else(|e| {
            eprintln!("TCP port {port}: {e}");
            std::process::exit(1);
        })
    };
    let plain = listen(port);
    if let Some(config) = tls {
        let secure = listen(tls_port);
        let (pages, web, cmd_tx) = (pages.clone(), web.clone(), cmd_tx.clone());
        std::thread::spawn(move || accept(&secure, Some(config), &pages, &cmd_tx, &web));
        for ip in crate::tls::local_addresses() {
            println!("from an https page (trahe.eu), the bridge field: {ip}  (wss://{ip}:{tls_port}); a phone trusts it once:");
            println!("  http://{ip}:{port}/geodb-bridge-ca.cer -> install the profile, then Settings > General > About > Certificate Trust Settings: on");
        }
    }
    println!("the board in the browser: http://localhost:{port}/ (and this host's addresses)");
    accept(&plain, None, &pages, &cmd_tx, &web);
}

fn accept(
    listener: &TcpListener,
    tls: Option<Arc<rustls::ServerConfig>>,
    pages: &Arc<Mutex<Pages>>,
    cmd_tx: &Sender<String>,
    web: &Path,
) {
    for stream in listener.incoming().flatten() {
        let (pages, web, cmd_tx, tls) = (
            pages.clone(),
            web.to_path_buf(),
            cmd_tx.clone(),
            tls.clone(),
        );
        std::thread::spawn(move || {
            if let Err(e) = connection(stream, tls, &pages, &cmd_tx, &web) {
                if e.kind() != ErrorKind::UnexpectedEof {
                    eprintln!("connection: {e}");
                }
            }
        });
    }
}

/// A plain or a TLS connection, behind one type.
trait Conn: Read + Write + Send {}
impl<T: Read + Write + Send> Conn for T {}

/// The request head read ahead, then the rest of the stream (for the WebSocket handshake).
struct Rewind<S> {
    head: Vec<u8>,
    at: usize,
    inner: S,
}

impl<S: Read> Read for Rewind<S> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.at < self.head.len() {
            let n = buf.len().min(self.head.len() - self.at);
            buf[..n].copy_from_slice(&self.head[self.at..self.at + n]);
            self.at += n;
            return Ok(n);
        }
        self.inner.read(buf)
    }
}

impl<S: Write> Write for Rewind<S> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.inner.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

/// The board's state packets to every page; the first (or a new) sender is the board.
fn relay_state(sock: &UdpSocket, pages: &Mutex<Pages>, board: &Mutex<Option<IpAddr>>) {
    let mut buf = [0u8; 64];
    loop {
        let Ok((n, from)) = sock.recv_from(&mut buf) else {
            continue;
        };
        if n != geodb_fw_core::ui::STATE_LEN || buf[0] != b'V' {
            continue;
        }
        *board.lock().unwrap() = Some(from.ip());
        let mut p = pages.lock().unwrap();
        p.found(from.ip().to_string());
        p.send(Message::binary(buf[..n].to_vec()));
    }
}

/// The pages' commands to the board's command port; a query waits for its answer.
fn net_commands(rx: &Receiver<String>, pages: &Mutex<Pages>, board: &Mutex<Option<IpAddr>>) {
    let sock = UdpSocket::bind(("0.0.0.0", 0)).expect("UDP socket");
    sock.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
    let mut buf = [0u8; 512];
    for text in rx {
        let Some(ip) = *board.lock().unwrap() else {
            continue;
        };
        let _ = sock.send_to(text.as_bytes(), (ip, COMMAND_PORT));
        if text.starts_with("!query") {
            // (anything else that arrives meanwhile is not the answer)
            while let Ok((n, _)) = sock.recv_from(&mut buf) {
                let reply = String::from_utf8_lossy(&buf[..n]);
                if reply.starts_with("!query") {
                    pages
                        .lock()
                        .unwrap()
                        .send(Message::text(reply.into_owned()));
                    break;
                }
            }
        }
    }
}

/// The board through the probe: its state and answers out of the link block, the pages'
/// commands into it (drags that pile up while the board is busy are added together).
#[cfg(feature = "probe")]
fn probe_link(rx: &Receiver<String>, pages: &Mutex<Pages>) {
    use crate::probe::Probe;
    use geodb_fw_core::link::{self, Command};
    use std::sync::mpsc::RecvTimeoutError;
    let mut probe = loop {
        match Probe::attach() {
            Ok(p) => break p,
            Err(e) => {
                eprintln!("probe: {e}; again in 3 s");
                std::thread::sleep(Duration::from_secs(3));
            }
        }
    };
    pages.lock().unwrap().found(format!("probe {}", probe.name));
    let mut seq = None;
    let mut queue: std::collections::VecDeque<Command> = Default::default();
    let mut asked = false; // a query is out
    loop {
        let block = match probe.read() {
            Ok(b) => b,
            Err(e) => {
                eprintln!("probe: {e}");
                std::thread::sleep(Duration::from_millis(500));
                continue;
            }
        };
        if seq != Some(block[link::W_SEQ]) {
            seq = Some(block[link::W_SEQ]);
            let packet = link::state_of(&block[link::W_STATE..]);
            pages.lock().unwrap().send(Message::binary(packet.to_vec()));
        }
        let idle = Probe::idle(&block);
        if idle && asked {
            asked = false;
            let mut text = String::from("!query ");
            let _ = link::answer_of(&block[link::W_ANSWER..]).write(&mut text);
            pages.lock().unwrap().send(Message::text(text));
        }
        // what the pages sent, in order; drags that pile up meanwhile are added together
        let wait = Duration::from_millis(if queue.is_empty() { 8 } else { 1 });
        match rx.recv_timeout(wait) {
            Ok(text) => {
                for cmd in std::iter::once(text)
                    .chain(rx.try_iter())
                    .filter_map(|t| Command::parse(&t))
                {
                    match (queue.back_mut(), cmd) {
                        (Some(Command::Drag { dx, dy }), Command::Drag { dx: ex, dy: ey }) => {
                            *dx += ex;
                            *dy += ey;
                        }
                        _ => queue.push_back(cmd),
                    }
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
        if idle && !asked {
            if let Some(&cmd) = queue.front() {
                match probe.send(cmd) {
                    Ok(()) => {
                        queue.pop_front();
                        asked = matches!(cmd, Command::Query { .. });
                    }
                    Err(e) => eprintln!("probe: {e}"),
                }
            }
        }
    }
}

fn connection(
    tcp: TcpStream,
    tls: Option<Arc<rustls::ServerConfig>>,
    pages: &Mutex<Pages>,
    cmd_tx: &Sender<String>,
    web: &Path,
) -> std::io::Result<()> {
    let peer = tcp.peer_addr()?;
    // generous for the TLS handshake and the request; short once it is a WebSocket
    tcp.set_read_timeout(Some(Duration::from_secs(10)))?;
    let ctl = tcp.try_clone()?;
    let mut stream: Box<dyn Conn> = match tls {
        Some(config) => {
            let conn = rustls::ServerConnection::new(config).map_err(std::io::Error::other)?;
            Box::new(rustls::StreamOwned::new(conn, tcp))
        }
        None => Box::new(tcp),
    };
    let mut head = Vec::new();
    let mut buf = [0u8; 2048];
    while !head.windows(4).any(|w| w == b"\r\n\r\n") && head.len() < 16 * 1024 {
        let n = stream.read(&mut buf)?;
        if n == 0 {
            return Ok(());
        }
        head.extend_from_slice(&buf[..n]);
    }
    let request = String::from_utf8_lossy(&head).to_ascii_lowercase();
    if request.contains("upgrade: websocket") {
        let stream = Rewind {
            head,
            at: 0,
            inner: stream,
        };
        let ws = tungstenite::accept(stream).map_err(|e| std::io::Error::other(e.to_string()))?;
        ctl.set_read_timeout(Some(Duration::from_millis(5)))?;
        page(ws, peer, pages, cmd_tx)
    } else {
        file(&head, stream, web)
    }
}

/// One page on `/ws`: packets and answers out, commands in.
fn page(
    mut ws: tungstenite::WebSocket<Rewind<Box<dyn Conn>>>,
    peer: std::net::SocketAddr,
    pages: &Mutex<Pages>,
    cmd_tx: &Sender<String>,
) -> std::io::Result<()> {
    let (tx, rx): (Sender<Message>, Receiver<Message>) = channel();
    {
        let mut p = pages.lock().unwrap();
        if let Some(board) = &p.board {
            let _ = tx.send(Message::text(format!("board {board}")));
        }
        p.out.push(tx);
    }
    println!("page {peer} connected");
    loop {
        match ws.read() {
            Ok(Message::Text(t)) if t.starts_with('!') => {
                let _ = cmd_tx.send(t.to_string());
            }
            Ok(Message::Close(_)) => break,
            Ok(_) => {}
            Err(tungstenite::Error::Io(e))
                if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
            Err(_) => break,
        }
        let mut sent = false;
        for m in rx.try_iter() {
            if ws.write(m).is_err() {
                return Ok(());
            }
            sent = true;
        }
        if sent && ws.flush().is_err() {
            break;
        }
    }
    println!("page {peer} gone");
    Ok(())
}

/// A plain GET of a file of the web directory.
fn file(head: &[u8], mut stream: Box<dyn Conn>, web: &Path) -> std::io::Result<()> {
    let request = String::from_utf8_lossy(head);
    let path = request
        .split_whitespace()
        .nth(1)
        .unwrap_or("/")
        .split('?')
        .next()
        .unwrap_or("/")
        .trim_start_matches('/');
    if path == "bridge" {
        // the page asks whether a bridge served it (then it connects by itself)
        stream.write_all(
            b"HTTP/1.1 200 OK\r\nX-Geodb-Bridge: 1\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Expose-Headers: X-Geodb-Bridge\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        )?;
        return stream.flush();
    }
    if path == "geodb-bridge-ca.cer" {
        // the certificate authority, for a phone to install (then trust) as a profile
        let Some(der) = crate::tls::ca_der() else {
            return stream.write_all(
                b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            );
        };
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: application/x-x509-ca-cert\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            der.len()
        )?;
        stream.write_all(&der)?;
        return stream.flush();
    }
    let path = if path.is_empty() { "index.html" } else { path };
    let found = (!path.contains("..") && !path.contains('/'))
        .then(|| std::fs::read(web.join(path)).ok())
        .flatten();
    match found {
        Some(body) => {
            let kind = match path.rsplit('.').next() {
                Some("html") => "text/html; charset=utf-8",
                Some("wasm") => "application/wasm",
                Some("js") => "text/javascript",
                Some("zip") => "application/zip",
                Some("json") => "application/json",
                _ => "application/octet-stream",
            };
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nCache-Control: no-cache\r\nConnection: close\r\n\r\n",
                body.len()
            )?;
            stream.write_all(&body)?;
            stream.flush()
        }
        None => stream
            .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"),
    }
}
