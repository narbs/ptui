#!/usr/bin/env bash

set -euo pipefail

# Wait for the Homebrew tap's `brew test-bot` CI run on a commit and report whether it passed.
# The run checks the formula against the latest Homebrew (brew doctor, style, audit), which
# no local check can stand in for: it once failed unnoticed from 2.3.0 to 2.7.6.
#
# Usage: ./check-homebrew-ci.sh [--formula NAME] [commit]
#   commit          tap commit to check; defaults to the tap's current HEAD
#   --formula NAME  also require every job to have built and tested NAME (for example
#                   narbs/tap/narbs-ptui), with no step failed, ignored or skipped. test-bot
#                   only builds formulae a pull request changes, and its run goes green
#                   without building anything, or with a failed step ignored.
# Exits 0 when the run passed, 1 when it failed, was not found, or did not finish within
# PTUI_CI_TIMEOUT seconds (default 3600).
# Set PTUI_TAP_DIR to use a tap checked out somewhere else.

REPO="narbs/homebrew-tap"
WORKFLOW="tests.yml"
FALLBACK_TAP_DIR="/home/linuxbrew/.linuxbrew/Homebrew/Library/Taps/narbs/homebrew-tap"
# A push triggers the run within seconds, but GitHub can take a while to list it
FIND_ATTEMPTS=20
FIND_INTERVAL=6

COMMIT=""
FORMULA=""
while [ $# -gt 0 ]; do
  case $1 in
  --formula)
    FORMULA="${2:?--formula needs a formula name}"
    shift
    ;;
  -h | --help)
    sed -n '5,17p' "$0" | sed 's/^# \{0,1\}//'
    exit 0
    ;;
  -*)
    echo "Unknown option: $1" >&2
    exit 1
    ;;
  *) COMMIT="$1" ;;
  esac
  shift
done

if ! command -v gh &>/dev/null; then
  echo "Error: gh (GitHub CLI) is needed to check the tap's CI" >&2
  exit 1
fi

TAP_DIR="${PTUI_TAP_DIR:-}"
if [ -z "$TAP_DIR" ] && command -v brew &>/dev/null; then
  TAP_DIR=$(brew --repository "$REPO" 2>/dev/null || true)
fi
[ -n "$TAP_DIR" ] && [ -d "$TAP_DIR" ] || TAP_DIR="$FALLBACK_TAP_DIR"
# GitHub matches runs on the full SHA only, so expand a short one (or a branch name)
COMMIT=$(git -C "$TAP_DIR" rev-parse --verify --quiet "${COMMIT:-HEAD}^{commit}" 2>/dev/null || echo "$COMMIT")
if [ -z "$COMMIT" ]; then
  echo "Error: cannot read the tap's HEAD at ${TAP_DIR}; pass a commit" >&2
  exit 1
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

if [ "$CONCLUSION" = "success" ] && [ -z "$FORMULA" ]; then
  echo "Homebrew tap CI passed."
  exit 0
fi

if [ "$CONCLUSION" = "success" ]; then
  # Read each job's log for proof that the formula was built and tested
  LOG=$(mktemp)
  trap 'rm -f "$LOG"' EXIT
  for _ in 1 2 3; do
    gh run view "$RUN_ID" --repo "$REPO" --log 2>/dev/null | sed 's/\x1b\[[0-9;]*m//g' >"$LOG" &&
      [ -s "$LOG" ] && break
    sleep 10
  done
  if [ ! -s "$LOG" ]; then
    echo "Error: CI passed but its log could not be read to confirm ${FORMULA} was built: ${RUN_URL}" >&2
    exit 1
  fi
  UNTESTED=""
  while IFS= read -r job; do
    JOB_LOG=$(grep -F "${job}"$'\t' "$LOG")
    if ! grep -qF "brew install --verbose --formula --build-bottle ${FORMULA}" <<<"$JOB_LOG" ||
      ! grep -qF "brew test --verbose ${FORMULA}" <<<"$JOB_LOG" ||
      grep -qE "FAILED|SKIPPED|failed steps? ignored" <<<"$JOB_LOG"; then
      UNTESTED+="  ${job}"$'\n'
    fi
  done < <(gh run view "$RUN_ID" --repo "$REPO" --json jobs -q '.jobs[].name')
  if [ -n "$UNTESTED" ]; then
    echo "Error: CI went green, but these jobs did not cleanly build and test ${FORMULA}:" >&2
    printf '%s' "$UNTESTED" >&2
    echo "See ${RUN_URL}" >&2
    exit 1
  fi
  echo "Homebrew tap CI passed, and built and tested ${FORMULA} in every job."
  exit 0
fi

echo "Error: Homebrew tap CI ${CONCLUSION}: ${RUN_URL}" >&2
echo "Failed steps:" >&2
gh run view "$RUN_ID" --repo "$REPO" --json jobs \
  -q '.jobs[] | select(.conclusion == "failure") | "  \(.name): " + ([.steps[] | select(.conclusion == "failure") | .name] | join(", "))' >&2 || true
exit 1
