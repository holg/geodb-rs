#!/usr/bin/env bash
#
# setup_signing.sh - Setup Android app signing
#
# Creates keystore and .env file for secure signing
#

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

echo "======================================================================"
echo "Android App Signing Setup"
echo "======================================================================"
echo ""

# Check if already set up
if [ -f ".env" ] && [ -f "app/release.keystore" ]; then
    echo -e "${YELLOW}Signing already configured!${NC}"
    echo ""
    echo "Found:"
    echo "  ✓ .env"
    echo "  ✓ app/release.keystore"
    echo ""
    read -p "Recreate? This will DELETE the existing keystore! [y/N] " -n 1 -r
    echo
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        echo "Aborted."
        exit 0
    fi
    echo ""
    echo -e "${RED}⚠️  WARNING: Deleting existing keystore!${NC}"
    echo "If this keystore was used for a published app, you won't be able to update it!"
    echo ""
    read -p "Are you SURE? [y/N] " -n 1 -r
    echo
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        echo "Aborted."
        exit 0
    fi
    rm -f app/release.keystore .env
fi

echo "This script will:"
echo "  1. Generate a release keystore"
echo "  2. Create .env with signing credentials"
echo ""
echo -e "${YELLOW}Important:${NC}"
echo "  - Keep the keystore SAFE and BACKED UP!"
echo "  - NEVER commit .env or .keystore to git!"
echo "  - You need this keystore to update your app!"
echo ""

# Collect information
echo -e "${BLUE}Keystore Configuration${NC}"
echo ""

read -p "Keystore password (min 6 chars): " -s KEYSTORE_PASSWORD
echo
if [ ${#KEYSTORE_PASSWORD} -lt 6 ]; then
    echo -e "${RED}Error: Password too short${NC}"
    exit 1
fi

read -p "Key password (min 6 chars, can be same as keystore): " -s KEY_PASSWORD
echo
if [ ${#KEY_PASSWORD} -lt 6 ]; then
    echo -e "${RED}Error: Password too short${NC}"
    exit 1
fi

echo ""
echo -e "${BLUE}App Information${NC}"
echo "(This info goes in the keystore certificate)"
echo ""

read -p "Your name: " CERT_NAME
read -p "Organization (optional): " CERT_ORG
read -p "City: " CERT_CITY
read -p "State/Province: " CERT_STATE
read -p "Country (2 letters, e.g. US): " CERT_COUNTRY

echo ""
echo -e "${BLUE}Generating keystore...${NC}"
echo ""

# Generate keystore
mkdir -p app
keytool -genkey -v \
    -keystore app/release.keystore \
    -alias geodb \
    -keyalg RSA \
    -keysize 2048 \
    -validity 10000 \
    -storepass "$KEYSTORE_PASSWORD" \
    -keypass "$KEY_PASSWORD" \
    -dname "CN=$CERT_NAME, O=$CERT_ORG, L=$CERT_CITY, ST=$CERT_STATE, C=$CERT_COUNTRY"

echo ""
echo -e "${GREEN}✓ Keystore created${NC}"
echo ""

# Create .env file
echo -e "${BLUE}Creating .env file...${NC}"
echo ""

cat > .env <<EOF
# Android Signing Configuration
# NEVER commit this file to git!
# Generated on $(date)

# Keystore configuration
KEYSTORE_FILE=release.keystore
KEYSTORE_PASSWORD=$KEYSTORE_PASSWORD
KEY_ALIAS=geodb
KEY_PASSWORD=$KEY_PASSWORD

# App configuration
APPLICATION_ID=com.example.geodb
VERSION_NAME=0.1.4
VERSION_CODE=4
EOF

echo -e "${GREEN}✓ .env created${NC}"
echo ""

# Show keystore info
echo -e "${BLUE}Keystore Information:${NC}"
echo ""
keytool -list -v -keystore app/release.keystore -storepass "$KEYSTORE_PASSWORD" | head -20

echo ""
echo "======================================================================"
echo -e "${GREEN}✅ Signing Setup Complete!${NC}"
echo "======================================================================"
echo ""
echo "Files created:"
echo "  ✓ app/release.keystore ($(du -h app/release.keystore | cut -f1))"
echo "  ✓ .env"
echo ""
echo -e "${YELLOW}IMPORTANT - Backup Your Keystore:${NC}"
echo ""
echo "1. Copy to a safe location:"
echo "   ${BLUE}cp app/release.keystore ~/Backup/geodb-release.keystore${NC}"
echo ""
echo "2. Store credentials securely (e.g., 1Password, LastPass)"
echo ""
echo "3. NEVER commit to git (already in .gitignore)"
echo ""
echo -e "${YELLOW}What happens if you lose the keystore?${NC}"
echo "  ❌ You cannot update your published app"
echo "  ❌ You must create a new app with a new package name"
echo "  ❌ Users must uninstall old app and install new one"
echo ""
echo "Next steps:"
echo ""
echo "1. Build signed release:"
echo "   ${BLUE}./scripts/build_android_release.sh${NC}"
echo ""
echo "2. Test the APK/AAB before publishing!"
echo ""
