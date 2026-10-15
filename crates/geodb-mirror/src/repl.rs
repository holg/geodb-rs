//! The Lisp of the mirror (AutoLISP, the `acadlisp` crate): the interpreter with the board's and geodb's
//! functions. The window's console feeds it lines; `(help)` lists the functions, one a line. The screen of the
//! board (and of the mirror) shows the form or board command that runs, in a strip at the bottom (`!code`).
//!
//! The interpreter runs on its own thread, so a script with `(sleep ...)` never freezes the window. It drives the
//! board with the same UDP commands the mouse sends (`!tap`, `!drag`, `!view`, `!spin`, `!layers`) and reads the
//! board's state from the last state packet:
//!
//! ```lisp
//! (view 48 11 1)          ; lat lon zoom        (spin T 25)   ; on at 25 deg/s, (spin nil) stops it
//! (layers 0)              ; the layer bits      (tap 560 454) ; a touch at x y
//! (repeat 120 (setq i (1+ (if i i 0))) (view 20 (* i 3) 1) (sleep 40))
//! (state)                 ; (lat lon zoom spinning fps layers)    (help) lists the rest
//! ```

use acadlisp::{Expr, Interpreter};
use geodb_core::prelude::*;
use geodb_core::spatial::haversine_distance;
use geodb_fw_core::render::View;
use geodb_fw_core::ui;
use std::net::{IpAddr, UdpSocket};
use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

const COMMAND_PORT: u16 = 7880;

/// What the board last said (the main loop fills it from the state packets).
#[derive(Clone, Copy)]
pub struct Snapshot {
    pub view: View,
    pub spin: ui::Spin,
    pub fps: u32,
    pub options: u8,
    pub selected: Option<u32>,
    pub board: Option<IpAddr>,
}

/// Something only the window can do.
pub enum Request {
    Screenshot(String),
}

/// One documented function: name, group, usage, what it does, an example.
pub struct Doc {
    pub name: &'static str,
    pub group: &'static str,
    pub usage: &'static str,
    pub text: &'static str,
    pub example: &'static str,
}

const fn doc(
    name: &'static str,
    group: &'static str,
    usage: &'static str,
    text: &'static str,
    example: &'static str,
) -> Doc {
    Doc {
        name,
        group,
        usage,
        text,
        example,
    }
}

pub const BOARD: &str = "THE BOARD";
pub const GEODB: &str = "THE GEODB (geodb-core; the dataset loads on first use, a few seconds)";

pub const DOCS: &[Doc] = &[
    doc("view", BOARD, "(view lat lon [zoom]) (view \"place\" [zoom])", "Sets the view at once. A place is a name, a find or city result, or lat lon. Zoom 1 shows the whole globe, 12 and more the scope view, 4000 is street level.", "(view \"Olfen\" 8)"),
    doc("spin", BOARD, "(spin T|nil [dps])", "Spin on (T) or off (nil), optionally at dps degrees per second; a negative speed turns the other way.", "(spin T -25)"),
    doc("layers", BOARD, "(layers bits)", "Sets the layers: 1 relief, 2 isohypses, 4 coastline, 8 city dots, 16 query ring (add 64 for the LAYERS button's direction).", "(layers 5)   ; coast and relief"),
    doc("tap", BOARD, "(tap x y)", "A touch at x y of the 800 x 480 screen, like a finger (buttons, a city, a point on the globe).", "(tap 640 454)   ; the LAYERS button"),
    doc("drag", BOARD, "(drag dx dy)", "Pans the globe by pixels, like a finger dragging; (release) lets go.", "(drag 40 0) (release)"),
    doc("release", BOARD, "(release)", "Ends a drag (the globe may coast on).", "(release)"),
    doc("fly", BOARD, "(fly place [zoom ms]) (fly from to [zoom ms])", "A smooth move to a place (lat lon, a name, a find or city result): ease in and out, the longitude takes the short way. With two places it is a trip from the first to the second: out to the whole globe, across, in (zoom 5, 5000 ms).", "(fly \"Ludinghausen\" \"Wuhan\")"),
    doc("sleep", BOARD, "(sleep ms)", "Waits ms (in a headless video: renders ms of video). The window never freezes meanwhile; Ctrl-C stops a script.", "(sleep 2000)"),
    doc("state", BOARD, "(state)", "What the board shows now: (lat lon zoom spinning fps layers).", "(state)"),
    doc("lat", BOARD, "(lat) (lon) (zoom) (fps) (spin-dps)", "Single readings of the board's state.", "(+ (lon) 10)"),
    doc("lon", "", "(lon)", "The longitude the board looks at; see (help lat).", "(lon)"),
    doc("zoom", "", "(zoom)", "The board's zoom; see (help lat).", "(zoom)"),
    doc("fps", "", "(fps)", "The board's frame rate; see (help lat).", "(fps)"),
    doc("spin-dps", "", "(spin-dps)", "The board's spin speed in degrees per second (negative: the other way).", "(spin-dps)"),
    doc("layer-bits", "", "(layer-bits)", "The layer bits in force; see (help layers).", "(layer-bits)"),
    doc("selected", BOARD, "(selected)", "The index of the city selected on the board (its card is open), or nil.", "(selected)"),
    doc("info", BOARD, "(info)", "The board's !info line: firmware, slot, uptime, fps, build id, image size, layers.", "(info)"),
    doc("screenshot", BOARD, "(screenshot \"f.png\")", "Saves the board's screen as a png.", "(screenshot \"alps.png\")"),
    doc("find", GEODB, "(find \"text\" [n])", "Cities, states and countries matching a text, best first (at most n, default 8): a list of (kind name state country lat lon). Filter it with foreach: (foreach c (find \"Berlin\" 100) (if (equal (nth 3 c) \"Germany\") (princ (nth 1 c)))). A call is (find \"x\"), not find(\"x\").", "(find \"Olfen\")"),
    doc("city", GEODB, "(city \"name\")", "The best match for a name (a city, else a state or country): (lat lon name country), or nil.", "(city \"Munich\")"),
    doc("goto", GEODB, "(goto \"name\" [zoom])", "Shows a place on the board at once (zoom 6 by default) and returns its name.", "(goto \"Bavaria\" 4)"),
    doc("hop", GEODB, "(hop place [zoom ms]) (hop from to [zoom ms])", "A trip to a place from where the view is (or from the first of two): out to the whole globe, across, in to zoom (default 5, in 4000 ms). Returns the name.", "(hop \"Tokyo\" 5 5000)"),
    doc("nearest", GEODB, "(nearest lat lon [n]) (nearest \"place\" [n])", "The n nearest cities (default 5): a list of (name country km lat lon).", "(nearest \"Olfen\" 3)"),
    doc("here", GEODB, "(here [n])", "The n cities nearest to where the board looks (default 5), like its side panel.", "(foreach c (here 5) (princ (car c)) (princ \" \"))"),
    doc("cities", GEODB, "(cities country [state [n]])", "The cities of a country (name or ISO code), or of one of its states (a part of the name is enough), at most n (default 20): a list of (name state population lat lon). Largest first where the dataset knows the population (it mostly does not: then in dataset order). Made for foreach.", "(foreach c (cities \"DE\" \"Bavaria\" 5) (princ (car c)) (princ \" \"))"),
    doc("count-cities", GEODB, "(count-cities country [state])", "How many cities a country has in the dataset, or one of its states (a part of the name is enough).", "(count-cities \"DE\" \"North Rhine\")"),
    doc("states", GEODB, "(states country)", "The names of the states or provinces of a country (name or ISO code).", "(states \"Germany\")"),
    doc("country", GEODB, "(country \"DE\")", "A country by its ISO code: (name capital region lat lon), or nil.", "(country \"JP\")"),
    doc("distance", GEODB, "(distance la1 lo1 la2 lo2) (distance place place)", "Great-circle distance in km between two points, or two places: names, find results, city results.", "(distance \"Ludinghausen\" \"Olfen\")"),
    doc("load", "LISP", "(load \"f.lsp\")", "Runs a script file (in the app: the demo folder is the working directory). The screen shows each board command it runs.", "(load \"TOUR.LSP\")"),
    doc("note", BOARD, "(note \"text\")", "A caption over the running code line on the screen (yellow, about 30 characters fit): says what the script is doing. (note \"\") clears it. It stays until the script ends.", "(note \"Layer 2: relief shading\")"),
    doc("pwd", "LISP", "(pwd)", "Where we are: the working directory, and the folders (load \"X.LSP\") looks in.", "(pwd)"),
    doc("ls", "LISP", "(ls)", "The scripts (*.LSP) (load) finds, with their first comment line.", "(ls)"),
    doc("help-window", "LISP", "(help-window)", "Opens the help in a window of its own (also Ctrl-H): the functions, where we are and the scripts to load; a click on a green line copies it to the clipboard, ready to paste here with Ctrl-V.", "(help-window)"),
    doc("help", "LISP", "(help) (help name)", "(help) lists the functions; (help find) tells about one.", "(help hop)"),
];

