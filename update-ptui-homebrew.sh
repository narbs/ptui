#!/usr/bin/env bash

set -euo pipefail

# Script to update the ptui formula with a new version

VERSION="${1:-}"

if [ -z "$VERSION" ]; then
  echo "Usage: $0 <version>"
  echo "Example: $0 1.0.10"
  exit 1
fi

# Remove 'v' prefix if provided
VERSION="${VERSION#v}"

FORMULA_FILE="Formula/narbs-ptui.rb"
DOWNLOAD_URL="https://github.com/narbs/ptui/archive/refs/tags/v${VERSION}.tar.gz"
TEMP_FILE=$(mktemp)

echo "Updating ptui formula to version ${VERSION}..."

# Download the release tarball
echo "Downloading ${DOWNLOAD_URL}..."
if ! curl -fsSL -o "$TEMP_FILE" "$DOWNLOAD_URL"; then
  echo "Error: Failed to download release tarball"
  rm -f "$TEMP_FILE"
  exit 1
fi

# Calculate SHA256
echo "Calculating SHA256 hash..."
if command -v sha256sum &> /dev/null; then
  SHA256=$(sha256sum "$TEMP_FILE" | awk '{print $1}')
elif command -v shasum &> /dev/null; then
  SHA256=$(shasum -a 256 "$TEMP_FILE" | awk '{print $1}')
else
  echo "Error: Neither sha256sum nor shasum found"
  rm -f "$TEMP_FILE"
  exit 1
fi

echo "SHA256: $SHA256"

# Clean up temp file
rm -f "$TEMP_FILE"

# Update the formula file
echo "Updating ${FORMULA_FILE}..."

cd /home/linuxbrew/.linuxbrew/Homebrew/Library/Taps/narbs/homebrew-tap

# Create a backup
cp "$FORMULA_FILE" "${FORMULA_FILE}.bak"

# Update the URL and SHA256 using sed
sed -i.tmp \
  -e "s|url \"https://github.com/narbs/ptui/archive/refs/tags/v.*\.tar\.gz\"|url \"${DOWNLOAD_URL}\"|" \
  -e "s|sha256 \".*\"|sha256 \"${SHA256}\"|" \
  "$FORMULA_FILE"

rm -f "${FORMULA_FILE}.tmp"

echo "Successfully updated ${FORMULA_FILE}"
echo ""
echo "Changes:"
echo "  Version: v${VERSION}"
echo "  SHA256:  ${SHA256}"
echo ""
echo "Please review the changes and commit them:"
echo "  cd /home/linuxbrew/.linuxbrew/Homebrew/Library/Taps/narbs/homebrew-tap/narbs-ptui"
echo "  git diff ${FORMULA_FILE}"
echo "  git add ${FORMULA_FILE}"
echo "  git commit -m \"Update ptui to v${VERSION}\""
echo "  git push"
