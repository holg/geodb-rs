#!/usr/bin/env bash
# File: /Users/htr/Documents/develeop/rust/geodb-rs/scripts/test-ci-locally.sh

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo -e "${YELLOW}=== Testing CI Workflow Locally ===${NC}\n"

# Run cargo fmt check
echo -e "${YELLOW}Step 1: Running cargo fmt check...${NC}"
if cargo fmt --all -- --check; then
    echo -e "${GREEN}✓ cargo fmt passed${NC}\n"
else
    echo -e "${RED}✗ cargo fmt failed${NC}"
    echo -e "${YELLOW}Run 'cargo fmt --all' to fix formatting issues${NC}\n"
    exit 1
fi

# Run clippy
echo -e "${YELLOW}Step 2: Running clippy...${NC}"
if cargo clippy --locked --workspace --all-targets -- -D warnings; then
    echo -e "${GREEN}✓ clippy passed${NC}\n"
else
    echo -e "${RED}✗ clippy failed${NC}\n"
    exit 1
fi

# Run cargo build
echo -e "${YELLOW}Step 3: Running cargo build...${NC}"
# Exclude geodb-py as it requires maturin to build
if cargo build --locked --workspace --exclude geodb-py; then
    echo -e "${GREEN}✓ build passed${NC}\n"
else
    echo -e "${RED}✗ build failed${NC}\n"
    exit 1
fi

# Build geodb-py separately with maturin
echo -e "${YELLOW}Step 3b: Building geodb-py with maturin...${NC}"
if command -v maturin &> /dev/null; then
#    if (source crates/geodb-py/.env_py312/bin/activate && maturin build --locked); then
    if (cd crates/geodb-py && source .env_py312/bin/activate && maturin build --locked); then
        echo -e "${GREEN}✓ geodb-py build passed${NC}\n"
    else
        echo -e "${RED}✗ geodb-py build failed${NC}\n"
        exit 1
    fi
else
    echo -e "${YELLOW}⚠ maturin not installed, skipping geodb-py build${NC}"
    echo -e "${YELLOW}Install with: pip install maturin${NC}\n"
fi

# Run cargo doc
echo -e "${YELLOW}Step 4: Running cargo doc...${NC}"
if RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --document-private-items --no-deps; then
    echo -e "${GREEN}✓ doc generation passed${NC}\n"
else
    echo -e "${RED}✗ doc generation failed${NC}\n"
    exit 1
fi

# Additional checks
echo -e "${YELLOW}Step 5: Running additional checks...${NC}"

# Check cargo-sort FIRST (it should run before taplo)
if command -v cargo-sort &> /dev/null; then
    echo "Running cargo-sort check..."
    if cargo-sort -cwg; then
        echo -e "${GREEN}✓ cargo-sort passed${NC}"
    else
        echo -e "${RED}✗ cargo-sort failed${NC}"
        echo -e "${YELLOW}Run 'cargo-sort -wg' to fix Cargo.toml sorting${NC}"
        exit 1
    fi
else
    echo -e "${YELLOW}⚠ cargo-sort not installed, skipping Cargo.toml sort check${NC}"
    echo -e "${YELLOW}Install with: cargo install cargo-sort${NC}"
fi

# Check taplo AFTER cargo-sort (taplo formats what cargo-sort organized)
if command -v taplo &> /dev/null; then
    echo "Running taplo format check..."
    if taplo format --check; then
        echo -e "${GREEN}✓ taplo passed${NC}"
    else
        echo -e "${RED}✗ taplo failed${NC}"
        echo -e "${YELLOW}Run 'taplo format' to fix TOML formatting${NC}"
        exit 1
    fi
else
    echo -e "${YELLOW}⚠ taplo not installed, skipping TOML format check${NC}"
    echo -e "${YELLOW}Install with: cargo install taplo-cli${NC}"
fi

# Check cargo-deny
if command -v cargo-deny &> /dev/null; then
    echo "Running cargo-deny check..."
    if cargo-deny check --hide-inclusion-graph --show-stats; then
        echo -e "${GREEN}✓ cargo-deny passed${NC}"
    else
        echo -e "${RED}✗ cargo-deny failed${NC}"
        exit 1
    fi
else
    echo -e "${YELLOW}⚠ cargo-deny not installed, skipping dependency check${NC}"
    echo -e "${YELLOW}Install with: cargo install cargo-deny${NC}"
fi

# Run Rust tests (native)
echo -e "${YELLOW}Step 6: Running Rust tests (native targets)...${NC}"
if cargo test --locked --workspace --exclude geodb-wasm --exclude geodb-py -- --test-threads=1; then
    echo -e "${GREEN}✓ native tests passed${NC}\n"
else
    echo -e "${RED}✗ native tests failed${NC}\n"
    exit 1
fi

