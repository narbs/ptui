#!/usr/bin/env bash

set -euo pipefail

# Update the ptui formula in the narbs/homebrew-tap to a released version, then commit and
# push the tap. The tag must already be on GitHub, since its source tarball is hashed.
#
# Usage: ./update-ptui-homebrew.sh [--no-push] [version]
#   version    defaults to the version in Cargo.toml; a leading 'v' is ignored
#   --no-push  commit the change in the tap but do not push it
# Set PTUI_TAP_DIR to use a tap checked out somewhere else.

TAP="narbs/homebrew-tap"
FALLBACK_TAP_DIR="/home/linuxbrew/.linuxbrew/Homebrew/Library/Taps/narbs/homebrew-tap"
FORMULA_FILE="Formula/narbs-ptui.rb"

PUSH=true
VERSION=""
for arg in "$@"; do
  case $arg in
  --no-push) PUSH=false ;;
  -h | --help)
    sed -n '5,11p' "$0" | sed 's/^# \{0,1\}//'
    exit 0
    ;;
  -*)
    echo "Unknown option: $arg" >&2
    exit 1
    ;;
  *) VERSION="$arg" ;;
  esac
done

if [ -z "$VERSION" ]; then
  VERSION=$(grep '^version = ' "$(dirname "$0")/Cargo.toml" | sed 's/version = "\([^"]*\)"/\1/')
  echo "No version given; using ${VERSION} from Cargo.toml"
fi
VERSION="${VERSION#v}"

# Find the tap: ask brew where it lives (differs between Linux and macOS), else the Linux default
TAP_DIR="${PTUI_TAP_DIR:-}"
if [ -z "$TAP_DIR" ] && command -v brew &>/dev/null; then
  TAP_DIR=$(brew --repository "$TAP" 2>/dev/null || true)
fi
if [ -z "$TAP_DIR" ] || [ ! -d "$TAP_DIR" ]; then
  TAP_DIR="$FALLBACK_TAP_DIR"
fi
if [ ! -f "$TAP_DIR/$FORMULA_FILE" ]; then
  echo "Error: ${FORMULA_FILE} not found in tap at ${TAP_DIR}" >&2
  exit 1
fi

cd "$TAP_DIR"

# Only ever commit our own change
if [ -n "$(git status --porcelain)" ]; then
  echo "Error: the tap at ${TAP_DIR} has uncommitted changes:" >&2
  git status --short >&2
  echo "Commit, stash or discard them first." >&2
  exit 1
fi

echo "Pulling the tap..."
git pull --ff-only --quiet

DOWNLOAD_URL="https://github.com/narbs/ptui/archive/refs/tags/v${VERSION}.tar.gz"

if grep -qF "url \"${DOWNLOAD_URL}\"" "$FORMULA_FILE"; then
  echo "The formula is already at v${VERSION}; nothing to do."
  exit 0
fi

TEMP_FILE=$(mktemp)
trap 'rm -f "$TEMP_FILE"' EXIT

echo "Downloading ${DOWNLOAD_URL}..."
if ! curl -fsSL -o "$TEMP_FILE" "$DOWNLOAD_URL"; then
  echo "Error: failed to download the release tarball. Is tag v${VERSION} pushed?" >&2
  exit 1
fi

if command -v sha256sum &>/dev/null; then
  SHA256=$(sha256sum "$TEMP_FILE" | awk '{print $1}')
elif command -v shasum &>/dev/null; then
  SHA256=$(shasum -a 256 "$TEMP_FILE" | awk '{print $1}')
else
  echo "Error: neither sha256sum nor shasum found" >&2
  exit 1
fi
echo "SHA256: ${SHA256}"

# -i.tmp works with both GNU and BSD sed
sed -i.tmp \
  -e "s|url \"https://github.com/narbs/ptui/archive/refs/tags/v.*\.tar\.gz\"|url \"${DOWNLOAD_URL}\"|" \
  -e "s|sha256 \".*\"|sha256 \"${SHA256}\"|" \
  "$FORMULA_FILE"
rm -f "${FORMULA_FILE}.tmp"

if ! grep -qF "url \"${DOWNLOAD_URL}\"" "$FORMULA_FILE" || ! grep -qF "sha256 \"${SHA256}\"" "$FORMULA_FILE"; then
  echo "Error: the formula was not updated as expected; leaving it for inspection:" >&2
  git diff >&2
  exit 1
fi

git diff
git add "$FORMULA_FILE"
git commit --quiet -m "Update ptui to v${VERSION}"
echo "Committed: Update ptui to v${VERSION}"

if [ "$PUSH" = true ]; then
  git push --quiet
  echo "Pushed the tap. Users get v${VERSION} with: brew update && brew upgrade narbs-ptui"
else
  echo "Not pushed (--no-push). To publish: git -C \"${TAP_DIR}\" push"
fi