pub const LISP_NOTE: &[&str] = &[
    "AutoLISP: setq defun repeat while if cond foreach list car cdr nth + - * / sin cos expt sqrt ...",
    "A call is (f arg arg), not f(arg). Several forms on a line run one after the other; the screen shows the one that runs.",
    "examples: (goto \"Tokyo\" 5)   (setq i 0) (repeat 120 (setq i (1+ i)) (view 20 (* i 3) 1) (sleep 40))",
];

/// The help in a window of its own: another instance of this program (its own scopekit window).
pub fn open_help_window() -> bool {
    match std::env::current_exe()
        .and_then(|exe| std::process::Command::new(exe).arg("--help-window").spawn())
    {
        Ok(_) => true,
        Err(e) => {
            say(&format!("the help window does not open: {e}"));
            false
        }
    }
}

/// `(help)`: every function in a line; `(help name)`: one in full.
fn help(topic: Option<&str>) {
    let Some(topic) = topic else {
        let mut group = "";
        for d in DOCS.iter().filter(|d| !d.group.is_empty()) {
            if d.group != group {
                group = d.group;
                say("");
                say(group);
            }
            let short = d.text.split(". ").next().unwrap_or("");
            let short: String = if short.chars().count() > 70 {
                short.chars().take(67).chain("...".chars()).collect()
            } else {
                short.to_string()
            };
            say(&format!("  {:<48} {short}", d.usage));
        }
        say("");
        LISP_NOTE.iter().for_each(|l| say(l));
        say("(help name) tells about one function.");
        return;
    };
    let wanted = topic.trim_matches(|c| c == '(' || c == ')').to_lowercase();
    match DOCS.iter().find(|d| d.name == wanted) {
        Some(d) => {
            say(d.usage);
            say(&format!("  {}", d.text));
            say(&format!("  example: {}", d.example));
        }
        None => say(&format!(
            "no help for {topic:?}: (help) lists the functions"
        )),
    }
}

struct Interrupted;

fn num(e: &Expr) -> Option<f64> {
    match e {
        Expr::Integer(i) => Some(f64::from(*i)),
        Expr::Real(r) => Some(*r),
        _ => None,
    }
}

fn n_of(a: &[Expr], i: usize) -> f64 {
    a.get(i).and_then(num).unwrap_or(0.0)
}

fn truthy(e: &Expr) -> bool {
    !matches!(e, Expr::Nil)
}

/// The code as the screen shows it: symbols are read in upper case, so they are lower case again (strings stay).
fn readable(code: &str) -> String {
    let mut in_string = false;
    code.chars()
        .map(|c| {
            if c == '"' {
                in_string = !in_string;
            }
            if in_string {
                c
            } else {
                c.to_ascii_lowercase()
            }
        })
        .collect()
}

/// The layers a `(layers bits)` switches on, in words.
fn layer_names(bits: u8) -> String {
    let names: Vec<&str> = [
        (4, "coast"),
        (1, "relief"),
        (2, "isohypses"),
        (8, "dots"),
        (16, "ring"),
    ]
    .iter()
    .filter(|(b, _)| bits & b != 0)
    .map(|(_, n)| *n)
    .collect();
    if names.is_empty() {
        "none".into()
    } else {
        names.join(" + ")
    }
}

/// `(name arg ...)` as written, for the code strip.
fn call_text(name: &str, args: &[Expr]) -> String {
    let mut text = format!("({name}");
    for a in args {
        text.push(' ');
        text.push_str(&a.to_string());
    }
    text.push(')');
    readable(&text)
}

/// A board command: the screen shows the call while it runs.
fn defc<F>(it: &mut Interpreter, board: &Arc<dyn Board>, name: &'static str, f: F)
where
    F: Fn(&[Expr]) -> Expr + Send + Sync + 'static,
{
    let board = board.clone();
    def(it, name, move |a| {
        let mut shown = call_text(name, a);
        if name == "layers" {
            // what the bits mean, so the screen says what is going on
            shown.push_str(&format!("  = {}", layer_names(n_of(a, 0) as u8)));
        }
        board.send(&format!("!code {shown}"));
        f(a)
    });
}

fn def<F>(it: &mut Interpreter, name: &str, f: F)
where
    F: Fn(&[Expr]) -> Expr + Send + Sync + 'static,
{
    it.register_fn(name, move |_, args| f(args));
}

type Db = GeoDb<DefaultBackend>;
/// The dataset, loaded on a thread of its own (so Ctrl-C can still stop a script that waits for it).
struct Data {
    cell: OnceLock<Option<Db>>,
    started: AtomicBool,
}

type Dataset = Arc<Data>;

fn new_dataset() -> Dataset {
    Arc::new(Data {
        cell: OnceLock::new(),
        started: AtomicBool::new(false),
    })
}

/// Set by Ctrl-C: whatever waits (a sleep, the dataset loading) ends the script.
static INTERRUPT: AtomicBool = AtomicBool::new(false);

/// Tests share the process-wide interrupt flag: they take turns.
#[cfg(test)]
static TEST_TURN: Mutex<()> = Mutex::new(());

/// What runs now: the script (or the first line typed), for the console's title.
static NOW: Mutex<String> = Mutex::new(String::new());

pub fn now() -> String {
    NOW.lock().map(|n| n.clone()).unwrap_or_default()
}

fn set_now(text: &str) {
    if let Ok(mut n) = NOW.lock() {
        *n = text.chars().take(50).collect();
    }
}

/// `GEODB_DATA`, else the one in an app bundle's Resources, else the repository's.
fn dataset_path() -> String {
    const FILE: &str = "countries+states+cities.json.gz";
    if let Ok(p) = std::env::var("GEODB_DATA") {
        return p;
    }
    let bundled = std::env::current_exe()
        .ok()
        .and_then(|e| Some(e.parent()?.parent()?.join("Resources").join(FILE)));
    match bundled {
        Some(p) if p.exists() => p.to_string_lossy().into_owned(),
        _ => concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../geodb-core/data/countries+states+cities.json.gz"
        )
        .into(),
    }
}