# Run WASM tests for geodb-wasm if tooling is available
echo -e "${YELLOW}Step 6b: Running geodb-wasm tests (wasm32, Node)...${NC}"
if command -v wasm-pack &> /dev/null; then
    if (cd crates/geodb-wasm && wasm-pack test --node); then
        echo -e "${GREEN}✓ geodb-wasm tests passed (node)${NC}\n"
    else
        echo -e "${RED}✗ geodb-wasm tests failed${NC}\n"
        exit 1
    fi
else
    echo -e "${YELLOW}⚠ wasm-pack not installed, skipping geodb-wasm tests${NC}"
    echo -e "${YELLOW}Install with: cargo install wasm-pack${NC}"
fi

# Build WASM target (demo app)
echo -e "${YELLOW}Step 7: Building WASM demo (Trunk)...${NC}"
if command -v trunk &> /dev/null && command -v wasm-bindgen &> /dev/null; then
    echo "Building geodb-wasm with Trunk..."
    if (cd crates/geodb-wasm && trunk build --release); then
        echo -e "${GREEN}✓ WASM build passed${NC}\n"
    else
        echo -e "${RED}✗ WASM build failed${NC}\n"
        exit 1
    fi
else
    echo -e "${YELLOW}⚠ trunk or wasm-bindgen-cli not installed, skipping WASM build${NC}"
    echo -e "${YELLOW}Install with:${NC}"
    echo -e "${YELLOW}  cargo install trunk${NC}"
    echo -e "${YELLOW}  cargo install wasm-bindgen-cli${NC}"
    echo -e "${YELLOW}  rustup target add wasm32-unknown-unknown${NC}"
fi

# Run Python tests for geodb-py if tooling is available
echo -e "${YELLOW}Step 7b: Running Python tests for geodb-py...${NC}"
if command -v python3 &> /dev/null && command -v maturin &> /dev/null; then
    TMP_VENV=".venv_geodb_test"
    python3 -m venv "$TMP_VENV"
    # shellcheck disable=SC1090
    source "$TMP_VENV/bin/activate"
    python -m pip install --upgrade pip >/dev/null 2>&1
    if python -c "import pytest" 2>/dev/null; then
        echo "pytest already available in venv"
    else
        python -m pip install pytest >/dev/null 2>&1 || true
    fi

    echo "Building and installing geodb-py into venv (maturin develop)..."
    if maturin develop -m crates/geodb-py/Cargo.toml --release >/dev/null; then
        echo -e "${GREEN}✓ geodb-py built and installed${NC}"
        echo "Running pytest..."
        if (cd crates/geodb-py && pytest -q); then
            echo -e "${GREEN}✓ geodb-py tests passed${NC}\n"
        else
            echo -e "${RED}✗ geodb-py tests failed${NC}\n"
            deactivate || true
            rm -rf "$TMP_VENV"
            exit 1
        fi
    else
        echo -e "${RED}✗ maturin develop failed for geodb-py${NC}"
        deactivate || true
        rm -rf "$TMP_VENV"
        exit 1
    fi

    deactivate || true
    rm -rf "$TMP_VENV"
else
    echo -e "${YELLOW}⚠ python3 or maturin not installed, skipping geodb-py tests${NC}"
    echo -e "${YELLOW}Install with:${NC}"
    echo -e "${YELLOW}  pipx install maturin  (or: pip install maturin)${NC}"
    echo -e "${YELLOW}  pip install pytest${NC}"
fi

# Pre-publish checks
echo -e "\n${BLUE}=== Pre-publish Checks for crates.io ===${NC}\n"

