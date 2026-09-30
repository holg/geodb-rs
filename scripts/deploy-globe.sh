#!/usr/bin/env bash
# Deploys the geodb-globe mini demo (WebGPU + WebGL2 builds, data layers, detail
# imagery) to trahe.eu: https://trahe.eu/geodb-rs/geodb-globe/
#
#   scripts/deploy-globe.sh             # build (web_release.py) and upload
#   scripts/deploy-globe.sh --no-build  # upload the last dist-web/
#   scripts/deploy-globe.sh --dry-run   # show what would change, upload nothing
#
# The site is served with precompressed files: dist-web/ holds raw data next to
# .br and .gz copies (nginx `brotli_static on; gzip_static on;`). rsync --delete
# keeps the directory equal to dist-web/, so renamed files (the old earth-16k.webp,
# the hashed wasm/js of earlier builds) go away. The directory is for this demo only.
#
# Needs: trunk, brotli, and `python3 crates/geodb-globe/scripts/fetch_detail.py`
# run once (the detail imagery is generated, not in git).

set -euo pipefail

BLUE='\033[0;34m'; GREEN='\033[0;32m'; RED='\033[0;31m'; NC='\033[0m'

REMOTE_HOST="trahe.eu"
REMOTE_PATH="/var/www/trahe/html/geodb-rs/geodb-globe"
LOCAL_DIST="crates/geodb-globe/dist-web"

build=1
dry=()
for arg in "$@"; do
    case "$arg" in
        --no-build) build=0 ;;
        --dry-run) dry=(--dry-run) ;;
        *) echo -e "${RED}unknown option: $arg${NC}"; exit 1 ;;
    esac
done

if [ ! -d "crates/geodb-globe" ]; then
    echo -e "${RED}Error: Must run from repository root${NC}"
    exit 1
fi

if [ "$build" = 1 ]; then
    echo -e "${BLUE}Step 1: building the globe release (both builds, .br/.gz)...${NC}"
    python3 crates/geodb-globe/scripts/web_release.py
    echo -e "${GREEN}✓ Build complete${NC}\n"
fi

if [ ! -f "$LOCAL_DIST/index.html" ]; then
    echo -e "${RED}Error: $LOCAL_DIST not found. Run without --no-build${NC}"
    exit 1
fi
if ! ls "$LOCAL_DIST"/earth-16k-*.webp >/dev/null 2>&1; then
    echo -e "${RED}Warning: no detail imagery in $LOCAL_DIST (run crates/geodb-globe/scripts/fetch_detail.py)${NC}"
fi

echo -e "${BLUE}Step 2: uploading to ${REMOTE_HOST}:${REMOTE_PATH}...${NC}"
ssh "$REMOTE_HOST" "mkdir -p '$REMOTE_PATH'"
rsync -av --delete "${dry[@]}" "$LOCAL_DIST"/ "${REMOTE_HOST}:${REMOTE_PATH}/"

echo -e "\n${GREEN}✓ Globe deployment complete!${NC}"
echo -e "${BLUE}View at: https://trahe.eu/geodb-rs/geodb-globe/${NC}"
