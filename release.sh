#!/bin/bash

# Release script for ptui
# Bumps version, builds, tests, commits, tags, publishes the GitHub release
# (with the Linux binary), and updates the AUR.
# Usage: ./release.sh [--dry-run] [--patch|--minor|--major]
#
# Order of operations (real run):
#   1. Bump version in Cargo.toml
#   2. Build (--features fast-jpeg) and test
#   3. Build release artifacts (cargo aur -> target/cargo-aur/)
#   4. Commit + tag, push the branch and the tag
#   5. Publish the GitHub release and upload the Linux binary
#   6. Update and push the AUR repo (only after the binary exists, so source= resolves)
#
# The macOS binary is built separately on a Mac; upload it to the same release afterwards.

set -e # Exit on any error

REPO="narbs/ptui"

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
  echo "  --dry-run    Perform all steps except committing, tagging and pushing"
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
  echo -e "${GREEN}[INFO]${NC} $1" >&2
}

echo_warn() {
  echo -e "${YELLOW}[WARN]${NC} $1" >&2
}

echo_error() {
  echo -e "${RED}[ERROR]${NC} $1" >&2
}

echo_dry() {
  echo -e "${BLUE}[DRY-RUN]${NC} $1" >&2
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
    # Back up so the dry run can restore these afterwards (cargo build rewrites Cargo.lock)
    cp Cargo.toml Cargo.toml.backup
    [ -f Cargo.lock ] && cp Cargo.lock Cargo.lock.backup
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
  echo_info "Building project with fast-jpeg feature..."
  if ! cargo build --release --features fast-jpeg; then
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

# Function to build the AUR/release artifacts (binary tarball + PKGBUILD)
build_release_artifacts() {
  echo_info "Building release artifacts (cargo aur)..."
  if ! cargo aur; then
    echo_error "cargo aur failed!"
    exit 1
  fi

  echo_info "Patching PKGBUILD to add --features fast-jpeg..."
  if ! ./patch-aur-pkgbuild.sh; then
    echo_error "Failed to patch PKGBUILD!"
    exit 1
  fi
}

# Function to commit and tag, then push the branch and the tag
commit_and_tag() {
  local version=$1
  local commit_msg="Bump release to v$version"
  local branch
  branch=$(git rev-parse --abbrev-ref HEAD)

  if [ "$DRY_RUN" = true ]; then
    echo_dry "Would commit changes with message: '$commit_msg'"
    echo_dry "Would create tag: v$version"
    echo_dry "Would push branch '$branch' and tag 'v$version'"
    return
  fi

  echo_info "Committing changes..."
  git add Cargo.toml Cargo.lock
  git commit -m "$commit_msg"

  echo_info "Creating tag v$version..."
  git tag "v$version"

  echo_info "Pushing branch '$branch' and tag 'v$version'..."
  git push origin "$branch"
  git push origin "v$version"
}

# Best-effort: extract the changelog paragraph for this version from CHANGELOG.md
release_notes() {
  local version=$1
  [ -f CHANGELOG.md ] || return 1
  awk -v ver="$version" '
    index($0, "PTUI " ver " released") { capture = 1 }
    capture {
      if ($0 ~ /^[[:space:]]*$/) exit
      print
    }
  ' CHANGELOG.md
}

# Function to publish the GitHub release and upload the Linux binary.
# Runs BEFORE the AUR push so the PKGBUILD source= URL resolves immediately.
publish_github_release() {
  local version=$1
  local tag="v$version"
  local tarball="target/cargo-aur/ptui-${version}-x86_64.tar.gz"

  if [ ! -f "$tarball" ]; then
    echo_error "Release tarball not found: $tarball"
    exit 1
  fi

  if [ "$DRY_RUN" = true ]; then
    echo_dry "Would create/update GitHub release $tag on $REPO"
    echo_dry "Would upload Linux asset: $tarball"
    return
  fi

  # Build release notes (best effort from CHANGELOG.md, else a simple title)
  local notes_file
  notes_file=$(mktemp)
  if ! release_notes "$version" >"$notes_file" || [ ! -s "$notes_file" ]; then
    echo "PTUI $tag" >"$notes_file"
  fi

  if gh release view "$tag" --repo "$REPO" >/dev/null 2>&1; then
    echo_info "GitHub release $tag already exists; uploading Linux binary (clobber)..."
    gh release upload "$tag" "$tarball" --repo "$REPO" --clobber
  else
    echo_info "Creating GitHub release $tag and uploading Linux binary..."
    gh release create "$tag" "$tarball" --repo "$REPO" --title "PTUI $tag" --notes-file "$notes_file"
  fi

  rm -f "$notes_file"
  echo_info "GitHub release $tag published with the Linux binary."
}

# Function to update the AUR repo (copy PKGBUILD, regenerate .SRCINFO, push)
update_aur_repo() {
  local version=$1

  # Check if AUR repo exists
  if [ ! -d "../ptui-aur" ]; then
    echo_error "AUR repository not found at ../ptui-aur"
    exit 1
  fi

  if [ ! -f "target/cargo-aur/PKGBUILD" ]; then
    echo_error "PKGBUILD not found at target/cargo-aur/PKGBUILD"
    exit 1
  fi

  if [ "$DRY_RUN" = true ]; then
    echo_dry "Would copy PKGBUILD from target/cargo-aur/ to ../ptui-aur"
    echo_dry "Would regenerate .SRCINFO and rewrite pkgbase to 'ptui'"
    echo_dry "Would commit AUR changes with message: 'Update to v$version' and push"
    return
  fi

  echo_info "Updating AUR repository..."
  cp target/cargo-aur/PKGBUILD ../ptui-aur/PKGBUILD

  (
    cd ../ptui-aur

    echo_info "Regenerating .SRCINFO..."
    makepkg --printsrcinfo >.SRCINFO

    echo_info "Modifying .SRCINFO to change ptui-bin to ptui..."
    sed -i 's/pkgbase = ptui-bin/pkgbase = ptui/g' .SRCINFO

    echo_info "Committing AUR changes..."
    git add PKGBUILD .SRCINFO
    git commit -m "Update to v$version"

    echo_info "Pushing AUR changes..."
    git push
  )
}

# Main execution
main() {

  if [ "$DRY_RUN" = true ]; then
    echo_dry "Running in DRY RUN mode - no commits, tags or pushes will be made"
    echo_dry "Cargo.toml is bumped to test the build, then restored at the end"
    echo ""
  fi

  echo_info "Starting release process..."

  # Check if makepkg is available (required for AUR package generation)
  if ! command -v makepkg >/dev/null 2>&1; then
    echo_error "makepkg is not available. This script must be run on a system that supports makepkg (e.g., Arch Linux)."
    echo_error "makepkg is required for AUR package generation."
    exit 1
  fi

  # Check if gh is available (required to publish the GitHub release + binary)
  if ! command -v gh >/dev/null 2>&1; then
    echo_error "gh (GitHub CLI) is not available. It is required to publish the GitHub release and upload the binary."
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

  # 1. Bump version
  new_version=$(bump_version)

  # 2. Build and test
  build_and_test

  # 3. Build release artifacts (binary tarball + PKGBUILD)
  build_release_artifacts

  # 4. Commit, tag, push branch + tag
  commit_and_tag "$new_version"

  # 5. Publish the GitHub release with the Linux binary (before the AUR push)
  publish_github_release "$new_version"

  # 6. Update and push the AUR repo (now that the binary exists)
  update_aur_repo "$new_version"

  if [ "$DRY_RUN" = true ]; then
    echo ""
    echo_dry "Dry run completed! Here's what would happen in a real release:"
    echo_dry "- Version bumped from $(grep '^version = ' Cargo.toml.backup | sed 's/version = "\([^"]*\)"/\1/') to $new_version"
    echo_dry "- Project built and tested successfully"
    echo_dry "- Committed, tagged v$new_version, pushed branch + tag"
    echo_dry "- GitHub release v$new_version created with the Linux binary"
    echo_dry "- AUR package updated and pushed"
    echo_dry ""
    echo_dry "To perform the actual release, run: ./release.sh --patch|--minor|--major"
    # Restore Cargo.toml/Cargo.lock so a dry run leaves the working tree clean
    mv Cargo.toml.backup Cargo.toml
    [ -f Cargo.lock.backup ] && mv Cargo.lock.backup Cargo.lock
  else
    echo_info "Release v$new_version completed successfully!"
    echo_info "Pushed: branch, tag v$new_version, GitHub release (Linux binary), and AUR."
    echo_warn "macOS binary is built separately. On a Mac, build and upload it:"
    echo_warn "  gh release upload v$new_version ptui-$new_version-mac-x86_64.tar.gz --repo $REPO"
  fi
}

# Run main function
main "$@"
