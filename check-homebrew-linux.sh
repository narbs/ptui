#!/usr/bin/env bash

set -euo pipefail

# Run the Homebrew tap's Linux CI checks locally, in the same container the tap's CI uses
# (ghcr.io/homebrew/brew:main): brew test-bot's setup (brew doctor), tap syntax (style, audit),
# and formula steps, the last of which builds narbs-ptui from source, bottles it, reinstalls
# from the bottle and runs its test.
#
# Release pushes to the tap never build the formula, and a local brew install is no stand-in
# for a clean Homebrew: a missing pkgconf build dependency went unnoticed that way. Run this
# before pushing a change to how the formula builds. It takes around 10 minutes.
#
# Usage: ./check-homebrew-linux.sh [--syntax-only]
#   --syntax-only  skip building the formula (setup and syntax checks only, ~1 minute)
# Checks the tap as it is on disk, uncommitted changes included, without modifying it.
# Set PTUI_TAP_DIR to use a tap checked out somewhere else.
# Needs Docker, usable without sudo (be in the docker group).

TAP="narbs/homebrew-tap"
FORMULA="narbs/tap/narbs-ptui"
IMAGE="ghcr.io/homebrew/brew:main"
FALLBACK_TAP_DIR="/home/linuxbrew/.linuxbrew/Homebrew/Library/Taps/narbs/homebrew-tap"

BUILD=true
for arg in "$@"; do
  case $arg in
  --syntax-only) BUILD=false ;;
  -h | --help)
    sed -n '5,19p' "$0" | sed 's/^# \{0,1\}//'
    exit 0
    ;;
  *)
    echo "Unknown option: $arg" >&2
    exit 1
    ;;
  esac
done

if ! command -v docker &>/dev/null; then
  echo "Error: docker is not installed" >&2
  exit 1
fi
if ! docker info &>/dev/null; then
  echo "Error: cannot reach the Docker daemon. Is it running, and are you in the docker group?" >&2
  echo "A group added since you logged in only applies to new logins; 'newgrp docker' starts a" >&2
  echo "shell with it." >&2
  exit 1
fi

TAP_DIR="${PTUI_TAP_DIR:-}"
if [ -z "$TAP_DIR" ] && command -v brew &>/dev/null; then
  TAP_DIR=$(brew --repository "$TAP" 2>/dev/null || true)
fi
[ -n "$TAP_DIR" ] && [ -d "$TAP_DIR" ] || TAP_DIR="$FALLBACK_TAP_DIR"
if [ ! -f "$TAP_DIR/Formula/narbs-ptui.rb" ]; then
  echo "Error: no tap with Formula/narbs-ptui.rb at ${TAP_DIR}" >&2
  exit 1
fi

STEPS=("--only-setup" "--only-tap-syntax")
[ "$BUILD" = true ] && STEPS+=("--only-formulae ${FORMULA}")

LOG=$(mktemp -t ptui-homebrew-linux.XXXXXX.log)
echo "Tap: ${TAP_DIR} ($(git -C "$TAP_DIR" rev-parse --short HEAD)$(
  [ -n "$(git -C "$TAP_DIR" status --porcelain)" ] && echo ", with uncommitted changes"
))"
echo "Pulling ${IMAGE}..."
docker pull -q "$IMAGE" >/dev/null
echo "Running brew test-bot in the container; full log: ${LOG}"

# The tap is mounted read-only and copied inside, so test-bot can do as it likes with it.
# test-bot's exit status cannot be trusted outside GitHub Actions (it exits 0 after a failed
# build), so each step's outcome is read from its output.
docker run --rm --privileged \
  -v "$TAP_DIR":/tap:ro \
  -e STEPS="$(printf '%s\n' "${STEPS[@]}")" \
  "$IMAGE" bash -c '
    export HOMEBREW_NO_ANALYTICS=1 HOMEBREW_NO_ENV_HINTS=1 HOMEBREW_NO_AUTO_UPDATE=1
    tapdir="$(brew --repository)/Library/Taps/narbs/homebrew-tap"
    mkdir -p "$(dirname "$tapdir")"
    cp -a /tap "$tapdir"
    git config --global --add safe.directory "*"
    cd "$tapdir"
    # brew doctor fails a tap that is not on its default branch, so a feature branch is
    # checked here as main; -B keeps the commit and any uncommitted changes as they are.
    git checkout -q -B main
    # ...and compares against origin/HEAD, which in a clone of a local checkout names
    # whatever branch that checkout was on.
    git show-ref -q refs/remotes/origin/main &&
      git symbolic-ref refs/remotes/origin/HEAD refs/remotes/origin/main
    brew --version | head -1
    while IFS= read -r step; do
      [ -n "$step" ] || continue
      echo "######## STEP brew test-bot $step"
      brew test-bot --tap=narbs/tap $step 2>&1 | tee /tmp/step.log
      if grep -q "^==> FAILED\|did not build\|^Error:" /tmp/step.log; then
        echo "######## FAILED brew test-bot $step"
      else
        echo "######## PASSED brew test-bot $step"
      fi
    done <<<"$STEPS"
  ' >"$LOG" 2>&1 || true

FAILED=false
for step in "${STEPS[@]}"; do
  if grep -qxF "######## PASSED brew test-bot ${step}" "$LOG"; then
    echo "  passed: brew test-bot ${step}"
  else
    echo "  FAILED: brew test-bot ${step}"
    FAILED=true
  fi
done

if [ "$FAILED" = true ]; then
  echo "Error: Homebrew Linux checks failed. Errors from the log:" >&2
  grep -E "^Error:|^error(\[|:)|panicked|did not build" "$LOG" | head -20 >&2 || true
  echo "Full log: ${LOG}" >&2
  exit 1
fi
echo "Homebrew Linux checks passed."
rm -f "$LOG"
