#!/bin/bash

# Release script for ptui
# Bumps version, builds, tests, commits, tags, and updates AUR
# Usage: ./release.sh [--dry-run] [--patch|--minor|--major]

set -e # Exit on any error

# Function to show help
show_help() {
  echo "Usage: $0 [--dry-run] [--patch|--minor|--major]"
  echo ""
  echo "Version bump options:"
  echo "  --patch      Bump patch version (default: x.y.z -> x.y.z+1)"
  echo "  --minor      Bump minor version (x.y.z -> x.y+1.0)"
  echo "  --major      Bump major version (x.y.z -> x+1.0.0)"
  echo ""
  echo "Other options:"
  echo "  --dry-run    Perform all steps except committing and pushing changes"
  echo "  -h, --help   Show this help message"
  echo ""
  echo "Examples:"
  echo "  $0                    # Bump patch version and release"
  echo "  $0 --minor           # Bump minor version and release"
  echo "  $0 --dry-run --major # Test major version bump without releasing"
}

# Show help if no arguments provided
if [ $# -eq 0 ]; then
  show_help
  exit 0
fi

# Parse command line arguments
DRY_RUN=false
VERSION_BUMP="patch"
for arg in "$@"; do
  case $arg in
  --dry-run)
    DRY_RUN=true
    shift
    ;;
  --patch)
    VERSION_BUMP="patch"
    shift
    ;;
  --minor)
    VERSION_BUMP="minor"
    shift
    ;;
  --major)
    VERSION_BUMP="major"
    shift
    ;;
  -h | --help)
    show_help
    exit 0
    ;;
  *)
    echo "Unknown option: $arg"
    echo "Use --help for usage information"
    exit 1
    ;;
  esac
done

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo_info() {
  echo -e "${GREEN}[INFO]${NC} $1"
}

echo_warn() {
  echo -e "${YELLOW}[WARN]${NC} $1"
}

echo_error() {
  echo -e "${RED}[ERROR]${NC} $1"
}

echo_dry() {
  echo -e "${BLUE}[DRY-RUN]${NC} $1"
}

# Function to bump version in Cargo.toml
bump_version() {
  local current_version=$(grep '^version = ' Cargo.toml | sed 's/version = "\([^"]*\)"/\1/')
  echo_info "Current version: $current_version"

  # Extract major, minor, patch
  IFS='.' read -r major minor patch <<<"$current_version"

  # Determine new version based on bump type
  case $VERSION_BUMP in
  "major")
    new_major=$((major + 1))
    new_version="${new_major}.0.0"
    echo_info "Bumping major version: $current_version -> $new_version"
    ;;
  "minor")
    new_minor=$((minor + 1))
    new_version="${major}.${new_minor}.0"
    echo_info "Bumping minor version: $current_version -> $new_version"
    ;;
  "patch")
    new_patch=$((patch + 1))
    new_version="${major}.${minor}.${new_patch}"
    echo_info "Bumping patch version: $current_version -> $new_version"
    ;;
  *)
    echo_error "Invalid version bump type: $VERSION_BUMP"
    exit 1
    ;;
  esac

  if [ "$DRY_RUN" = true ]; then
    echo_dry "Will update Cargo.toml version from $current_version to $new_version"
    # Create backup for dry run restoration
    cp Cargo.toml Cargo.toml.backup
  else
    echo_info "Updating Cargo.toml..."
  fi

  # Update Cargo.toml (for both dry run and real run to test build)
  sed -i "s/version = \"$current_version\"/version = \"$new_version\"/" Cargo.toml

  # Return new version for use in other functions
  echo "$new_version"
}

# Function to build and test
build_and_test() {
  echo_info "Building project..."
  if ! cargo build --release; then
    echo_error "Build failed!"
    exit 1
  fi

  echo_info "Running tests..."
  if ! cargo test; then
    echo_error "Tests failed!"
    exit 1
  fi

  echo_info "Build and tests successful!"
}

