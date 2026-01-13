#!/bin/bash

set -e

echo "Building PTUI for release with fast-jpeg feature..."
cargo build --release --features fast-jpeg

VERSION=$(grep '^version = ' Cargo.toml | sed 's/version = "\(.*\)"/\1/')

if [ -z "$VERSION" ]; then
    echo "Error: Could not extract version from Cargo.toml"
    exit 1
fi

TARBALL="ptui-${VERSION}-mac-x86_64.tar.gz"

echo "Creating tarball: $TARBALL"
tar -czf "$TARBALL" -C target/release ptui

echo "Release tarball created: $TARBALL"

