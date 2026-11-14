#!/usr/bin/env bash

set -euo pipefail

# Update the ptui formula in the narbs/homebrew-tap to a released version through a pull
# request: commit it on a branch, open a PR, wait for the tap's CI to build and test the
# formula on Linux and macOS (check-homebrew-ci.sh), then squash-merge it. A push straight to
# main would not do: the tap's CI only builds formulae a pull request changes. If CI fails,
# the PR is left open, unmerged, and the script exits 1; re-running it picks up that PR.
# The tag must already be on GitHub, since its source tarball is hashed.
#
# Usage: ./update-ptui-homebrew.sh [--no-push|--no-merge] [version]
#   version     defaults to the version in Cargo.toml; a leading 'v' is ignored
#   --no-push   commit the change on a branch in the tap, but push nothing
#   --no-merge  open the PR and wait for CI, but leave merging it to you
# Set PTUI_TAP_DIR to use a tap checked out somewhere else.

TAP="narbs/homebrew-tap"
FORMULA="narbs/tap/narbs-ptui"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
FALLBACK_TAP_DIR="/home/linuxbrew/.linuxbrew/Homebrew/Library/Taps/narbs/homebrew-tap"
FORMULA_FILE="Formula/narbs-ptui.rb"

PUSH=true
MERGE=true
VERSION=""
for arg in "$@"; do
  case $arg in
  --no-push) PUSH=false ;;
  --no-merge) MERGE=false ;;
  -h | --help)
    sed -n '5,17p' "$0" | sed 's/^# \{0,1\}//'
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

if [ "$PUSH" = true ] && ! command -v gh &>/dev/null; then
  echo "Error: gh (GitHub CLI) is needed to open and merge the tap's pull request" >&2
  exit 1
fi

BRANCH="narbs-ptui-${VERSION}"

# Wait for the PR's CI to build and test the formula, then merge it (unless --no-merge)
finish_pr() {
  local pr_url=$1 head_sha=$2
  echo "Pull request: ${pr_url}"
  if ! "$SCRIPT_DIR/check-homebrew-ci.sh" --formula "$FORMULA" "$head_sha"; then
    echo "Error: the tap's CI did not pass; the PR was left open and unmerged: ${pr_url}" >&2
    echo "Push fixes to its branch (${BRANCH}), then re-run this script to check and merge it." >&2
    exit 1
  fi
  if [ "$MERGE" = false ]; then
    echo "Not merged (--no-merge). Merge it with: gh pr merge ${pr_url} --squash --delete-branch"
    exit 0
  fi
  # --match-head-commit: merge only what CI checked, even if the branch moved meanwhile
  gh pr merge "$pr_url" --repo "$TAP" --squash --delete-branch --match-head-commit "$head_sha"
  git checkout --quiet main
  git pull --ff-only --quiet
  git branch -D --quiet "$BRANCH" 2>/dev/null || true
  echo "Merged. Users get v${VERSION} with: brew update && brew upgrade narbs-ptui"
}

# The tap stays on main: brew doctor complains about a tap left on another branch
git checkout --quiet main
echo "Pulling the tap..."
git pull --ff-only --quiet

# Re-run after a failed or timed-out CI: carry on with the PR already open
if [ "$PUSH" = true ]; then
  OPEN_PR=$(gh pr list --repo "$TAP" --head "$BRANCH" --state open --json url,headRefOid \
    -q '.[0] | select(.) | .url + " " + .headRefOid')
  if [ -n "$OPEN_PR" ]; then
    echo "A pull request for v${VERSION} is already open; checking it."
    finish_pr "${OPEN_PR% *}" "${OPEN_PR#* }"
    exit 0
  fi
fi

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

# Run the same style and audit checks the tap's CI runs (`brew test-bot --only-tap-syntax`),
# so a formula CI would reject never gets pushed. brew audit takes a formula name, which
# resolves to brew's own copy of the tap, so it is skipped when PTUI_TAP_DIR points elsewhere.
if command -v brew &>/dev/null; then
  echo "Checking the formula (brew style, brew audit)..."
  CHECK_FAILED=false
  brew style "$FORMULA_FILE" || CHECK_FAILED=true
  if [ "$(cd "$(brew --repository "$TAP" 2>/dev/null)" 2>/dev/null && pwd -P)" = "$(pwd -P)" ]; then
    brew audit --strict --online --formula narbs/tap/narbs-ptui || CHECK_FAILED=true
  else
    echo "Skipping brew audit: ${TAP_DIR} is not brew's copy of ${TAP}"
  fi
  if [ "$CHECK_FAILED" = true ]; then
    git checkout -- "$FORMULA_FILE"
    echo "Error: the formula failed brew's checks above; the tap was left unchanged." >&2
    exit 1
  fi
else
  echo "Warning: brew not found; skipping brew style and brew audit" >&2
fi

# -B: a branch left by an earlier attempt whose PR was closed is started afresh
git checkout --quiet -B "$BRANCH"
git add "$FORMULA_FILE"
git commit --quiet -m "Update ptui to v${VERSION}"
echo "Committed on branch ${BRANCH}: Update ptui to v${VERSION}"

if [ "$PUSH" = false ]; then
  git checkout --quiet main
  echo "Not pushed (--no-push). To publish, run this script again without --no-push."
  exit 0
fi

HEAD_SHA=$(git rev-parse HEAD)
git push --quiet --force -u origin "$BRANCH"
git checkout --quiet main
PR_URL=$(gh pr create --repo "$TAP" --base main --head "$BRANCH" \
  --title "narbs-ptui ${VERSION}" \
  --body "Update narbs-ptui to [v${VERSION}](https://github.com/narbs/ptui/releases/tag/v${VERSION}). Opened by ptui's update-ptui-homebrew.sh, which merges it once CI has built and tested the formula.")
finish_pr "$PR_URL" "$HEAD_SHA"
