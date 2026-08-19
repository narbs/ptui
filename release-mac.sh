#!/bin/bash

set -e

REPO="narbs/ptui"
UPLOAD=true

# Colors for output, matching release.sh
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

echo_info() {
  echo -e "${GREEN}[INFO]${NC} $1" >&2
}

echo_warn() {
  echo -e "${YELLOW}[WARN]${NC} $1" >&2
}

echo_error() {
  echo -e "${RED}[ERROR]${NC} $1" >&2
}

usage() {
  cat <<EOF
Usage: ./release-mac.sh [--no-upload]

Builds the macOS binary with the fast-jpeg feature, packs it into a tarball, and uploads it
to the GitHub release for the version in Cargo.toml.

  --no-upload   Build and pack only, leaving the tarball for you to upload yourself.

The upload needs the tag to have been pushed already, which ./release.sh does. If the
release does not exist yet, the tarball is left in place and the command to run later is
printed.
EOF
}

while [[ $# -gt 0 ]]; do
  case $1 in
  --no-upload)
    UPLOAD=false
    shift
    ;;
  -h | --help)
    usage
    exit 0
    ;;
  *)
    echo_error "Unknown option: $1"
    usage
    exit 1
    ;;
  esac
done

# Refuse to build a "mac" tarball anywhere else. Without this the script would happily pack
# a Linux binary under a macOS name and, now that the upload is automatic, publish it.
if [ "$(uname -s)" != "Darwin" ]; then
  echo_error "This script builds the macOS binary and must run on a Mac (found $(uname -s))."
  exit 1
fi

echo_info "Building PTUI for release with fast-jpeg feature..."
cargo build --release --features fast-jpeg

VERSION=$(grep '^version = ' Cargo.toml | sed 's/version = "\(.*\)"/\1/')

if [ -z "$VERSION" ]; then
  echo_error "Could not extract version from Cargo.toml"
  exit 1
fi

# Name the tarball after the architecture actually built, so an Apple Silicon build is not
# published as x86_64.
ARCH=$(uname -m)
TAG="v${VERSION}"
TARBALL="ptui-${VERSION}-mac-${ARCH}.tar.gz"

echo_info "Creating tarball: $TARBALL"
tar -czf "$TARBALL" -C target/release ptui

echo_info "Release tarball created: $TARBALL"

if [ "$UPLOAD" = false ]; then
  echo_info "Skipping upload as requested. Upload it with:"
  echo_info "  gh release upload $TAG $TARBALL --repo $REPO"
  exit 0
fi

if ! command -v gh >/dev/null 2>&1; then
  echo_warn "gh is not installed, so $TARBALL was not uploaded."
  echo_warn "Install the GitHub CLI, then run:"
  echo_warn "  gh release upload $TAG $TARBALL --repo $REPO"
  exit 0
fi

if ! gh auth status >/dev/null 2>&1; then
  echo_warn "gh is not authenticated, so $TARBALL was not uploaded."
  echo_warn "Run 'gh auth login', then:"
  echo_warn "  gh release upload $TAG $TARBALL --repo $REPO"
  exit 0
fi

# The tag has to be on GitHub already, which ./release.sh does. Uploading to a release that
# does not exist would fail, so say what to do rather than leaving an error behind.
if ! gh release view "$TAG" --repo "$REPO" >/dev/null 2>&1; then
  echo_warn "No GitHub release $TAG exists yet, so $TARBALL was not uploaded."
  echo_warn "Cut the release first with ./release.sh, then run:"
  echo_warn "  gh release upload $TAG $TARBALL --repo $REPO"
  exit 0
fi

echo_info "Uploading $TARBALL to GitHub release $TAG..."
# --clobber so re-running after a rebuild replaces the asset instead of failing.
if gh release upload "$TAG" "$TARBALL" --repo "$REPO" --clobber; then
  echo_info "Uploaded $TARBALL to $TAG."
  echo_info "https://github.com/$REPO/releases/tag/$TAG"
else
  echo_error "Upload failed. The tarball is still here; retry with:"
  echo_error "  gh release upload $TAG $TARBALL --repo $REPO --clobber"
  exit 1
fi
