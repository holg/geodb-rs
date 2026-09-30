# From Nested Trees to Flat Blobs
### How I Built a Cross‑Platform Geo Database in Rust (CLI, WASM, and Python)

I started this project with a simple goal:

> **Have one high‑quality world dataset (countries, states, cities, aliases, regions)**
> and make it available everywhere:
>
> - as a **Rust library**
> - via a **CLI tool**
> - compiled to **WASM** for the browser
> - and as a **Python package** on PyPI

On paper, this sounded straightforward. In practice, it turned into a tour through:

- data modelling (nested vs flat),
- cross‑platform builds (Linux, macOS, Windows, manylinux),
- OpenSSL vs rustls,
- Rust → Python bindings with `pyo3` and `maturin`,
- and performance profiling with `criterion` and **Xcode Instruments**.

This post is a walk‑through of that journey, using the [`geodb-rs`](https://github.com/holg/geodb-rs) repo as a concrete example.

---

## 1. The Core Idea: One GeoDB, Many Frontends

At the heart of the repo is **`geodb-core`**, a Rust crate that:

- reads a big JSON dataset (`countries+states+cities.json.gz`),
- normalizes it into a strongly typed `GeoDb` structure,
- supports basic queries: countries, states, cities, phone codes, regions, aliases,
- and caches a binary representation (`*.bin`) to avoid reparsing JSON every time.

On top of this, there are three “frontends”:

- **`geodb-cli`** – a command‑line interface for quick querying and scripting,
- **`geodb-wasm`** – a WASM bundle for use in the browser with a simple JS API,
- **`geodb-py` / `geodb_rs`** – Python bindings, published on PyPI with prebuilt wheels.

The design principle is:

> **All serious logic lives in `geodb-core`.**
> Everything else is just a thin integration layer.

This paid off repeatedly: whenever I fixed bugs or added features (like regions or aliases), they became instantly available in the CLI, the WASM demo, and Python bindings.

---

## 2. The Data Model: Nested vs Flat

The raw JSON dataset looks like this (simplified):

```json
[
  {
    "name": "Germany",
    "iso2": "DE",
    "iso3": "DEU",
    "phonecode": "49",
    "states": [
      {
        "name": "Nordrhein-Westfalen",
        "iso2": "NW",
        "cities": [
          { "name": "Münster", "latitude": "51.9624", "longitude": "7.6257" },
          { "name": "Dortmund", "latitude": "51.5136", "longitude": "7.4653" }
        ]
      }
    ]
  }
]
```

The **obvious** Rust representation mirrors this 1:1:

```rust
pub struct GeoDb<B: GeoBackend> {
    pub countries: Vec<Country<B>>,
}

pub struct Country<B: GeoBackend> {
    pub name: B::Str,
    pub iso2: B::Str,
    pub iso3: Option<B::Str>,
    pub states: Vec<State<B>>,
    // ...
}

pub struct State<B: GeoBackend> {
    pub name: B::Str,
    pub cities: Vec<City<B>>,
    // ...
}

pub struct City<B: GeoBackend> {
    pub name: B::Str,
    pub latitude: Option<B::Float>,
    pub longitude: Option<B::Float>,
    pub timezone: Option<B::Str>,
}
```

This is what I now call the **legacy nested model**.

### Why even consider a flat model?

Nested structures are great for readability (and for anyone debugging the JSON), but inside tight loops they can be:

- cache‑unfriendly,
- expensive to traverse,
- awkward to store in compact binary format if you want to squeeze memory.

I wanted to experiment with a **“flat” model** where:

- cities are stored in big contiguous arrays,
- with index ranges or small indirection layers (like `Country` → range of `State` indices, `State` → range of `City` indices).

The expectation was:

> “Flatten data, get **much faster** lookups and better cache utilisation.”

So I added a feature flag `legacy_model` in `geodb-core` and built a second internal representation gated by features. Same API on top, different storage underneath.

---

## 3. Measuring Reality: Criterion Benchmarks

Opinion is nice, numbers are better.

I set up **criterion** benchmarks in `crates/geodb-core/benches/benchmarks.rs` and compiled two variants:

- legacy nested model:

```bash
cargo bench -p geodb-core \
  --no-default-features \
  --features "compact,json,legacy_model" \
  --bench benchmarks
```

- new flat model:

```bash
cargo bench -p geodb-core \
  --no-default-features \
  --features "compact,json" \
  --bench benchmarks
```

Then I used **criterion baselines** to compare them:

```bash
# 1) Save baseline for nested model
cargo bench -p geodb-core \
  --no-default-features \
  --features "compact,json,legacy_model" \
  --bench benchmarks \
  -- \
  --save-baseline nested

# 2) Save baseline for flat model
cargo bench -p geodb-core \
  --no-default-features \
  --features "compact,json" \
  --bench benchmarks \
  -- \
  --save-baseline flat

# 3) Compare flat model against nested baseline
cargo bench -p geodb-core \
  --no-default-features \
  --features "compact,json" \
  --bench benchmarks \
  -- \
  --baseline nested
```

### What we measured

The benchmarks focus on:

- smart search for “Berlin” (realistic usage),
- a “worst‑case” search (stress test),
- micro‑benchmarks for the text folding and substring checks used in search.

The results (summarised):

- **Smart search (Berlin)**:
  Flat model is ~2–4% faster – an improvement, but not a game‑changer.
- **Worst‑case search**:
  Flat model improved by ~1–3% in many runs, occasionally within noise.
- **Micro‑benchmarks** (folding / matching):
  Mostly identical or micro improvements / regressions within noise.

The uncomfortable but important conclusion:

> The flat model is **only slightly faster** than the simple nested structure.

Given the complexity it introduced (indices, extra mapping logic, more code paths), this forced me to think hard about trade‑offs.

---

## 4. Profiling with Xcode Instruments

Benchmarks told me *how much* faster, but I also wanted to know *why*.

On macOS, Instruments (with the **Time Profiler** template) gave good insight into where the time actually goes during a search.

Rough steps:

1. Build the benchmark binary:

   ```bash
   cargo bench -p geodb-core --no-default-features --features "compact,json" --bench benchmarks
   ```

2. Locate the compiled bench in `target/aarch64-apple-darwin/release/deps/`.

3. Create a **Time Profiler** run in Instruments, attach it to that binary.

4. Run a benchmark (or a dedicated binary that runs one search many times) and capture a profile.

### What the profiler showed

A representative profile looked like this (flattened):

- ~96% of time in `benchmarks::main`
- of that, a big chunk in `GeoDb::smart_search`
- and **inside that**, a lot of time in:

    - `geodb_core::text::match_score`
    - `geodb_core::text::fold_key`
    - `deunicode::deunicode_with_tofu_cow`
    - `str::to_lowercase`

In other words:

> The bottleneck is **text normalisation and scoring**, not data structure layout.

Flattening the data helped a bit (better locality, simpler traversal), but it couldn’t magically fix the fundamental cost of:

- deunicoding (e.g. “Münster” → “Munster”),
- lowercasing,
- substring and similarity scoring.

This is actually good news:

- It means the **data layout is “good enough” already.**
- Real gains will come from improving the text pipeline (caching folded keys, using cheaper scoring, precomputing tokens, etc.), which we can do orthogonally to the underlying structure.

---

## 5. The Loader & Cache Design

The loader in `geodb-core/src/loader.rs` is responsible for:

- finding the dataset,
- loading JSON (via gzip),
- applying optional ISO2 filters,
- building the `GeoDb` structure,
- and caching a `.bin` file next to the JSON for fast reload.

Key aspects:

```rust
impl GeoDb<DefaultBackend> {
    /// Default directory: `<crate>/data`
    pub fn default_data_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data")
    }

    /// Default dataset file name
    pub fn default_dataset_filename() -> &'static str {
        "countries+states+cities.json.gz"
    }

    /// Load the default DB (unfiltered)
    pub fn load() -> Result<Self> {
        GEO_DB_CACHE.get_or_try_init(|| {
            let dir = Self::default_data_dir();
            let file = Self::default_dataset_filename();
            Self::load_from_path(dir.join(file), None)
        }).cloned()
    }

    /// Load from a custom on-disk dataset path
    pub fn load_from_path(
        json_path: impl AsRef<Path>,
        iso2_filter: Option<&[&str]>
    ) -> Result<Self> {
        let json_path = json_path.as_ref().to_path_buf();
        load_generic(json_path, iso2_filter)
    }

    /// Load filtered DB using default dataset
    pub fn load_filtered_by_iso2(iso2: &[&str]) -> Result<Self> {
        let dir = Self::default_data_dir();
        let file = Self::default_dataset_filename();
        Self::load_from_path(dir.join(file), Some(iso2))
    }
}
```

The cache file naming is simple and robust:

- base JSON: `countries+states+cities.json.gz`
- cache for all countries: `countries+states+cities.json.gz.ALL.bin`
- cache for ISO2 filter `["DE", "FR"]`: `countries+states+cities.json.gz.DE_FR.bin`

So the cache is always **derived from the data file name**, no extra config needed.
You can also point `load_from_path` at your own dataset copy and it will generate separate caches for that file.

---

## 6. CLI: `geodb-cli` as a Living Example

One goal of the CLI was **not** to reinvent logic in another crate, but to:

- show how to use `geodb-core` idiomatically,
- expose a few useful queries on the command line,
- and serve as extra coverage for error handling, filtering, etc.

The argument parsing is done via `clap` in `crates/geodb-cli/src/args.rs`, kept separate from `main.rs` for clarity.

The CLI exposes commands like:

- `geodb-cli stats` – show country/state/city counts,
- `geodb-cli country DE` – lookup by ISO2/ISO3,
- `geodb-cli phone +49` – lookup by phone code prefix.

Internally, you’ll see things like:

```rust
match db.find_country_by_code(&code) {
    Some(c) => {
        println!("Country: {}", c.name());
        println!("ISO2: {}", c.iso2());
        println!("ISO3: {}", c.iso3());
        println!("Capital: {:?}", c.capital());
        println!("Phone Code: {}", c.phone_code());
        println!("Currency: {}", c.currency());
        println!("Region: {}", c.region());
        println!("Population: {:?}", c.population());
        println!("States: {}", c.states().len());
    }
    None => {
        eprintln!("No country found for: {}", code);
    }
}
```

Here, `find_country_by_code` is a small helper in `GeoDb` that checks both ISO2 and ISO3:

```rust
impl<B: GeoBackend> GeoDb<B> {
    pub fn find_country_by_iso2(&self, iso2: &str) -> Option<&Country<B>> {
        self.countries
            .iter()
            .find(|c| c.iso2.as_ref().eq_ignore_ascii_case(iso2))
    }

    pub fn find_country_by_iso3(&self, iso3: &str) -> Option<&Country<B>> {
        self.countries
            .iter()
            .find(|c| c.iso3.as_ref().map_or(false, |s| s.as_ref().eq_ignore_ascii_case(iso3)))
    }

    pub fn find_country_by_code(&self, code: &str) -> Option<&Country<B>> {
        self.find_country_by_iso2(code).or_else(|| self.find_country_by_iso3(code))
    }
}
```

The nice part: this logic lives in `geodb-core`, so Python and WASM could use it too.

---

## 7. WASM: `geodb-wasm` and Embedded DB

The WASM crate (`crates/geodb-wasm`) does roughly two things:

1. Embeds a prebuilt binary DB (generated from the JSON) at compile time.
2. Exposes a simple JS API to query it from the browser.

The init path uses `OnceCell` and `bincode`:

```rust
static DB: OnceCell<GeoDb<StandardBackend>> = OnceCell::new();

#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
    web_sys::console::log_1(&"Initializing GeoDB WASM module...".into());

    DB.get_or_init(|| {
        web_sys::console::log_1(&"Deserializing embedded DB...".into());
        match bincode::deserialize::<GeoDb<StandardBackend>>(EMBEDDED_DB) {
            Ok(db) => {
                web_sys::console::log_1(
                    &format!("✓ Loaded {} countries", db.countries().len()).into(),
                );
                db
            }
            Err(e) => {
                web_sys::console::error_1(&format!("✗ DB load failed: {}", e).into());
                panic!("Failed to load DB: {}", e);
            }
        }
    });
}
```

The DB is embedded as a `&'static [u8]` using `include_bytes!` in a small build step. That way, the WASM bundle is self‑contained – no separate fetch needed.

### Why this design?

- You get **instant, offline data** in the demo.
- The same `GeoDb` structure is used as in the CLI and library.
- The benchmark work we did applies here too: if `smart_search` gets faster, the browser demo becomes faster automatically.

---

## 8. Python: `geodb_rs` Wheels on PyPI

Publishing the Python binding was another rabbit hole:

- use `pyo3` for the Rust ↔ Python bridge,
- use `maturin` to build wheels and sdist,
- make sure **data files are bundled** inside the Python package,
- and make CI build manylinux + macOS + Windows wheels.

The Python crate layout:

- `crates/geodb-py/Cargo.toml` – Rust side, crate name `geodb-py`, library `geodb_rs`
- `crates/geodb-py/pyproject.toml` – Python packaging config, project name `geodb-rs`

Example `pyproject.toml`:

```toml
[build-system]
requires = ["maturin>=1.7,<2.0"]
build-backend = "maturin"

[project]
name = "geodb-rs"
dynamic = ["version"]
description = "Python bindings for geodb-core"
readme = "README.md"
requires-python = ">=3.8"
license = { text = "MIT" }
authors = [{ name = "Holger Trahe" }]

[tool.maturin]
bindings = "pyo3"
module-name = "geodb_rs"
include = [
  { path = "geodb_rs_data/countries+states+cities.json.gz", format = "sdist" },
  { path = "geodb_rs_data/countries+states+cities.json.gz", format = "wheel" },
]
```

The important points:

- **Rust crate name** (`geodb-py`) and **Python package name** (`geodb-rs`) can differ – maturin handles this via metadata.
- Data is shipped via the `include` section, so the Python package has direct access to the same JSON (or prebuilt bin) as Rust.

### CI for wheels

Using `messense/maturin-action`, we build wheels for:

- linux `x86_64` and `aarch64` (manylinux),
- macOS `x86_64` and `aarch64`,
- Windows `x64`.

The CI job looks roughly like this:

```yaml
build-wheels:
  needs: lint-test
  if: github.event_name != 'pull_request'
  strategy:
    fail-fast: false
    matrix:
      include:
        - os: ubuntu-latest
          target: x86_64
          manylinux: auto
        - os: ubuntu-latest
          target: aarch64
          manylinux: auto
        - os: macos-13
          target: x86_64
          manylinux: ""
        - os: macos-14
          target: aarch64
          manylinux: ""
        - os: windows-latest
          target: x64
          manylinux: ""

  runs-on: ${{ matrix.os }}

  steps:
    - uses: actions/checkout@v4
    - uses: actions/setup-python@v5
      with:
        python-version: "3.11"
    - uses: dtolnay/rust-toolchain@stable

    - name: Build wheels with maturin
      uses: messense/maturin-action@v1
      env:
        CARGO_TARGET_DIR: /tmp/maturin-target
      with:
        manylinux: ${{ matrix.manylinux }}
        target: ${{ matrix.target }}
        args: --release -m crates/geodb-py/Cargo.toml -o dist
```

The wheels are then uploaded as workflow artifacts and, in a separate `pypi.yml`, published to PyPI on tagged releases (using `MATURIN_PYPI_TOKEN`).

---

## 9. CI, OpenSSL, and the Switch to rustls

One fun CI failure came from **OpenSSL**:

- On manylinux containers and some CI environments, system OpenSSL is missing or incompatible.
- `reqwest` defaults to OpenSSL (`native-tls`) unless you ask for `rustls`.

The fix in `geodb-core`:

```toml
[dependencies]
# ...
reqwest = { version = "0.12", features = ["blocking", "rustls-tls"], optional = true }
```

After adding this, I verified with:

```bash
cargo tree -i openssl-sys --target all
```

and made sure **no** dependency pulls `openssl-sys` anymore.

This is one of those things that matter a lot when you want:

- manylinux wheels,
- reproducible CI builds,
- and minimal non‑Rust system dependencies.

---

## 10. Release Automation: Tags, Crates.io, PyPI, GitHub Releases

Last piece of the puzzle: release hygiene.

I use `cargo-release` with a small `release.toml`:

```toml
# release.toml – config for the `cargo release` tool.

shared-version = true
dependent-version = "upgrade"

tag-name = "v{{version}}"
tag-message = "Release v{{version}}"

publish = false
push = false

allow-branch = ["main"]
```

The flow is:

1. **Bump versions + tag**:

   ```bash
   cargo release patch --execute
   ```

   This updates versions, creates a commit, and tags `vX.Y.Z`.

2. **CI reacts to the tag**:

    - runs tests and checks,
    - builds CLI binaries and Python wheels,
    - uploads artifacts,
    - publishes to crates.io (via a dedicated workflow, if desired),
    - publishes to PyPI (via `pypi.yml`),
    - and finally attaches binaries + wheels to the GitHub Release.

End result:

- A tagged release like `v0.1.3` has:
    - Rust crates updated and published,
    - Python wheels for major platforms on PyPI,
    - CLI binaries downloadable directly from the GitHub Release,
    - and a WASM demo build artifact if you wire it in.

---

## 11. Lessons Learned

A few takeaways from this little adventure:

1. **Start simple, measure first.**
   The flat vs nested saga was a good reminder: complexity should follow proven need.
   The flat model is marginally faster, but the real bottleneck was text processing.

2. **Invest in a clean core crate.**
   Having everything go through `geodb-core` made it easy to:
    - add features once (aliases, regions, phone search),
    - reuse in CLI, WASM, and Python,
    - and drive all tests and benchmarks from one place.

3. **CI for multi‑platform is worth it early.**
   It forces you to:
    - get rid of OpenSSL pain,
    - think about manylinux constraints,
    - and set up proper caching, feature flags, and build scripts.

4. **Profilers and benchmarks are complementary.**
    - Criterion: “flat is ~2–4% faster than nested.”
    - Instruments: “you’re mostly paying for text folding and scoring.”
      Both are necessary to understand where to focus next.

5. **Rust + Python + WASM is a powerful trio.**
    - Rust gives you robust core logic and performance,
    - Python bindings unlock the data science ecosystem,
    - WASM gives you instant demos and interactive docs.

---

## 12. What’s Next?

From here, the most interesting improvements are likely in:

- **Text search pipeline**
    - caching folded keys per city,
    - precomputing tokens,
    - exploring simpler scoring algorithms.

- **Smaller / more compact backends**
    - using `smol_str` or similar compact string storage,
    - considering more advanced binary layouts.

- **Richer APIs**
    - better alias and region search,
    - reverse lookups (geo → city/country),
    - timezone utilities on top of `CountryTimezone`.

If any of that sounds fun, contributions and issue discussions are very welcome:

👉 Repo: `https://github.com/holg/geodb-rs`

This is very much a “real” project: it scrapes enough of the Rust, CI, packaging, and profiling surface area that you can learn a lot by poking around – exactly how I like it.

---

## Addendum: A 1.3 MB Globe — Geoid-Only Data, WebGPU and the Browser

The WASM demo from section 7 embeds the whole flat database: the `.wasm` is **11.3 MB**. That is fine for a desktop, but I wanted a showcase for the other end of the scale:

> **The real globe, all 153,312 cities, search and spatial queries — in about 1 MB over the network, with low memory, running in any modern browser (even from `file://`).**

This addendum is about how far the data can be squeezed, what the Z-order code buys you, and what happened when I put the same queries on the GPU.

### A.1 Geoid-only: no coordinates at all

Every city in `geodb-core` already carries a **geoid**: a 64-bit Morton (Z-order) code that interleaves the quantized latitude (odd bits) and longitude (even bits), 32 bits per axis. One latitude step is 180° / 2³² ≈ 4.7 mm.

The new `CompactGlobeDb` (`geodb-core/src/globe_db.rs`) keeps *only* that geoid as the position — no `f64` latitude/longitude — plus names, compact ids and a rank (town / regional / capital). The file format (`.globe`) is columnar:

```text
header   "GDBG"  version  geoid_bits  compressed  reserved
counts   countries, states, cities                          (varints)
cities   sorted by geoid, as columns:
         geoid     varint delta of (geoid >> (64 - geoid_bits))
         country   u8 (u16 if > 256 countries)
         state     zigzag delta against the previous city
         rank      2 bits per city
         names     NUL-separated, in city order
```

Cities that are neighbours on the Z-order curve are neighbours on the map, so the geoid deltas are tiny and the country/state columns are long runs. Columns compress much better than records.

The geoid can be truncated to trade precision for size. Measured on the real dataset (153,312 cities):

| geoid bits | file (gzip inside) | bytes / city | worst position error | vs flat `.bin` (7.3 MB) |
|---:|---:|---:|---:|---:|
| 64 | 1.75 MB | 11.4 | 0 | 4.2× smaller |
| 48 | 1.43 MB | 9.3 | 1.3 m | 5.1× smaller |
| 40 | 1.27 MB | 8.3 | 21 m | 5.8× smaller |
| **32** | **1.10 MB** | **7.1** | **341 m** | **6.7× smaller** |

341 m is invisible on a globe, so the demo uses 32 bits. Every size is smaller than the 3.6 MB source `json.gz`.

`geodb-cli build-globe --bits 32 [--raw] -o cities.globe` writes the format, `geodb-cli globe-nearest --lat … --lng …` answers from the file alone.

### A.2 The sort *is* the spatial index

Because the cities are sorted by geoid, there is no separate index to ship. A radius query asks `RadiusBounds::geoid_ranges()` for the at most 16 Z-order cells that cover the search circle (exact bounding box, antimeridian and poles included), and each cell is a contiguous slice of the sorted array — two binary searches. Then an `f64` haversine on the few candidates.

That took the compact radius search from a 905 µs linear scan to **37 µs** (Munich) and 15 µs (Fiji, across the dateline). `find_nearest` is exact: it widens the radius (25 km, doubling) until it holds `k` cities. Both are tested against a brute-force scan at hard places (antimeridian, the 0°/0° cell boundary, the poles) and 300 random centres.

### A.3 Load what you have: geodb-mini and geodb-float

The globe app no longer embeds a database. It fetches one, and **the file decides what the app can do**. A small trait, `GlobeSource`, hides which one it got; the loader sniffs the first bytes:

- `GDBG` → **geodb-mini**: the compact file. Positions from 32-bit geoids (±341 m), name search by scanning, countries fly to their capital, regions to the centre of their cities.
- gzip → **geodb-float**: the full flat database — exact `f64` coordinates, smart search (folding, aliases, ISO codes), timezones, currencies, native names.

Hovering a city shows everything the loaded data knows. For Munich, geodb-mini has 7 fields (and its geoid is `0xe0602f6d`, 32 bits); geodb-float has 15 (`0xe0602f6d30a2dcde` — the mini geoid is literally its first 32 bits).

Cargo features decide which loaders are compiled in, so the smallest page does not pay for the big one:

| build | contains | wasm | total download |
|---|---|---:|---:|
| `mini.html` | geodb-mini, WebGPU | 555 KB (192 KB br) | **1.30 MB** with brotli |
| `flex.html` | + geodb-float (`?data=float`), WebGL2 fallback | 3.7 MB | 11 MB |
| `one-in-all.html` | mini, everything inline (base64) | — | 2.46 MB file, 1.49 MB br |
| `one-in-all-flex.html` | flex, everything inline | — | 16.5 MB |

The single-file variants open straight from disk: `file://` cannot `fetch()` or import modules, so the wasm-bindgen glue is inlined and the wasm and data are base64 blocks the app reads before it would fetch.

One compression lesson: I first assumed the one-file HTML with brotli would be best. Measured, it is not — the data files are already gzipped inside, and base64 hides byte patterns. Storing the `.globe` columns **raw** and letting the server send brotli is 10% smaller than the gzip inside the file (979 KB vs 1,090 KB), so the release (`scripts/web_release.py`) ships raw data plus precompressed `.br`/`.gz`:

| file | plain | gzip | brotli |
|---|---:|---:|---:|
| cities.globe (raw) | 2,490,193 | 1,089,698 | 979,216 |
| coast.bin (raw) | 162,458 | 117,077 | 109,898 |
| wasm | 554,903 | 232,136 | 191,604 |
| js + html | 100,167 | 18,927 | 16,037 |
| **total** | 3.31 MB | 1.46 MB | **1.30 MB** |

Chrome's own Resource Timing, shown live in the page's *Cost* panel, confirms these sizes on the wire. The coastlines are Natural Earth 1:50m, packed as zigzag-varint deltas in 1/100° (118 KB instead of 450 KB of GeoJSON); the earth texture is baked in the browser from them plus the city density.

### A.4 WebGPU: frames and compute

Rendering is the same `wgpu` renderer as the native TUI/window app, compiled to WebGPU. On an M2 Max with a 60 Hz Studio Display the page is **vsync-capped at 60 fps** — which says nothing about the GPU. So the benchmark also renders 120 frames offscreen back to back and waits for the queue: **3,750 fps** equivalent (0.27 ms per frame at 4096 × 1842 with 4× MSAA). The per-frame budget goes to the view query (a few ms at 1,024 markers), not the GPU.

More interesting: the same geoids go to the GPU as a **compute index**. The kernels deinterleave the Morton codes (WGSL has no 64-bit integers, so each geoid is two `u32`), take exact integer axis deltas, and finish with an `f32` haversine. Radius search runs one query per call or many per dispatch (each with its own radius); k-nearest is two passes (each workgroup keeps the top k of 4,096 geoids, a second pass merges).

The benchmark panel runs **the same random queries** through every engine, side by side. 2,000 queries, radius log-uniform between 1 and 1,000 km, geodb-mini, Chrome, M2 Max:

| radius search | CPU index | GPU, 1 query/call | GPU, 1024/call |
|---|---:|---:|---:|
| median | **5.0 µs** | 260 µs | 11.4 µs |
| mean | 240 µs | 321 µs | 11.4 µs |
| queries/s | 4.2 k | 3.1 k | **87 k** |
| agrees with CPU | reference | 1993/2000 (±1) | 1993/2000 (±1) |

| 10 nearest | CPU index | GPU, 1/call | GPU, 1024/call | CPU scan |
|---|---:|---:|---:|---:|
| median | **10 µs** | 1.2 ms | 51 µs | 12.6 ms |
| agrees with CPU | reference | 1999/2000 | 1999/2000 | — |

What the numbers say:

- **One GPU call per query is dominated by the round trip** (submit, compute, map, read back ≈ 0.25 ms). It cannot beat an index that answers most queries in 5 µs.
- **Batched, the GPU wins on throughput for radius search** — the CPU's cost grows with the radius (1,000 km returns ~60,000 cities, built and sorted), the GPU tests all 153k geoids per query no matter what: ~11 µs, flat.
- **For k-nearest the index wins.** The GPU is brute force; against a brute-force CPU scan it is ~250× faster, against a good index 5× slower.
- The first kernel used an `f32` *flat-earth* distance and disagreed on 10% of the counts (off by up to 71 cities at long radii). Switching to haversine on the exact integer deltas brought that to ±1 — cities within metres of the circle, where `f32` and `f64` round differently.
- Browser timers are coarse (100 µs) unless the page is cross-origin isolated; the local server sends COOP/COEP so the panel gets 5 µs and can time single queries.

### A.5 The bug the GPU found

The comparison paid for itself immediately. With geodb-float loaded, the GPU's 10 nearest cities matched the CPU's on only **1,503 of 2,000** queries. The GPU had been verified against an exact scan, so I checked the CPU the same way:

> `GeoDb::find_nearest` returned the wrong cities for **501 of 2,000** random queries, the 10th neighbour up to **336 km** too far.

It scanned a fixed window of the geoid-sorted index around the query. Z-order curves have jumps: two cities 5 km apart can be far apart in the sort. The window missed them. (The legacy nested model scanned everything but ranked by squared *degrees* — wrong near the poles and across the dateline.)

The fix reuses A.2: `find_nearest` and `find_cities_in_radius_by_geoid` now walk the Z-order cells of the geoid index with an `f64` haversine; nearest widens the radius until it has `k`. It is exact, and the radius search got 3.7× faster (662 → 180 µs mean on the same queries). A regression test compares both models with a brute-force scan; run against the old code it fails at the very first place it checks (Munich).

### A.6 How it compares

Is 1.3 MB for a globe with 153k searchable cities unusual? I checked comparable projects. Sizes marked brotli were measured from the published npm files (jsDelivr/unpkg, brotli -q 11, 29 September 2026); the rest are as their docs state them.

**Globe and map engines — the library alone, before any data:**

| project | download (brotli) | geodata included | offline |
|---|---:|---|---|
| [cobe](https://github.com/shuding/cobe) 2.0.1 | 5 KB | a 256×128 dot map, no cities | yes |
| [OpenGlobus](https://github.com/openglobus/openglobus) 0.28.7 | 174 KB | none, streams tile layers | not documented |
| [MapLibre GL JS](https://maplibre.org/maplibre-gl-js/docs/examples/display-a-globe-with-a-vector-map/) 6.11 (globe since v5) | 253 KB | none, needs a style and tiles | only with local tiles |
| [NASA Web WorldWind](https://github.com/NASAWorldWind/WebWorldWind) 0.11 | 273 KB | none, imagery "retrieved from remote servers" | no |
| [globe.gl](https://github.com/vasturiano/globe.gl) 2.46 (incl. three.js) | 408 KB | none; its places example fetches a 190 KB GeoJSON | if self-hosted |
| [Mapbox GL JS](https://docs.mapbox.com/mapbox-gl-js/guides/globe/) 3.32 | 411 KB | Mapbox-hosted tiles, access token | no |
| [deck.gl](https://deck.gl/docs/api-reference/core/globe-view) 9.4 (GlobeView "experimental") | 451 KB | none | if self-hosted |
| [CesiumJS](https://cesium.com/learn/cesiumjs-learn/cesiumjs-quickstart/) 1.145 (main bundle only) | 1.36 MB | streams imagery/terrain from Cesium ion (token) | no |
| **geodb-globe mini** | **1.30 MB total** | **153,312 cities, regions, countries, coastlines** | **yes, even `file://`** |

The geodb-globe app itself is 192 KB wasm + 13 KB JS (brotli) — about OpenGlobus's size — and the other 1.09 MB is data.

**City data in the browser:**

| dataset / package | cities | brotli | fields |
|---|---:|---:|---|
| **cities.globe (geodb-mini)** | **153,312** | **979 KB** | name, region, country, flag, rank; 32-bit geoid |
| [country-state-city](https://www.npmjs.com/package/country-state-city) 3.2.1 `city.json` (same dr5hn source) | 148,038 | 1.70 MB | name, codes, lat/lng strings |
| [cities.json](https://www.npmjs.com/package/cities.json) 1.1.64 (GeoNames cities1000) | 171,075 | 2.34 MB | name, lat/lng, ISO2, admin1 code |
| [all-the-cities](https://www.npmjs.com/package/all-the-cities) 3.1.0 (protobuf) | 138,398 | 2.68 MB | GeoNames fields |
| [browser-geocoder-geonames](https://github.com/AshKyd/browser-geocoder-geonames) | GeoNames | ~3.5 MB (its README) | forward/reverse geocoding, no map |

The datasets differ in fields and counts, so this is a bytes-per-city comparison, not a like-for-like race — but a spatially sorted columnar file with 32-bit geoids is clearly a compact way to ship a gazetteer. Node-side offline geocoders are much bigger: [local-reverse-geocoder](https://www.npmjs.com/package/local-reverse-geocoder) downloads "roughly 2GB" on first run, [offline-geocoder](https://www.npmjs.com/package/offline-geocoder)'s SQLite database is "roughly 12 MB".

**Tiles and spatial formats:** a [PMTiles](https://docs.protomaps.com/basemaps/downloads) world basemap is 17 MB at zoom 0–5 ([Simon Willison's measurements](https://til.simonwillison.net/gis/pmtiles)) and still needs a renderer; the packed 1:50m coastlines here are 110 KB. [FlatGeobuf](https://github.com/flatgeobuf/flatgeobuf) (15 KB) and [h3-js](https://github.com/uber/h3-js) (54 KB) are formats and index math, without data.

**GPU compute:** I found no public library or demo that runs geospatial queries as WebGPU compute shaders; deck.gl's WebGPU backend is "not production ready" and its guide does not mention compute. The nearest related work is WebGPU *relational* query processing ([WGLog, arXiv 2607.17571](https://arxiv.org/abs/2607.17571)).

So, as far as a search can tell (it cannot rule out every hobby demo): every mainstream engine ships the renderer and streams the world; the city packages ship the data without a globe, index or search; nothing I found combines a 3D globe, a 150k-city database, offline search, radius/k-nearest queries and a CPU-vs-GPU benchmark in about 1 MB.

### A.7 Lessons

1. **Measure the wire, not the file.** Base64 + brotli, gzip inside gzip: the intuitive choice lost. Raw columns + HTTP brotli beat the one-file page by 13% (1.30 vs 1.49 MB).
2. **A sort order can be an index.** Z-order + covering cells gave exact spatial queries without shipping an index — as long as you respect the curve's jumps (which the old `find_nearest` did not).
3. **vsync hides everything.** 60 fps on a 60 Hz display is not a benchmark; render offscreen without it.
4. **GPU compute is a batch tool.** One query per round trip loses to a CPU index; a thousand per dispatch wins where the CPU's cost grows with the result.
5. **A second implementation is the best test.** Two engines on the same random queries, with an "agrees" row, found a 25% wrong-answer bug that unit tests with hand-picked cities never hit.

Try it: `crates/geodb-globe` (`mini.html`, `flex.html`, `scripts/one_in_all.py`, `scripts/web_release.py`), and `cargo run --release -p geodb-globe --example make_mini_assets` for the data.
