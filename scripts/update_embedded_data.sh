#!/usr/bin/env bash
#
# update_embedded_data.sh - Download and build fresh database for FFI
#

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
CORE_CRATE="$ROOT_DIR/crates/geodb-core"
FFI_DATA_DIR="$ROOT_DIR/crates/geodb-ffi/geodb_rs_data"

GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
NC='\033[0m'

echo "======================================================================"
echo "Update Embedded Database"
echo "======================================================================"
echo ""

# Check current data
if [ -f "$FFI_DATA_DIR/geodb.flat.comp.blobs.bin" ]; then
    CURRENT_SIZE=$(du -h "$FFI_DATA_DIR/geodb.flat.comp.blobs.bin" | cut -f1)
    CURRENT_DATE=$(stat -f "%Sm" -t "%Y-%m-%d %H:%M" "$FFI_DATA_DIR/geodb.flat.comp.blobs.bin")
    echo "Current data: $CURRENT_SIZE (from $CURRENT_DATE)"
else
    echo "No embedded data found"
fi

echo ""
echo -e "${BLUE}Downloading latest data from GitHub...${NC}"
echo ""

# Upstream publishes the gzipped dataset as a release asset;
# "latest/download" always resolves to the newest release.
DATA_URL="https://github.com/dr5hn/countries-states-cities-database/releases/latest/download/json-countries%2Bstates%2Bcities.json.gz"
DATA_DIR="$CORE_CRATE/data"
DATA_FILE="$DATA_DIR/countries+states+cities.json.gz"

mkdir -p "$DATA_DIR"

# Download to a temp file first so a failed download never replaces good data
curl -fL -o "$DATA_FILE.part" "$DATA_URL"
if [ "$(head -c 2 "$DATA_FILE.part" | xxd -p)" != "1f8b" ]; then
    echo -e "${YELLOW}Error: download is not gzip data${NC}"
    rm -f "$DATA_FILE.part"
    exit 1
fi
mv "$DATA_FILE.part" "$DATA_FILE"

DOWNLOAD_SIZE=$(du -h "$DATA_FILE" | cut -f1)
echo ""
echo -e "${GREEN}✓ Download complete${NC} ($DOWNLOAD_SIZE)"
echo ""

# Build the embedded binaries. `geodb-cli build` writes
# data/geodb.<model>.comp.blobs.bin (and a copy into the working directory,
# which is data/ here as well).
echo -e "${BLUE}Building optimized binary caches...${NC}"
echo ""
cd "$DATA_DIR"
cargo run --release --manifest-path "$ROOT_DIR/Cargo.toml" -p geodb-cli -- build
cargo run --release --manifest-path "$ROOT_DIR/Cargo.toml" -p geodb-cli --features legacy_model -- build

for f in geodb.flat.comp.blobs.bin geodb.nested.comp.blobs.bin; do
    if [ ! -f "$DATA_DIR/$f" ]; then
        echo -e "${YELLOW}Error: $f was not generated${NC}"
        exit 1
    fi
done
NEW_SIZE=$(du -h "$DATA_DIR/geodb.flat.comp.blobs.bin" | cut -f1)

# crates/geodb-ffi/geodb_rs_data is a symlink to crates/geodb-core/data,
# so the FFI, Python and WASM crates pick the new files up directly.
echo -e "${GREEN}✓ Embedded data updated${NC}"
echo ""

# Show summary
echo "======================================================================"
echo -e "${GREEN}✅ Database Updated Successfully${NC}"
echo "======================================================================"
echo ""
echo "Location: $DATA_DIR/geodb.{flat,nested}.comp.blobs.bin"
echo "Size:     $NEW_SIZE"
echo "Source:   https://github.com/dr5hn/countries-states-cities-database"
echo ""
echo "Next steps:"
echo ""
echo "1. Rebuild the SPM package:"
echo "   ${BLUE}./scripts/build_spm_universal.sh${NC}"
echo ""
echo "2. Create release:"
echo "   ${BLUE}./scripts/publish_manual_release.sh v0.1.4${NC}"
echo ""