/// The dataset, loaded on first use (a few seconds; the binary cache next to it makes later starts faster).
/// Starts loading the dataset on its own thread, once.
fn start_load(data: &Dataset) {
    if data.started.swap(true, Ordering::Relaxed) {
        return;
    }
    let data = data.clone();
    std::thread::spawn(move || {
        data.cell.get_or_init(|| {
            let path = dataset_path();
            say(&format!("loading the dataset {path} ..."));
            match Db::load_from_path(&path, None) {
                Ok(db) => {
                    say("the dataset is loaded");
                    Some(db)
                }
                Err(e) => {
                    say(&format!(
                        "the dataset does not load: {e} (GEODB_DATA names the file)"
                    ));
                    None
                }
            }
        });
    });
}

/// The one dataset of the process (a rebuilt interpreter keeps it).
fn global_data() -> Dataset {
    static DATA: OnceLock<Dataset> = OnceLock::new();
    DATA.get_or_init(new_dataset).clone()
}

/// Loads the dataset in the background now, so the first `(find ...)` is quick.
pub fn preload() {
    start_load(&global_data());
}

fn dataset(data: &Dataset) -> Option<&Db> {
    if let Some(d) = data.cell.get() {
        return d.as_ref();
    }
    start_load(data);
    loop {
        if let Some(d) = data.cell.get() {
            return d.as_ref();
        }
        if INTERRUPT.load(Ordering::Relaxed) {
            resume_unwind(Box::new(Interrupted));
        }
        std::thread::sleep(Duration::from_millis(30));
    }
}

fn real(v: f64) -> Expr {
    Expr::Real(v)
}

fn text_of(a: &[Expr], i: usize) -> Option<&str> {
    match a.get(i) {
        Some(Expr::String(s)) => Some(s),
        _ => None,
    }
}

/// A place a text finds: cities, states and countries, best first: (kind, name, state, country, lat, lon).
type Place = (&'static str, String, String, String, f64, f64);

fn find_places(db: &Db, query: &str, limit: usize) -> Vec<Place> {
    db.smart_search(query)
        .into_iter()
        .filter_map(|hit| match hit.item {
            SmartItem::City {
                city,
                state,
                country,
            } => Some((
                "city",
                city.name().to_string(),
                state.name().to_string(),
                country.name().to_string(),
                city.lat()?,
                city.lng()?,
            )),
            SmartItem::State { state, country } => Some((
                "state",
                state.name().to_string(),
                String::new(),
                country.name().to_string(),
                state.lat()?,
                state.lng()?,
            )),
            SmartItem::Country(country) => Some((
                "country",
                country.name().to_string(),
                String::new(),
                String::new(),
                country.lat()?,
                country.lng()?,
            )),
        })
        .take(limit)
        .collect()
}

/// A position from what a script passes: a place name ("Olfen"), a result of `find` (one place or the list of
/// them), of `city` ((lat lon ...)) or a plain (lat lon).
fn position_of(db: &Db, e: &Expr) -> Option<(f64, f64)> {
    match e {
        Expr::String(name) => find_places(db, name, 1)
            .pop()
            .map(|(_, _, _, _, la, lo)| (la, lo)),
        Expr::List(items) => match items.as_slice() {
            // a `find` entry: (kind name state country lat lon)
            [Expr::String(_), _, _, _, la, lo] => Some((num(la)?, num(lo)?)),
            [la, lo, ..] if num(la).is_some() => Some((num(la)?, num(lo)?)),
            // the list `find` returns: its first entry
            [first @ Expr::List(_), ..] => position_of(db, first),
            _ => None,
        },
        _ => None,
    }
}

/// A country by ISO code or by (a part of) its name.
fn country_of<'a>(db: &'a Db, text: &str) -> Option<&'a Country<DefaultBackend>> {
    db.find_country_by_iso2(&text.to_uppercase()).or_else(|| {
        let wanted = text.to_lowercase();
        let by_name = db.find_countries_by_substring(text);
        by_name
            .iter()
            .find(|c| c.name().to_lowercase() == wanted)
            .or_else(|| by_name.first())
            .copied()
    })
}

fn nearest_cities(db: &Db, lat: f64, lon: f64, n: usize) -> Expr {
    Expr::List(
        db.find_nearest(lat, lon, n)
            .into_iter()
            .map(|(city, _, country)| {
                let (la, lo) = (city.lat().unwrap_or(0.0), city.lng().unwrap_or(0.0));
                Expr::List(vec![
                    Expr::String(city.name().to_string()),
                    Expr::String(country.iso2().to_string()),
                    real(haversine_distance(lat, lon, la, lo)),
                    real(la),
                    real(lo),
                ])
            })
            .collect(),
    )
}

/// The board a script drives: the real one over UDP, or a simulated one (the headless video writer).
pub trait Board: Send + Sync {
    /// A board command (`!view ...`, `!tap ...`); false when it could not be sent.
    fn send(&self, text: &str) -> bool;
    /// What the board shows now.
    fn state(&self) -> Snapshot;
    /// The board's `!info` line.
    fn info(&self) -> Option<String>;
    /// Lets `ms` pass (real time on the board, virtual and frame by frame in a video).
    fn sleep(&self, ms: u64);
    fn screenshot(&self, name: &str);
}

/// The real board, by UDP; its state comes from the state packets in `snap`.
pub struct UdpBoard {
    pub snap: Arc<Mutex<Snapshot>>,
    sock: UdpSocket,
    abort: Arc<AtomicBool>,
    requests: Mutex<Sender<Request>>,
    /// The code line last shown: sent again now and then, the board lets a line expire (a script that went away).
    code: Mutex<String>,
    note: Mutex<String>,
}

