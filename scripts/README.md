# Scripts

This directory contains build and testing scripts for the geodb-rs project.

## test-ci-locally.sh

Comprehensive CI testing script that mimics GitHub Actions workflows locally.

### Basic Usage

```bash
# Run all checks (no auto-fixes)
./scripts/test-ci-locally.sh

# Show help
./scripts/test-ci-locally.sh --help
```

### Auto-Fix Mode

Automatically fix formatting issues when they're detected:

```bash
# Auto-fix cargo fmt, cargo-sort, and taplo issues
./scripts/test-ci-locally.sh --fix
```

**What gets auto-fixed:**
- ✅ `cargo fmt` - Rust code formatting
- ✅ `cargo-sort` - Cargo.toml dependency sorting
- ✅ `taplo` - TOML file formatting

**What does NOT get auto-fixed:**
- ❌ Clippy warnings (must be fixed manually)
- ❌ Build errors
- ❌ Test failures
- ❌ Documentation issues

### Deploy Mode

Run all checks, and if successful, commit and push changes:

```bash
# Check, commit all changes, and push
./scripts/test-ci-locally.sh --deploy "Fix clippy warnings"

# Auto-fix formatting AND deploy
./scripts/test-ci-locally.sh --fix --deploy "Update code formatting"
```

**Deploy workflow:**
1. Run all CI checks
2. If checks pass:
   - Stage all changes (`git add -A`)
   - Commit with provided message
   - Push to current branch
3. Show commit info and success message

**Safety:**
- Deploy only runs if ALL checks pass
- Aborts if push fails (e.g., need to pull first)
- Shows all changes before committing

### Examples

**Typical development workflow:**

```bash
# 1. Make code changes
# ...

# 2. Run checks and auto-fix formatting
./scripts/test-ci-locally.sh --fix

# 3. If all passes, deploy
./scripts/test-ci-locally.sh --deploy "Add HarmonyOS translations"
```

**Quick fix and deploy:**

```bash
# One-shot: fix formatting and deploy if tests pass
./scripts/test-ci-locally.sh --fix --deploy "Format code and fix warnings"
```

**Just check without changes:**

```bash
# Check if code is ready for CI (no fixes, no deploy)
./scripts/test-ci-locally.sh
```

### What Gets Tested

The script runs the following checks in order:

1. **Formatting** (`cargo fmt --check`)
2. **Linting** (`cargo clippy`)
3. **Build** (all crates including geodb-py with maturin)
4. **Documentation** (`cargo doc`)
5. **Additional checks:**
   - `cargo-sort` - Cargo.toml dependency order
   - `taplo` - TOML formatting
   - `cargo-deny` - Dependency licenses and security
6. **Tests:**
   - Native Rust tests (single-threaded)
   - WASM tests (Node.js via wasm-pack)
   - Python tests (pytest via maturin develop)
7. **Pre-publish validation:**
   - Package metadata
   - Dry-run publish (crates.io)
8. **Git checks:**
   - Uncommitted changes
   - Current branch
   - Version tag conflicts
9. **Documentation build** (docs.rs preview)

### Requirements

**Required:**
- Rust toolchain (stable + nightly)
- Git

**Optional (for full checks):**
- `cargo-sort` - `cargo install cargo-sort`
- `taplo` - `cargo install taplo-cli`
- `cargo-deny` - `cargo install cargo-deny`
- `maturin` - `pip install maturin` (for geodb-py)
- `wasm-pack` - `cargo install wasm-pack` (for geodb-wasm)
- `trunk` - `cargo install trunk` (for WASM demo)
- `wasm-bindgen-cli` - `cargo install wasm-bindgen-cli`
- `pytest` - `pip install pytest` (for Python tests)

Missing tools are skipped with warnings.

### Exit Codes

- `0` - All checks passed (and deploy succeeded if requested)
- `1` - One or more checks failed (or deploy failed)

### CI Equivalence

This script closely matches the GitHub Actions workflows:

- `.github/workflows/rust.yml` - Main CI checks
- `.github/workflows/wasm.yml` - WASM build
- `.github/workflows/python.yml` - Python package

Running this script successfully means your PR should pass CI.

### Tips

**Before pushing:**
```bash
./scripts/test-ci-locally.sh --fix
# Review auto-fixed changes
git diff
# If happy, deploy
./scripts/test-ci-locally.sh --deploy "Commit message"
```

**Quick iteration:**
```bash
# Fix formatting issues automatically
./scripts/test-ci-locally.sh --fix

# Fix clippy warnings manually
# ...

# Deploy when ready
./scripts/test-ci-locally.sh --deploy "Fix warnings"
```

**CI debugging:**
```bash
# If CI fails but you're not sure why
./scripts/test-ci-locally.sh
# Script will show exactly which check failed
```

## Other Scripts

### build_android.sh
Build Android app with Rust library.

### build_spm_package.sh
Build Swift Package Manager framework (iOS/macOS).

### test-ci-locally.sh (deprecated methods)
Old script name - now with enhanced features above.

### deploy-wasm.sh
Deploy WASM demo to hosting.

### cross_build_on_mac.sh
Cross-compile for multiple targets on macOS.

## Contributing

When adding new CI checks to GitHub Actions, also update `test-ci-locally.sh` to maintain equivalence.