# Check package metadata
echo -e "${YELLOW}Step 8: Validating package metadata...${NC}"
for crate_dir in crates/*/; do
    crate_name=$(basename "$crate_dir")
    echo "Checking $crate_name..."

    if (cd "$crate_dir" && cargo package --list --allow-dirty > /dev/null 2>&1); then
        echo -e "${GREEN}✓ $crate_name package metadata valid${NC}"
    else
        echo -e "${RED}✗ $crate_name package metadata invalid${NC}"
        echo -e "${YELLOW}Run 'cd $crate_dir && cargo package --list' for details${NC}"
        exit 1
    fi
done
echo ""

# Dry-run publish in dependency order
echo -e "${YELLOW}Step 9: Running dry-run publish (in dependency order)...${NC}"

# Define publish order: dependencies first, then dependents
PUBLISH_ORDER=(
    "geodb-core"
    "geodb-wasm"
    "geodb-cli"
    "geodb-py"
)

for crate_name in "${PUBLISH_ORDER[@]}"; do
    crate_dir="crates/$crate_name"

    if [[ ! -d "$crate_dir" ]]; then
        echo -e "${YELLOW}⚠ Skipping $crate_name (directory not found)${NC}"
        continue
    fi

    echo "Validating $crate_name package..."

    # For geodb-core (no dependencies), do full dry-run publish
    if [[ "$crate_name" == "geodb-core" ]]; then
        if (cd "$crate_dir" && cargo publish --dry-run --allow-dirty); then
            echo -e "${GREEN}✓ $crate_name dry-run publish passed${NC}"
        else
            echo -e "${RED}✗ $crate_name dry-run publish failed${NC}"
            exit 1
        fi
    else
        # For dependent crates, just verify package contents
        # (can't do full dry-run until dependencies are on crates.io)
        if (cd "$crate_dir" && cargo package --allow-dirty --list > /dev/null 2>&1); then
            echo -e "${GREEN}✓ $crate_name package validation passed${NC}"
            echo -e "${BLUE}  Note: Full publish validation will happen after geodb-core is published${NC}"
        else
            echo -e "${RED}✗ $crate_name package validation failed${NC}"
            exit 1
        fi
    fi
done
echo ""

# Check for uncommitted changes
echo -e "${YELLOW}Step 10: Checking for uncommitted changes...${NC}"
if [[ -n $(git status --porcelain) ]]; then
    echo -e "${YELLOW}⚠ You have uncommitted changes:${NC}"
    git status --short
    echo -e "${YELLOW}Consider committing or stashing changes before publishing${NC}\n"
else
    echo -e "${GREEN}✓ No uncommitted changes${NC}\n"
fi

# Check if on main branch
echo -e "${YELLOW}Step 11: Checking git branch...${NC}"
current_branch=$(git branch --show-current)
if [[ "$current_branch" != "main" ]]; then
    echo -e "${YELLOW}⚠ You are on branch '$current_branch', not 'main'${NC}"
    echo -e "${YELLOW}Consider switching to main branch before publishing${NC}\n"
else
    echo -e "${GREEN}✓ On main branch${NC}\n"
fi

# Check for version tags
echo -e "${YELLOW}Step 12: Checking version consistency...${NC}"
for crate_name in "${PUBLISH_ORDER[@]}"; do
    crate_dir="crates/$crate_name"

    if [[ ! -d "$crate_dir" ]]; then
        continue
    fi

    version=$(grep -m1 '^version = ' "$crate_dir/Cargo.toml" | sed 's/.*"\(.*\)".*/\1/')

    if git tag | grep -q "^${crate_name}-v${version}$"; then
        echo -e "${YELLOW}⚠ Tag ${crate_name}-v${version} already exists${NC}"
        echo -e "${YELLOW}Consider bumping version in $crate_dir/Cargo.toml${NC}"
    else
        echo -e "${GREEN}✓ $crate_name v$version - version tag available${NC}"
    fi
done
echo ""

# Build documentation as it would appear on docs.rs
echo -e "${YELLOW}Step 13: Building documentation for review (as on docs.rs)...${NC}"
echo "Building docs with all features enabled..."

# Clean previous docs
rm -rf target/doc

# Build docs with docs.rs settings
if RUSTDOCFLAGS="--cfg docsrs" cargo +nightly doc --workspace --all-features --no-deps; then
    echo -e "${GREEN}✓ Documentation built successfully${NC}\n"

    # Find the main crate documentation
    DOC_PATH="target/doc/geodb_core/index.html"

    if [[ -f "$DOC_PATH" ]]; then
        echo -e "${BLUE}Opening documentation in browser...${NC}"

        # Detect OS and open browser accordingly
        if [[ "$OSTYPE" == "darwin"* ]]; then
            open "$DOC_PATH"
        elif [[ "$OSTYPE" == "linux-gnu"* ]]; then
            if command -v xdg-open &> /dev/null; then
                xdg-open "$DOC_PATH"
            else
                echo -e "${YELLOW}Please open: file://$(pwd)/$DOC_PATH${NC}"
            fi
        else
            echo -e "${YELLOW}Please open: file://$(pwd)/$DOC_PATH${NC}"
        fi

        echo -e "${GREEN}✓ Documentation opened for review${NC}"
        echo -e "${BLUE}Review the documentation at: file://$(pwd)/$DOC_PATH${NC}\n"
    else
        echo -e "${YELLOW}⚠ Documentation index not found at expected location${NC}"
        echo -e "${YELLOW}Check target/doc/ directory manually${NC}\n"
    fi
else
    echo -e "${RED}✗ Documentation build failed${NC}"
    echo -e "${YELLOW}Note: This uses nightly Rust with --cfg docsrs flag${NC}"
    echo -e "${YELLOW}Install nightly with: rustup toolchain install nightly${NC}\n"
fi

echo -e "\n${GREEN}=== All CI checks passed! ===${NC}"
echo -e "${GREEN}Your code is ready to be pushed.${NC}"
echo -e "\n${BLUE}Publishing Order (IMPORTANT - follow this sequence):${NC}"
echo -e "  ${YELLOW}1.${NC} Review the documentation that was just opened"
echo -e "  ${YELLOW}2.${NC} Ensure all changes are committed"
echo -e "  ${YELLOW}3.${NC} Publish ${BLUE}geodb-core${NC} first (it has no dependencies):"
echo -e "     ${YELLOW}cd crates/geodb-core && cargo publish${NC}"
echo -e "  ${YELLOW}4.${NC} Wait for geodb-core to be available on crates.io"