impl UdpBoard {
    fn new(snap: Arc<Mutex<Snapshot>>, requests: Sender<Request>, abort: Arc<AtomicBool>) -> Self {
        let sock = UdpSocket::bind(("0.0.0.0", 0)).expect("script socket");
        let _ = sock.set_read_timeout(Some(Duration::from_millis(600)));
        UdpBoard {
            snap,
            sock,
            abort,
            requests: Mutex::new(requests),
            code: Mutex::new(String::new()),
            note: Mutex::new(String::new()),
        }
    }
}

impl Board for UdpBoard {
    fn send(&self, text: &str) -> bool {
        // no packet heard yet: the board's address is not known
        let Some(ip) = self.snap.lock().unwrap().board else {
            return false;
        };
        if text.starts_with("!code") {
            *self.code.lock().unwrap() = text.to_string();
        }
        if text.starts_with("!note") {
            *self.note.lock().unwrap() = text.to_string();
        }
        let _ = self.sock.send_to(text.as_bytes(), (ip, COMMAND_PORT));
        true
    }

    fn state(&self) -> Snapshot {
        *self.snap.lock().unwrap()
    }

    fn info(&self) -> Option<String> {
        let ip = self.snap.lock().unwrap().board?;
        let _ = self.sock.send_to(b"!info", (ip, COMMAND_PORT));
        let mut buf = [0u8; 300];
        let (len, _) = self.sock.recv_from(&mut buf).ok()?;
        Some(String::from_utf8_lossy(&buf[..len]).into_owned())
    }

    fn sleep(&self, ms: u64) {
        let end = Instant::now() + Duration::from_millis(ms);
        let mut refreshed = Instant::now();
        while Instant::now() < end {
            if self.abort.load(Ordering::Relaxed) || INTERRUPT.load(Ordering::Relaxed) {
                resume_unwind(Box::new(Interrupted));
            }
            // a long wait keeps its code line alive on the board
            if refreshed.elapsed() >= Duration::from_secs(3) {
                refreshed = Instant::now();
                let code = self.code.lock().unwrap().clone();
                if code.len() > "!code".len() {
                    self.send(&code);
                }
                let note = self.note.lock().unwrap().clone();
                if note.len() > "!note".len() {
                    self.send(&note);
                }
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn screenshot(&self, name: &str) {
        let _ = self
            .requests
            .lock()
            .unwrap()
            .send(Request::Screenshot(name.into()));
    }
}

fn flag(on: bool) -> Expr {
    if on {
        Expr::T
    } else {
        Expr::Nil
    }
}

/// What a call that moves the view asks for, found from the types of its arguments (see [`plan`]).
struct Plan {
    /// Where the trip starts, when two places were given (else: where the view is).
    from: Option<(f64, f64)>,
    to: (f64, f64),
    zoom: Option<f64>,
    ms: Option<f64>,
    /// The name of the place `to`, when it was one.
    label: String,
}

/// Routes the arguments of `view`, `fly` and `hop` by their types, so each reads naturally:
///
/// * a place is a name `"Olfen"`, a `find` or `city` result, or a point as two numbers `48.1 11.6`;
/// * one place is where to go, two places are from and to (`(fly "Ludinghausen" "Wuhan")`);
/// * the plain numbers that follow are the zoom, then the milliseconds.
fn plan(data: &Dataset, args: &[Expr]) -> Option<Plan> {
    let mut places: Vec<((f64, f64), String)> = Vec::new();
    let mut scalars: Vec<f64> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match (&args[i], args.get(i + 1)) {
            // two numbers in a row before any zoom: a point
            (a, Some(b))
                if places.is_empty()
                    && scalars.is_empty()
                    && num(a).is_some()
                    && num(b).is_some() =>
            {
                places.push(((num(a)?, num(b)?), String::new()));
                i += 2;
                continue;
            }
            (a, _) if num(a).is_some() => scalars.push(num(a)?),
            (e @ (Expr::String(_) | Expr::List(_)), _) => {
                let found = dataset(data).and_then(|db| {
                    let label = match e {
                        Expr::String(name) => find_places(db, name, 1).pop().map(|p| p.1),
                        _ => None,
                    };
                    position_of(db, e).map(|pos| (pos, label.unwrap_or_default()))
                });
                match found {
                    Some(p) => places.push(p),
                    None => {
                        say(&format!("no place matches {e}"));
                        return None;
                    }
                }
            }
            (other, _) => {
                say(&format!("I do not know what to do with {other}: a place is a name, a find result or lat lon"));
                return None;
            }
        }
        i += 1;
    }
    let (from, to) = match places.as_slice() {
        [to] => (None, to.clone()),
        [from, to] => (Some(from.0), to.clone()),
        _ => {
            say("wants a place: (fly \"Olfen\"), (fly \"Olfen\" \"Wuhan\"), (fly 48.1 11.6 5)");
            return None;
        }
    };
    Some(Plan {
        from,
        to: to.0,
        zoom: scalars.first().copied(),
        ms: scalars.get(1).copied(),
        label: to.1,
    })
}

/// A trip: out to the whole globe, across, in to `zoom`; it starts at `from` (a jump there first).
fn route(board: &dyn Board, from: (f64, f64), to: (f64, f64), zoom: f64, ms: f64) {
    let here = board.state().view;
    if (f64::from(here.lat) - from.0).abs() > 0.01 || (f64::from(here.lon) - from.1).abs() > 0.01 {
        board.send(&format!(
            "!view {} {} {}",
            from.0,
            from.1,
            f64::from(here.zoom).max(1.3)
        ));
    }
    fly(board, from.0, from.1, 1.3, ms * 0.25);
    fly(board, to.0, to.1, 1.3, ms * 0.35);
    fly(board, to.0, to.1, zoom.max(1.0), ms * 0.4);
}

/// A smooth move of the view to (la, lo, zoom) in `ms`: the zoom changes geometrically, the longitude takes
/// the short way, an ease in and out.
fn fly(board: &dyn Board, la: f64, lo: f64, zoom: f64, ms: f64) {
    let s = board.state();
    let (la0, lo0, z0) = (
        f64::from(s.view.lat),
        f64::from(s.view.lon),
        f64::from(s.view.zoom),
    );
    let mut dlo = lo - lo0;
    if dlo > 180.0 {
        dlo -= 360.0;
    }
    if dlo < -180.0 {
        dlo += 360.0;
    }
    let steps = ((ms / 40.0) as u32).max(1);
    for i in 1..=steps {
        let u = f64::from(i) / f64::from(steps);
        let k = u * u * (3.0 - 2.0 * u);
        board.send(&format!(
            "!view {} {} {}",
            la0 + k * (la - la0),
            lo0 + k * dlo,
            z0 * (zoom / z0).powf(k)
        ));
        board.sleep(40);
    }
}

/// Where `(load "X.LSP")` looks: the working directory, the demo folder of an app bundle, the repository's.
pub fn script_dirs() -> Vec<std::path::PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        dirs.push(cwd);
    }
    if let Some(d) = std::env::current_exe()
        .ok()
        .and_then(|e| Some(e.parent()?.parent()?.join("Resources").join("demo")))
    {
        dirs.push(d);
    }
    dirs.push(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../firmware/stm32f769i-disco/demo"),
    );
    let mut seen = Vec::new();
    dirs.into_iter()
        .filter_map(|d| d.canonicalize().ok())
        .filter(|d| {
            d.is_dir()
                && !{
                    let dup = seen.contains(d);
                    seen.push(d.clone());
                    dup
                }
        })
        .collect()
}