# Function to commit and tag
commit_and_tag() {
  local version=$1
  local commit_msg="Bump release to v$version"

  if [ "$DRY_RUN" = true ]; then
    echo_dry "Would commit changes with message: '$commit_msg'"
    echo_dry "Would create tag: v$version"
    echo_dry "Would push tag: v$version"
  fi

  if [ "$DRY_RUN" = false ]; then
    echo_info "Committing changes..."
    git add Cargo.toml Cargo.lock
    git commit -m "$commit_msg"

    echo_info "Creating tag v$version..."
    git tag "v$version"

    echo_info "Pushing tags..."
    git push origin "v$version"
  fi
}

# Function to build AUR package
build_aur_package() {
  local version=$1

  echo_info "Building AUR package..."
  if ! cargo aur; then
    echo_error "cargo aur failed!"
    exit 1
  fi

  # Check if AUR repo exists
  if [ ! -d "../ptui-aur" ]; then
    echo_error "AUR repository not found at ../ptui-aur"
    exit 1
  fi

  if [ "$DRY_RUN" = true ]; then
    echo_dry "Dry, run, code would update AUR repository at ../ptui-aur"
    echo_dry "Will copy PKGBUILD from target/cargo-aur/"
    echo_dry "Will regenerate .SRCINFO"
    echo_dry "Would commit AUR changes with message: 'Update to v$version'"
    echo_dry "Would push AUR changes"
  fi

  echo_info "Updating AUR repository..."
  cd ../ptui-aur

  # Copy PKGBUILD
  if [ ! -f "../ptui/target/cargo-aur/PKGBUILD" ]; then
    echo_error "PKGBUILD not found at ../ptui/target/cargo-aur/PKGBUILD"
    exit 1
  fi

  cp ../ptui/target/cargo-aur/PKGBUILD .

  echo_info "Regenerating .SRCINFO..."
  makepkg --printsrcinfo >.SRCINFO

  echo_info "Modifying .SRCINFO to change ptui-bin to ptui..."
  sed -i 's/pkgbase = ptui-bin/pkgbase = ptui/g' .SRCINFO

  if [ "$DRY_RUN" = false ]; then
    echo_info "Committing AUR changes..."
    git add PKGBUILD .SRCINFO
    git commit -m "Update to v$version"

    echo_info "Pushing AUR changes..."
    git push
  fi

  cd ../ptui
}

# Main execution
main() {

  if [ "$DRY_RUN" = true ]; then
    echo_dry "Running in DRY RUN mode - no commits or pushes will be made"
    echo_dry "Files will be modified and left in-place for inspection"
    echo ""
  fi

  echo_info "Starting release process..."

  # Check if makepkg is available (required for AUR package generation)
  if ! command -v makepkg >/dev/null 2>&1; then
    echo_error "makepkg is not available. This script must be run on a system that supports makepkg (e.g., Arch Linux)."
    echo_error "makepkg is required for AUR package generation."
    exit 1
  fi

  # Check if we're in a git repository
  if ! git rev-parse --git-dir >/dev/null 2>&1; then
    echo_error "Not in a git repository!"
    exit 1
  fi

  # Check if working directory is clean (skip for dry run)
  if [ "$DRY_RUN" = false ] && [ -n "$(git status --porcelain)" ]; then
    echo_error "Working directory is not clean. Please commit or stash changes first."
    exit 1
  fi

  # Bump version
  new_version=$(bump_version)

  # Build and test
  build_and_test

  # Commit and tag
  commit_and_tag "$new_version"

  # Build AUR package
  build_aur_package "$new_version"

  if [ "$DRY_RUN" = true ]; then
    echo ""
    echo_dry "Dry run completed! Here's what would happen in a real release:"
    echo_dry "- Version bumped from $(grep '^version = ' Cargo.toml.backup | sed 's/version = "\([^"]*\)"/\1/') to $new_version"
    echo_dry "- Project built and tested successfully"
    echo_dry "- Changes committed with tag v$new_version"
    echo_dry "- AUR package updated and pushed"
    echo_dry ""
    echo_dry "To perform the actual release, run: ./release.sh"
    rm Cargo.toml.backup
  else
    echo_info "Release v$new_version completed successfully!"
    echo_info "Don't forget to push the main branch: git push origin main"
  fi
}

# Run main function
main "$@"
