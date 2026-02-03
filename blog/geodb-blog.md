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