/// The script `name` (as typed) in the first folder that has it.
pub fn find_script(name: &str) -> Option<std::path::PathBuf> {
    let direct = std::path::Path::new(name);
    if direct.is_absolute() {
        return direct.exists().then(|| direct.to_path_buf());
    }
    script_dirs()
        .into_iter()
        .map(|d| d.join(name))
        .find(|p| p.is_file())
}

/// The scripts (`*.LSP`) in the script folders: path and the first comment line.
pub fn scripts() -> Vec<(std::path::PathBuf, String)> {
    let mut out = Vec::new();
    for dir in script_dirs() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut found: Vec<_> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("lsp")))
            .collect();
        found.sort();
        for p in found {
            let note = std::fs::read_to_string(&p)
                .ok()
                .and_then(|t| {
                    t.lines()
                        .find(|l| l.trim_start().starts_with(';'))
                        .map(|l| l.trim_start().trim_start_matches(';').trim().to_string())
                })
                .unwrap_or_default();
            out.push((p, note));
        }
    }
    out
}

/// The script name of a `(load "X.LSP")` form.
fn load_name(form: &Expr) -> Option<&str> {
    let Expr::List(items) = form else { return None };
    match items.as_slice() {
        [Expr::Symbol(head), Expr::String(name)] if head.eq_ignore_ascii_case("load") => Some(name),
        _ => None,
    }
}

/// `(help find)` as `(help "find")`: the symbol is the topic, not a variable.
fn bare_help(form: &Expr) -> Option<Expr> {
    let Expr::List(items) = form else { return None };
    let [Expr::Symbol(head), rest @ ..] = items.as_slice() else {
        return None;
    };
    if rest.is_empty()
        || !head.to_string().eq_ignore_ascii_case("help")
        || !rest.iter().all(|e| matches!(e, Expr::Symbol(_)))
    {
        return None;
    }
    let mut call = vec![Expr::Symbol(head.clone())];
    call.extend(rest.iter().map(|e| Expr::String(e.to_string())));
    Some(Expr::List(call))
}

/// Runs the forms of `source` one by one, the screen showing the one that runs; returns the results.
pub fn run_forms(it: &mut Interpreter, board: &dyn Board, source: &str) -> Vec<Expr> {
    let forms = acadlisp::parser::Parser::from_input(source).parse_all();
    let mut results = Vec::new();
    for form in &forms {
        // (help find): the name is not a variable to evaluate
        let help_call;
        let form = match bare_help(form) {
            Some(call) => {
                help_call = call;
                &help_call
            }
            None => form,
        };
        // (load "X.LSP"): run the file here, form by form, so the screen shows each line of the script and a
        // (load "LIB.LSP") inside it is found the same way (the working directory, then the demo folders)
        if let Some(name) = load_name(form) {
            match find_script(name).map(|p| std::fs::read_to_string(&p)) {
                Some(Ok(text)) => {
                    let before = now();
                    set_now(&format!("(load \"{name}\")"));
                    run_forms(it, board, &text);
                    set_now(&before);
                    results.push(Expr::T);
                }
                Some(Err(e)) => {
                    say(&format!("cannot read {name}: {e}"));
                    results.push(Expr::Nil);
                }
                None => {
                    say(&format!(
                        "cannot find {name}; (pwd) lists where (load) looks"
                    ));
                    results.push(Expr::Nil);
                }
            }
            continue;
        }
        let shown = form.to_string();
        // (hold 2000) and (sleep 2000) wait: the line that ran before stays on the screen
        let waits = matches!(form, Expr::List(items) if matches!(items.first(),
            Some(Expr::Symbol(h)) if h.eq_ignore_ascii_case("hold") || h.eq_ignore_ascii_case("sleep")));
        if !waits {
            let text: Vec<&str> = shown.split_whitespace().collect();
            board.send(&format!("!code {}", readable(&text.join(" "))));
        }
        results.push(it.eval(form));
    }
    board.send("!code");
    results
}

