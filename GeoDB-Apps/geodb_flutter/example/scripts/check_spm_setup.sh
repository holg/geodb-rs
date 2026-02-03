#!/usr/bin/env bash

echo "======================================================================"
echo "Checking SPM Package Setup"
echo "======================================================================"

SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
EXAMPLE_DIR="$SCRIPT_DIR/.."
SPM_PACKAGE_PATH="/Users/htr/Documents/develeop/rust/geodb-rs/crates/SPM-GeoDB-ffi"

# Colors
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

check_spm_exists() {
    echo ""
    echo "1. Checking if SPM package exists..."
    if [ -d "$SPM_PACKAGE_PATH" ]; then
        echo -e "   ${GREEN}✅ SPM package found${NC}"
        return 0
    else
        echo -e "   ${RED}❌ SPM package not found at: $SPM_PACKAGE_PATH${NC}"
        echo "   Build it with: ./scripts/build_spm_package.sh"
        return 1
    fi
}

check_project_references() {
    local PLATFORM=$1
    echo ""
    echo "2. Checking $PLATFORM project references..."

    local PBXPROJ="$EXAMPLE_DIR/$PLATFORM/Runner.xcodeproj/project.pbxproj"

    if [ ! -f "$PBXPROJ" ]; then
        echo -e "   ${YELLOW}⚠️  Project not found: $PBXPROJ${NC}"
        return 1
    fi

    if grep -q "GeodbKit" "$PBXPROJ"; then
        echo -e "   ${GREEN}✅ GeodbKit reference found in project${NC}"
        return 0
    else
        echo -e "   ${YELLOW}⚠️  GeodbKit not found in project${NC}"
        echo "   This means the SPM package hasn't been added in Xcode yet."
        return 1
    fi
}

check_workspace() {
    local PLATFORM=$1
    echo ""
    echo "3. Checking $PLATFORM workspace..."

    local WORKSPACE="$EXAMPLE_DIR/$PLATFORM/Runner.xcworkspace"

    if [ -d "$WORKSPACE" ]; then
        echo -e "   ${GREEN}✅ Workspace exists${NC}"
        return 0
    else
        echo -e "   ${RED}❌ Workspace not found${NC}"
        return 1
    fi
}

try_build() {
    local PLATFORM=$1
    echo ""
    echo "4. Attempting to build $PLATFORM..."

    cd "$EXAMPLE_DIR"

    if flutter build $PLATFORM --debug 2>&1 | grep -q "Unable to find module dependency: 'GeodbKit'"; then
        echo -e "   ${RED}❌ Build failed - GeodbKit module not found${NC}"
        echo "   The SPM package needs to be added in Xcode."
        return 1
    elif flutter build $PLATFORM --debug 2>&1 | grep -q "BUILD FAILED"; then
        echo -e "   ${RED}❌ Build failed${NC}"
        return 1
    else
        echo -e "   ${GREEN}✅ Build succeeded (or is in progress)${NC}"
        return 0
    fi
}

# Main checks
echo ""
echo "Checking SPM package setup for example app..."
echo ""

SPM_EXISTS=0
IOS_OK=0
MACOS_OK=0

# Check SPM package
if check_spm_exists; then
    SPM_EXISTS=1
fi

# Check iOS
echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "iOS Setup"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

if check_workspace "ios"; then
    if check_project_references "ios"; then
        IOS_OK=1
    fi
fi

# Check macOS
echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "macOS Setup"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

if check_workspace "macos"; then
    if check_project_references "macos"; then
        MACOS_OK=1
    fi
fi

# Summary
echo ""
echo "======================================================================"
echo "Summary"
echo "======================================================================"
echo ""

if [ $SPM_EXISTS -eq 1 ]; then
    echo -e "${GREEN}✅ SPM Package: Ready${NC}"
else
    echo -e "${RED}❌ SPM Package: Not built${NC}"
fi

if [ $IOS_OK -eq 1 ]; then
    echo -e "${GREEN}✅ iOS: SPM package configured${NC}"
else
    echo -e "${YELLOW}⚠️  iOS: SPM package not added to Xcode project${NC}"
fi

if [ $MACOS_OK -eq 1 ]; then
    echo -e "${GREEN}✅ macOS: SPM package configured${NC}"
else
    echo -e "${YELLOW}⚠️  macOS: SPM package not added to Xcode project${NC}"
fi

echo ""

if [ $IOS_OK -eq 0 ] || [ $MACOS_OK -eq 0 ]; then
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo "To Fix: Add SPM Package in Xcode"
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo ""

    if [ $IOS_OK -eq 0 ]; then
        echo "For iOS:"
        echo "  1. open ios/Runner.xcworkspace"
        echo "  2. File → Add Package Dependencies..."
        echo "  3. Add Local → Select: $SPM_PACKAGE_PATH"
        echo "  4. Add to Runner target"
        echo ""
    fi

    if [ $MACOS_OK -eq 0 ]; then
        echo "For macOS:"
        echo "  1. open macos/Runner.xcworkspace"
        echo "  2. File → Add Package Dependencies..."
        echo "  3. Add Local → Select: $SPM_PACKAGE_PATH"
        echo "  4. Add to Runner target"
        echo ""
    fi

    echo "See SETUP_SPM.md for detailed instructions."
fi

echo "======================================================================"
echo ""

# Exit code
if [ $SPM_EXISTS -eq 1 ] && [ $IOS_OK -eq 1 ] && [ $MACOS_OK -eq 1 ]; then
    echo -e "${GREEN}✅ All checks passed!${NC}"
    exit 0
else
    echo -e "${YELLOW}⚠️  Some checks failed. See above for details.${NC}"
    exit 1
fi
