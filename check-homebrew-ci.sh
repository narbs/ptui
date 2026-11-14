#!/usr/bin/env bash

set -euo pipefail

# Wait for the Homebrew tap's `brew test-bot` CI run on a commit and report whether it passed.
# The run checks the formula against the latest Homebrew (brew doctor, style, audit), which
# no local check can stand in for: it once failed unnoticed from 2.3.0 to 2.7.6.
#
# Usage: ./check-homebrew-ci.sh [commit]
#   commit   tap commit to check; defaults to the tap's current HEAD
# Exits 0 when the run passed, 1 when it failed, was not found, or did not finish within
# PTUI_CI_TIMEOUT seconds (default 3600).
# Set PTUI_TAP_DIR to use a tap checked out somewhere else.

REPO="narbs/homebrew-tap"
WORKFLOW="tests.yml"
FALLBACK_TAP_DIR="/home/linuxbrew/.linuxbrew/Homebrew/Library/Taps/narbs/homebrew-tap"
# A push triggers the run within seconds, but GitHub can take a while to list it
FIND_ATTEMPTS=20
FIND_INTERVAL=6

case "${1:-}" in
-h | --help)
  sed -n '5,13p' "$0" | sed 's/^# \{0,1\}//'
  exit 0
  ;;
esac

if ! command -v gh &>/dev/null; then
  echo "Error: gh (GitHub CLI) is needed to check the tap's CI" >&2
  exit 1
fi

COMMIT="${1:-}"
if [ -z "$COMMIT" ]; then
  TAP_DIR="${PTUI_TAP_DIR:-}"
  if [ -z "$TAP_DIR" ] && command -v brew &>/dev/null; then
    TAP_DIR=$(brew --repository "$REPO" 2>/dev/null || true)
  fi
  [ -n "$TAP_DIR" ] && [ -d "$TAP_DIR" ] || TAP_DIR="$FALLBACK_TAP_DIR"
  COMMIT=$(git -C "$TAP_DIR" rev-parse HEAD)
fi

echo "Waiting for ${REPO} CI (${WORKFLOW}) on ${COMMIT:0:7}..."
RUN_ID=""
for _ in $(seq "$FIND_ATTEMPTS"); do
  RUN_ID=$(gh run list --repo "$REPO" --workflow "$WORKFLOW" --commit "$COMMIT" \
    --limit 1 --json databaseId -q '.[0].databaseId // empty' 2>/dev/null || true)
  [ -n "$RUN_ID" ] && break
  sleep "$FIND_INTERVAL"
done

if [ -z "$RUN_ID" ]; then
  echo "Error: no ${WORKFLOW} run found for ${COMMIT:0:7}. Was the commit pushed?" >&2
  exit 1
fi

RUN_URL="https://github.com/${REPO}/actions/runs/${RUN_ID}"
echo "Run: ${RUN_URL}"

# Poll rather than `gh run watch`, which proved unreliable without a terminal attached.
# macOS jobs can sit in GitHub's queue for a while, hence the generous limit.
DEADLINE=$((SECONDS + ${PTUI_CI_TIMEOUT:-3600}))
CONCLUSION=""
while [ "$SECONDS" -lt "$DEADLINE" ]; do
  STATE=$(gh run view "$RUN_ID" --repo "$REPO" --json status,conclusion \
    -q '.status + " " + .conclusion' 2>/dev/null || true)
  if [ "${STATE%% *}" = "completed" ]; then
    CONCLUSION="${STATE#* }"
    break
  fi
  sleep 30
done

if [ -z "$CONCLUSION" ]; then
  echo "Error: gave up waiting for ${RUN_URL}; check it again later with $0 ${COMMIT}" >&2
  exit 1
fi

if [ "$CONCLUSION" = "success" ]; then
  echo "Homebrew tap CI passed."
  exit 0
fi

echo "Error: Homebrew tap CI ${CONCLUSION}: ${RUN_URL}" >&2
echo "Failed steps:" >&2
gh run view "$RUN_ID" --repo "$REPO" --json jobs \
  -q '.jobs[] | select(.conclusion == "failure") | "  \(.name): " + ([.steps[] | select(.conclusion == "failure") | .name] | join(", "))' >&2 || true
exit 1