/// The interpreter with the board and geodb functions.
pub fn interpreter(board: Arc<dyn Board>) -> Interpreter {
    let send = {
        let board = board.clone();
        move |text: String| flag(board.send(&text))
    };
    let mut it = Interpreter::new();
    let data: Dataset = global_data();
    let n = n_of;
    {
        let send = send.clone();
        defc(&mut it, &board, "tap", move |a| {
            send(format!("!tap {} {}", n(a, 0) as i32, n(a, 1) as i32))
        });
    }
    {
        let send = send.clone();
        defc(&mut it, &board, "drag", move |a| {
            send(format!("!drag {} {}", n(a, 0) as i32, n(a, 1) as i32))
        });
    }
    {
        let send = send.clone();
        defc(&mut it, &board, "release", move |_| send("!release".into()));
    }
    {
        let (data, board) = (data.clone(), board.clone());
        defc(&mut it, &board.clone(), "view", move |a| {
            let Some(p) = plan(&data, a) else {
                return Expr::Nil;
            };
            let zoom = p.zoom.unwrap_or_else(|| f64::from(board.state().view.zoom));
            flag(board.send(&format!("!view {} {} {}", p.to.0, p.to.1, zoom.max(1.0))))
        });
    }
    {
        let send = send.clone();
        defc(&mut it, &board, "spin", move |a| {
            let on = a.first().is_some_and(truthy);
            match a.get(1).and_then(num) {
                Some(dps) => send(format!("!spin {} {dps}", if on { "on" } else { "off" })),
                None => send(format!("!spin {}", if on { "on" } else { "off" })),
            }
        });
    }
    {
        let send = send.clone();
        defc(&mut it, &board, "layers", move |a| {
            send(format!("!layers {}", n(a, 0) as i32))
        });
    }
    {
        let board = board.clone();
        def(&mut it, "sleep", move |a| {
            board.sleep(n(a, 0).max(0.0) as u64);
            Expr::T
        });
    }
    {
        let board = board.clone();
        def(&mut it, "info", move |_| {
            board.info().map_or(Expr::Nil, Expr::String)
        });
    }
    {
        let board = board.clone();
        def(&mut it, "state", move |_| {
            let s = board.state();
            Expr::List(vec![
                Expr::Real(f64::from(s.view.lat)),
                Expr::Real(f64::from(s.view.lon)),
                Expr::Real(f64::from(s.view.zoom)),
                flag(s.spin.on),
                Expr::Integer(s.fps as i32),
                Expr::Integer(i32::from(s.options)),
            ])
        });
    }
    // single readings
    for (name, get) in [
        (
            "lat",
            (|s: &Snapshot| f64::from(s.view.lat)) as fn(&Snapshot) -> f64,
        ),
        ("lon", |s| f64::from(s.view.lon)),
        ("zoom", |s| f64::from(s.view.zoom)),
        ("fps", |s| f64::from(s.fps)),
        ("spin-dps", |s| f64::from(s.spin.dps)),
    ] {
        let board = board.clone();
        def(&mut it, name, move |_| Expr::Real(get(&board.state())));
    }
    {
        let board = board.clone();
        def(&mut it, "selected", move |_| {
            board
                .state()
                .selected
                .map_or(Expr::Nil, |i| Expr::Integer(i as i32))
        });
    }
    {
        let board = board.clone();
        def(&mut it, "layer-bits", move |_| {
            Expr::Integer(i32::from(board.state().options))
        });
    }
    {
        let board = board.clone();
        def(&mut it, "screenshot", move |a| {
            board.screenshot(text_of(a, 0).unwrap_or("board-script.png"));
            Expr::T
        });
    }
    // smooth moves (the demo scripts use them): the board gets a view every 40 ms, `sleep` in between
    {
        let (data, board) = (data.clone(), board.clone());
        defc(&mut it, &board.clone(), "fly", move |a| {
            let Some(p) = plan(&data, a) else {
                return Expr::Nil;
            };
            match p.from {
                // from one place to another: there, then the trip (out, across, in)
                Some(from) => route(
                    &*board,
                    from,
                    p.to,
                    p.zoom.unwrap_or(5.0),
                    p.ms.unwrap_or(5000.0),
                ),
                None => {
                    let zoom = p.zoom.unwrap_or_else(|| f64::from(board.state().view.zoom));
                    fly(
                        &*board,
                        p.to.0,
                        p.to.1,
                        zoom.max(1.0),
                        p.ms.unwrap_or(3000.0),
                    );
                }
            }
            Expr::T
        });
    }
    // the geodb-core functions
    {
        let data = data.clone();
        def(&mut it, "find", move |a| {
            let Some(q) = text_of(a, 0) else {
                say("find wants a text: (find \"olfen\") (a call is (f arg), not f(arg))");
                return Expr::Nil;
            };
            let Some(db) = dataset(&data) else {
                return Expr::Nil;
            };
            let n = a.get(1).and_then(num).map_or(8, |v| v as usize);
            let found = find_places(db, q, n);
            if found.is_empty() {
                say(&format!("nothing found for {q:?}"));
            }
            Expr::List(
                found
                    .into_iter()
                    .map(|(kind, name, state, country, lat, lon)| {
                        Expr::List(vec![
                            Expr::String(kind.into()),
                            Expr::String(name),
                            Expr::String(state),
                            Expr::String(country),
                            real(lat),
                            real(lon),
                        ])
                    })
                    .collect(),
            )
        });
    }
    {
        let data = data.clone();
        def(&mut it, "city", move |a| {
            let (Some(q), Some(db)) = (text_of(a, 0), dataset(&data)) else {
                return Expr::Nil;
            };
            match find_places(db, q, 1).pop() {
                Some((_, name, _, country, lat, lon)) => Expr::List(vec![
                    real(lat),
                    real(lon),
                    Expr::String(name),
                    Expr::String(country),
                ]),
                None => Expr::Nil,
            }
        });
    }
    {
        let (data, send) = (data.clone(), send.clone());
        defc(&mut it, &board, "goto", move |a| {
            let (Some(q), Some(db)) = (text_of(a, 0), dataset(&data)) else {
                return Expr::Nil;
            };
            let zoom = a.get(1).and_then(num).unwrap_or(6.0).max(1.0);
            match find_places(db, q, 1).pop() {
                Some((_, name, _, _, lat, lon)) => {
                    send(format!("!view {lat} {lon} {zoom}"));
                    Expr::String(name)
                }
                None => {
                    say(&format!("no city matches {q:?}"));
                    Expr::Nil
                }
            }
        });
    }
    {
        let data = data.clone();
        def(&mut it, "nearest", move |a| {
            let Some(db) = dataset(&data) else {
                return Expr::Nil;
            };
            // (nearest lat lon [n]) or (nearest "place" [n])
            let (pos, n) = if a.first().and_then(num).is_some() {
                (
                    Some((n_of(a, 0), n_of(a, 1))),
                    a.get(2).and_then(num).map_or(5, |v| v as usize),
                )
            } else {
                (
                    a.first().and_then(|e| position_of(db, e)),
                    a.get(1).and_then(num).map_or(5, |v| v as usize),
                )
            };
            match pos {
                Some((la, lo)) => nearest_cities(db, la, lo, n),
                None => {
                    say("nearest wants (lat lon [n]) or (\"place\" [n])");
                    Expr::Nil
                }
            }
        });
    }
    {
        let (data, board) = (data.clone(), board.clone());
        def(&mut it, "here", move |a| {
            let Some(db) = dataset(&data) else {
                return Expr::Nil;
            };
            let view = board.state().view;
            let n = a.first().and_then(num).map_or(5, |v| v as usize);
            nearest_cities(db, f64::from(view.lat), f64::from(view.lon), n)
        });
    }
    {
        let data = data.clone();
        def(&mut it, "country", move |a| {
            let (Some(code), Some(db)) = (text_of(a, 0), dataset(&data)) else {
                return Expr::Nil;
            };
            match db.find_country_by_iso2(&code.to_uppercase()) {
                Some(c) => Expr::List(vec![
                    Expr::String(c.name().to_string()),
                    Expr::String(c.capital().to_string()),
                    Expr::String(c.region().to_string()),
                    real(c.lat().unwrap_or(0.0)),
                    real(c.lng().unwrap_or(0.0)),
                ]),
                None => Expr::Nil,
            }
        });
    }
    {
        let (data, board) = (data.clone(), board.clone());
        defc(&mut it, &board.clone(), "hop", move |a| {
            let Some(p) = plan(&data, a) else {
                return Expr::Nil;
            };
            let s = board.state();
            let from = p
                .from
                .unwrap_or((f64::from(s.view.lat), f64::from(s.view.lon)));
            route(
                &*board,
                from,
                p.to,
                p.zoom.unwrap_or(5.0),
                p.ms.unwrap_or(4000.0),
            );
            Expr::String(p.label)
        });
    }
    {
        let data = data.clone();
        def(&mut it, "distance", move |a| {
            // (distance la1 lo1 la2 lo2), or two places: (distance "Olfen" "Munich"), (distance (city "x") (city "y"))
            let ends = if a.len() >= 4 && a.iter().take(4).all(|e| num(e).is_some()) {
                Some(((n_of(a, 0), n_of(a, 1)), (n_of(a, 2), n_of(a, 3))))
            } else if a.len() == 2 {
                dataset(&data)
                    .and_then(|db| Some((position_of(db, &a[0])?, position_of(db, &a[1])?)))
            } else {
                None
            };
            match ends {
                Some(((la1, lo1), (la2, lo2))) => real(haversine_distance(la1, lo1, la2, lo2)),
                None => {
                    say("distance wants 4 numbers or two places: (distance \"Olfen\" \"Munich\")");
                    Expr::Nil
                }
            }
        });
    }
    {
        let data = data.clone();
        def(&mut it, "states", move |a| {
            let (Some(q), Some(db)) = (text_of(a, 0), dataset(&data)) else {
                say("states wants a country: (states \"DE\") or (states \"Germany\")");
                return Expr::Nil;
            };
            match country_of(db, q) {
                Some(c) => Expr::List(
                    db.states_for_country(c)
                        .iter()
                        .map(|s| Expr::String(s.name().to_string()))
                        .collect(),
                ),
                None => {
                    say(&format!("no country matches {q:?}"));
                    Expr::Nil
                }
            }
        });
    }
    {
        let data = data.clone();
        def(&mut it, "cities", move |a| {
            let (Some(q), Some(db)) = (text_of(a, 0), dataset(&data)) else {
                say("cities wants a country: (cities \"DE\" [\"Bavaria\" [n]])");
                return Expr::Nil;
            };
            let Some(country) = country_of(db, q) else {
                say(&format!("no country matches {q:?}"));
                return Expr::Nil;
            };
            // (cities "DE" "Bavaria" 5), (cities "DE" 5) or (cities "DE")
            let (state, n) = match (a.get(1), a.get(2)) {
                (Some(Expr::String(st)), n) => (Some(st.to_lowercase()), n.and_then(num)),
                (n, _) => (None, n.and_then(num)),
            };
            let n = n.map_or(20, |v| v as usize);
            let mut found: Vec<(u64, Expr)> = Vec::new();
            for st in db.states_for_country(country) {
                if state
                    .as_ref()
                    .is_some_and(|s| !st.name().to_lowercase().contains(s.as_str()))
                {
                    continue;
                }
                for city in db.cities_for_state(st) {
                    let pop = city.population().unwrap_or(0);
                    found.push((
                        pop,
                        Expr::List(vec![
                            Expr::String(city.name().to_string()),
                            Expr::String(st.name().to_string()),
                            Expr::Real(pop as f64),
                            real(city.lat().unwrap_or(0.0)),
                            real(city.lng().unwrap_or(0.0)),
                        ]),
                    ));
                }
            }
            found.sort_by_key(|f| std::cmp::Reverse(f.0));
            Expr::List(found.into_iter().take(n).map(|f| f.1).collect())
        });
    }
    {
        let data = data.clone();
        def(&mut it, "count-cities", move |a| {
            let (Some(q), Some(db)) = (text_of(a, 0), dataset(&data)) else {
                say("count-cities wants a country: (count-cities \"DE\" [\"North Rhine\"])");
                return Expr::Nil;
            };
            let Some(country) = country_of(db, q) else {
                say(&format!("no country matches {q:?}"));
                return Expr::Nil;
            };
            let state = text_of(a, 1).map(str::to_lowercase);
            let total: usize = db
                .states_for_country(country)
                .iter()
                .filter(|st| {
                    state
                        .as_ref()
                        .is_none_or(|s| st.name().to_lowercase().contains(s.as_str()))
                })
                .map(|st| db.cities_for_state(st).len())
                .sum();
            Expr::Integer(total as i32)
        });
    }
    {
        let board = board.clone();
        def(&mut it, "note", move |a| {
            // a caption over the code line: (note "Layer 2: relief"); (note) or (note "") clears it
            let text = match a.first() {
                Some(Expr::String(t)) => t.clone(),
                Some(other) => other.to_string(),
                None => String::new(),
            };
            flag(board.send(&format!("!note {text}")))
        });
    }
    def(&mut it, "pwd", |_| {
        let cwd =
            std::env::current_dir().map_or_else(|e| e.to_string(), |d| d.display().to_string());
        say(&format!("working directory: {cwd}"));
        say("(load \"X.LSP\") looks in:");
        for d in script_dirs() {
            say(&format!("  {}", d.display()));
        }
        Expr::String(cwd)
    });
    def(&mut it, "ls", |_| {
        let found = scripts();
        if found.is_empty() {
            say("no *.LSP scripts in the script folders: (pwd) lists them");
        }
        for (path, note) in &found {
            let name = path
                .file_name()
                .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
            say(&format!("  (load \"{name}\")   {note}"));
        }
        Expr::Integer(found.len() as i32)
    });
    def(&mut it, "help-window", |_| flag(open_help_window()));
    def(&mut it, "help", |a| {
        let topic = match a.first() {
            Some(Expr::String(t)) => Some(t.clone()),
            Some(other) => Some(other.to_string()),
            None => None,
        };
        help(topic.as_deref());
        Expr::T
    });
    it
}

