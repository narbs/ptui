#!/bin/bash

# Helper script to patch the generated PKGBUILD to use --features fast-jpeg
# Run this after `cargo aur` and before copying to AUR repo

set -e

PKGBUILD_PATH="target/cargo-aur/PKGBUILD"

if [ ! -f "$PKGBUILD_PATH" ]; then
    echo "Error: PKGBUILD not found at $PKGBUILD_PATH"
    echo "Please run 'cargo aur' first"
    exit 1
fi

echo "Patching PKGBUILD to add --features fast-jpeg..."

# Backup original
cp "$PKGBUILD_PATH" "${PKGBUILD_PATH}.backup"

# Patch the build() function to add --features fast-jpeg
sed -i 's/cargo build --release --locked/cargo build --release --locked --features fast-jpeg/' "$PKGBUILD_PATH"

# Also patch the package() function if it has a cargo install command
sed -i 's/cargo install --locked --path/cargo install --locked --features fast-jpeg --path/' "$PKGBUILD_PATH"

echo "✓ PKGBUILD patched successfully"
echo ""
echo "Changes made:"
diff -u "${PKGBUILD_PATH}.backup" "$PKGBUILD_PATH" || true
echo ""
echo "Next steps:"
echo "  1. Review the changes above"
echo "  2. Continue with the release.sh script to copy to AUR repo"
