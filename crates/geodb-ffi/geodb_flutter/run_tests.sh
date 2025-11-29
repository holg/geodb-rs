#!/usr/bin/env bash
set -e

echo "======================================================================"
echo "GeoDB Flutter Plugin - Test Suite"
echo "======================================================================"

# Colors for output
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Check if we're in the right directory
if [ ! -f "pubspec.yaml" ]; then
    echo "Error: Must run from geodb_flutter directory"
    exit 1
fi

# Parse command line arguments
RUN_UNIT=true
RUN_INTEGRATION=false
DEVICE=""
VERBOSE=false

while [[ $# -gt 0 ]]; do
    case $1 in
        --integration|-i)
            RUN_INTEGRATION=true
            shift
            ;;
        --unit-only|-u)
            RUN_INTEGRATION=false
            shift
            ;;
        --device|-d)
            DEVICE="$2"
            shift 2
            ;;
        --verbose|-v)
            VERBOSE=true
            shift
            ;;
        --help|-h)
            echo "Usage: $0 [OPTIONS]"
            echo ""
            echo "Options:"
            echo "  -u, --unit-only       Run only unit tests (default)"
            echo "  -i, --integration     Run integration tests (requires device)"
            echo "  -d, --device DEVICE   Specify device for integration tests (ios/macos)"
            echo "  -v, --verbose         Verbose output"
            echo "  -h, --help            Show this help message"
            echo ""
            echo "Examples:"
            echo "  $0                          # Run unit tests only"
            echo "  $0 -i -d macos              # Run integration tests on macOS"
            echo "  $0 -i -d ios                # Run integration tests on iOS"
            exit 0
            ;;
        *)
            echo "Unknown option: $1"
            echo "Use --help for usage information"
            exit 1
            ;;
    esac
done

# Step 1: Run unit tests
if [ "$RUN_UNIT" = true ]; then
    echo ""
    echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo -e "${BLUE}Running Unit Tests${NC}"
    echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo ""

    if [ "$VERBOSE" = true ]; then
        flutter test --verbose
    else
        flutter test
    fi

    if [ $? -eq 0 ]; then
        echo ""
        echo -e "${GREEN}✅ Unit tests passed!${NC}"
    else
        echo ""
        echo -e "${YELLOW}❌ Unit tests failed!${NC}"
        exit 1
    fi
fi

# Step 2: Run integration tests
if [ "$RUN_INTEGRATION" = true ]; then
    echo ""
    echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo -e "${BLUE}Running Integration Tests${NC}"
    echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo ""

    # Check if SPM package is added
    echo -e "${YELLOW}⚠️  Integration tests require the SPM package to be added in Xcode${NC}"
    echo ""
    echo "Make sure you have:"
    echo "  1. Added SPM-GeoDB-ffi package to the Runner target"
    echo "  2. Built the project at least once"
    echo ""
    read -p "Press Enter to continue or Ctrl+C to cancel..."

    cd example

    # Determine device
    if [ -z "$DEVICE" ]; then
        echo ""
        echo "Available devices:"
        flutter devices
        echo ""
        read -p "Enter device ID (or 'macos' for macOS, 'ios' for iOS Simulator): " DEVICE
    fi

    echo ""
    echo "Running integration tests on device: $DEVICE"
    echo ""

    if [ "$VERBOSE" = true ]; then
        flutter test integration_test/geodb_integration_test.dart -d "$DEVICE" --verbose
    else
        flutter test integration_test/geodb_integration_test.dart -d "$DEVICE"
    fi

    if [ $? -eq 0 ]; then
        echo ""
        echo -e "${GREEN}✅ Integration tests passed!${NC}"
    else
        echo ""
        echo -e "${YELLOW}❌ Integration tests failed!${NC}"
        exit 1
    fi

    cd ..
fi

# Summary
echo ""
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${GREEN}✅ All tests completed successfully!${NC}"
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo ""