fn worker(
    snap: Arc<Mutex<Snapshot>>,
    requests: Sender<Request>,
    abort: Arc<AtomicBool>,
    code: Receiver<String>,
    pending: Arc<AtomicUsize>,
    flush: Arc<AtomicBool>,
) {
    let board: Arc<dyn Board> = Arc::new(UdpBoard::new(snap, requests, abort.clone()));
    let mut it = interpreter(board.clone());
    while let Ok(source) = code.recv() {
        abort.store(false, Ordering::Relaxed);
        INTERRUPT.store(false, Ordering::Relaxed);
        set_now(source.trim());
        let run = catch_unwind(AssertUnwindSafe(|| run_forms(&mut it, &*board, &source)));
        for line in it.output.drain(..) {
            say(&line);
        }
        match run {
            Ok(results) => {
                if let Some(last) = results.last() {
                    say(&format!("=> {last}"));
                }
            }
            Err(e) => {
                say(if e.is::<Interrupted>() {
                    "interrupted"
                } else {
                    "error in the interpreter (state reset)"
                });
                it = interpreter(board.clone());
                board.send("!code"); // the screen shows no line of a script that is not running
            }
        }
        if source.trim_start().to_lowercase().starts_with("(load") {
            board.send("!note"); // a script ended: its caption goes with it
        }
        set_now("");
        pending.fetch_sub(1, Ordering::Relaxed);
        // Ctrl-C also drops what was typed meanwhile
        if flush.swap(false, Ordering::Relaxed) {
            while code.try_recv().is_ok() {
                pending.fetch_sub(1, Ordering::Relaxed);
            }
        }
    }
}

pub struct Repl {
    to_worker: Sender<String>,
    requests: Receiver<Request>,
    abort: Arc<AtomicBool>,
    pending: Arc<AtomicUsize>,
    flush: Arc<AtomicBool>,
}

impl Repl {
    pub fn new(snap: Arc<Mutex<Snapshot>>) -> Repl {
        let (to_worker, code) = channel();
        let (req, requests) = channel();
        let abort = Arc::new(AtomicBool::new(false));
        let pending = Arc::new(AtomicUsize::new(0));
        let flush = Arc::new(AtomicBool::new(false));
        {
            let (abort, pending, flush) = (abort.clone(), pending.clone(), flush.clone());
            std::thread::spawn(move || worker(snap, req, abort, code, pending, flush));
        }
        Repl {
            to_worker,
            requests,
            abort,
            pending,
            flush,
        }
    }

    /// Evaluates `source` (one line typed, or a whole script) on the interpreter's thread; its output comes
    /// through [`say`].
    pub fn run(&self, source: String) {
        self.pending.fetch_add(1, Ordering::Relaxed);
        let _ = self.to_worker.send(source);
    }

    /// How many commands are running or waiting (the first is the running one).
    pub fn pending(&self) -> usize {
        self.pending.load(Ordering::Relaxed)
    }

    /// Stops the script that is running (it stops at its next `(sleep ...)` or move) and drops the commands
    /// typed meanwhile.
    pub fn interrupt(&self) {
        self.flush.store(true, Ordering::Relaxed);
        self.abort.store(true, Ordering::Relaxed);
        INTERRUPT.store(true, Ordering::Relaxed);
    }

    /// A request for the window (a screenshot), if the script made one.
    pub fn poll(&self) -> Option<Request> {
        self.requests.try_recv().ok()
    }
}

type Sink = Box<dyn Fn(&str) + Send + Sync>;
static SINK: Mutex<Option<Sink>> = Mutex::new(None);

/// Where the console's output goes (the window's console panel); without one it goes to the terminal.
pub fn set_sink(sink: impl Fn(&str) + Send + Sync + 'static) {
    *SINK.lock().unwrap() = Some(Box::new(sink));
}

/// One line of console output.
pub fn say(text: &str) {
    match SINK.lock().unwrap().as_ref() {
        Some(sink) => text.lines().for_each(sink),
        None => println!("{text}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_board_functions_answer_and_a_script_can_loop() {
        let _turn = TEST_TURN.lock().unwrap_or_else(|e| e.into_inner());
        let snap = Arc::new(Mutex::new(Snapshot {
            view: View::new(48.0, 11.0),
            spin: ui::Spin::new(),
            fps: 30,
            options: 5,
            selected: None,
            board: None,
        }));
        let (req, _requests) = channel();
        let mut it = interpreter(Arc::new(UdpBoard::new(
            snap,
            req,
            Arc::new(AtomicBool::new(false)),
        )));
        let r = it.run(
            "(setq n 0) (repeat 3 (setq n (+ n 1)) (sleep 1)) (list n (lat) (zoom) (layer-bits))",
        );
        assert_eq!(format!("{}", r.last().unwrap()), "(3 48 1 5)");
        // no packet heard yet: the commands say nil instead of sending
        assert_eq!(format!("{}", it.run("(view 1 2 3)")[0]), "nil");
        let r = it.run("(defun go (lat lon) (view lat lon 6)) (go 35.7 139.7)");
        assert_eq!(format!("{}", r.last().unwrap()), "nil"); // no board
                                                             // the geodb functions (the dataset is in the repository)
        let r = it.run("(city \"Tokyo\") (country \"DE\") (distance 0 0 0 1) (car (car (nearest 48.1 11.6 1)))");
        let shown: Vec<String> = r.iter().map(|e| e.to_string()).collect();
        println!("{shown:?}");
        assert!(
            shown[0].contains("Tokyo") && shown[1].contains("Germany"),
            "{shown:?}"
        );
        assert!(shown[3].starts_with('"'), "{shown:?}");
        // places as arguments: names, results of find and city
        let r = it.run("(distance \"Ludinghausen\" \"Olfen\") (distance (find \"Olfen\") (city \"Munich\")) (car (car (nearest \"Olfen\" 1)))");
        let d: Vec<String> = r.iter().map(|e| e.to_string()).collect();
        println!("{d:?}");
        assert!(
            d[0] != "nil" && d[1] != "nil" && d[2].starts_with('"'),
            "{d:?}"
        );
    }
}

#[cfg(test)]
mod help_tests {
    use super::*;

    #[test]
    fn help_takes_a_bare_name() {
        let call =
            bare_help(&acadlisp::parser::Parser::from_input("(help find)").parse_all()[0]).unwrap();
        assert_eq!(call.to_string(), "(HELP \"FIND\")");
        assert!(
            bare_help(&acadlisp::parser::Parser::from_input("(help)").parse_all()[0]).is_none()
        );
        // every function a doc names exists once, and the long text ends sentences properly
        let mut names: Vec<_> = DOCS.iter().map(|d| d.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), DOCS.len());
    }
}

#[cfg(test)]
mod plan_tests {
    use super::*;

    fn args(text: &str) -> Vec<Expr> {
        let form = acadlisp::parser::Parser::from_input(text)
            .parse_all()
            .remove(0);
        match form {
            Expr::List(mut items) => items.drain(1..).collect(),
            _ => Vec::new(),
        }
    }

    #[test]
    fn the_types_of_the_arguments_route_them() {
        let _turn = TEST_TURN.lock().unwrap_or_else(|e| e.into_inner());
        let data: Dataset = new_dataset();
        // a point and the numbers after it
        let p = plan(&data, &args("(fly 48.1 11.6 5 3000)")).unwrap();
        assert_eq!(
            (p.from, p.to, p.zoom, p.ms),
            (None, (48.1, 11.6), Some(5.0), Some(3000.0))
        );
        // two places: from and to; the numbers are zoom and time
        let p = plan(&data, &args("(fly \"Ludinghausen\" \"Wuhan\" 4 2000)")).unwrap();
        assert!(p.from.is_some() && (p.to.0 - 30.6).abs() < 1.0);
        assert_eq!((p.zoom, p.ms), (Some(4.0), Some(2000.0)));
        // a place and then zoom and time (not a second point)
        let p = plan(&data, &args("(fly \"Wuhan\" 8 3000)")).unwrap();
        assert!(p.from.is_none() && p.zoom == Some(8.0) && p.ms == Some(3000.0));
        assert!(plan(&data, &args("(fly)")).is_none());
    }
}

#[cfg(test)]
mod interrupt_tests {
    use super::*;

    #[test]
    fn ctrl_c_stops_a_running_script_and_drops_what_waits() {
        let _turn = TEST_TURN.lock().unwrap_or_else(|e| e.into_inner());
        let snap = Arc::new(Mutex::new(Snapshot {
            view: geodb_fw_core::render::View::new(0.0, 0.0),
            spin: geodb_fw_core::ui::Spin::new(),
            fps: 0,
            options: 0,
            selected: None,
            board: None,
        }));
        let repl = Repl::new(snap);
        repl.run("(sleep 600000)".into());
        repl.run("(princ 1)".into());
        std::thread::sleep(Duration::from_millis(300));
        assert_eq!(repl.pending(), 2);
        repl.interrupt();
        let end = Instant::now() + Duration::from_secs(3);
        while repl.pending() > 0 && Instant::now() < end {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(repl.pending(), 0);
        INTERRUPT.store(false, Ordering::Relaxed);
    }
}
