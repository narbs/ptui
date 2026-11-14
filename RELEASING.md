PTUI - Picture TUI - RELEASING
==============================

How to cut a release. The process lives in the scripts in this directory; this file explains
what they do, in what order, and what has to be true before you start.

Prerequisites
-------------

- **Arch Linux with `makepkg`** - `release.sh` refuses to run without it, because it generates
  the AUR package.
- **`cargo aur`** installed (`cargo install cargo-aur`).
- **The AUR repository checked out at `../ptui-aur`** - a sibling of this directory.
- **`nasm`** - build dependency of turbojpeg, used by the `fast-jpeg` feature.
- **A clean working directory** - `release.sh` checks `git status --porcelain`, which also
  counts *untracked* files. Commit, ignore, or `git stash -u` anything left over first.
- **The Homebrew tap** (`brew tap narbs/homebrew-tap`, on Linux at
  `/home/linuxbrew/.linuxbrew/Homebrew/Library/Taps/narbs/homebrew-tap`) with no uncommitted
  changes, and push access to it. `gh` must be logged in with access to open and merge its
  pull requests.

Before you start
----------------

1. Merge the work into `main` - releases are cut from `main`.
2. Add a `CHANGELOG.md` entry for the new version. `release.sh` does *not* do this; its commit
   contains only `Cargo.toml` and `Cargo.lock`, so the changelog needs to be committed
   beforehand to be included in the tag.
3. Update `README.md` if controls or configuration changed.

Step 1 - Release, tag and publish
---------------------------------

    ./release.sh --minor     # or --patch / --major

Note that `./release.sh` with no arguments prints help and exits - the bump type is required.

The script:

1. Bumps the version in `Cargo.toml` (patch/minor/major).
2. Builds with `cargo build --release --features fast-jpeg` and runs `cargo test`. The release
   aborts if either fails.
3. Runs `cargo aur`, then `patch-aur-pkgbuild.sh`, which adds `--features fast-jpeg`, sets
   `pkgbase=ptui` (the AUR repository's name) and installs the license under
   `/usr/share/licenses/ptui`. The tarball also carries README.md, NEWS.md, CHANGELOG.md and
   `docs/example.config.ptui.json`, installed under `/usr/share/doc/ptui/` - the list is
   `files` in `[package.metadata.aur]` in `Cargo.toml`.
4. Commits `Cargo.toml` and `Cargo.lock` as `Bump release to vX.Y.Z`, creates tag `vX.Y.Z`,
   and pushes both the branch and the tag.
5. Creates the GitHub release and uploads the Linux tarball, with notes taken from the
   CHANGELOG.md entry.
6. Copies the PKGBUILD into `../ptui-aur`, regenerates `.SRCINFO`, then commits and pushes
   the AUR repository. This comes after step 5 so the PKGBUILD's `source=` already resolves.
7. Runs `update-ptui-homebrew.sh`, which opens a pull request to the Homebrew tap, waits for
   its CI to build and test the formula on Linux and macOS, and merges it (see Step 2). This
   takes 10-20 minutes. Everything else is published by then, so if this fails the release
   still finishes, with a warning and the command to re-run; a PR whose CI failed is left
   open and unmerged, so Homebrew users stay on the previous version.

To rehearse without committing or pushing anything:

    ./release.sh --dry-run --minor

The dry run still edits `Cargo.toml` and builds, then restores `Cargo.toml` and `Cargo.lock`
at the end.

Step 2 - The Homebrew tap
-------------------------

`release.sh` does this for you; run it by hand only to retry after a failure. It needs the tag
to be on GitHub, since it downloads the tag tarball.

    ./update-ptui-homebrew.sh             # version from Cargo.toml
    ./update-ptui-homebrew.sh 2.7.0       # or a given version
    ./update-ptui-homebrew.sh --no-merge  # open the PR and wait for CI, but do not merge
    ./update-ptui-homebrew.sh --no-push   # commit on a branch in the tap, push nothing

The script finds the tap with `brew --repository narbs/homebrew-tap`, refuses to run if the tap
has uncommitted changes, and checks out and pulls `main`. It then downloads
`https://github.com/narbs/ptui/archive/refs/tags/vX.Y.Z.tar.gz`, computes its SHA256, updates
`Formula/narbs-ptui.rb`, checks that both the url and sha256 were replaced, and prints the diff.

Before committing, it runs `brew style` and `brew audit --strict --online` on the new formula,
the same checks as the tap CI's `brew test-bot --only-tap-syntax`. If either fails, it puts the
formula back and stops without committing or pushing.

It then commits `Update ptui to vX.Y.Z` on a branch `narbs-ptui-X.Y.Z`, pushes it, and opens a
pull request titled `narbs-ptui X.Y.Z`. It waits for the PR's CI with `check-homebrew-ci.sh
--formula narbs/tap/narbs-ptui` (see below), and when every job has built and tested the
formula it squash-merges the PR, deletes the branch, and pulls `main`. The local tap is left on
`main` throughout, since `brew doctor` objects to a tap on another branch.

If CI fails or times out, the PR stays open and the script exits 1. Push a fix to the PR's
branch (or wait), then re-run the script with the same version: it finds the open PR and
checks and merges that instead of opening another. If the formula on `main` is already at that
version it does nothing, so it is always safe to re-run.

### The tap's CI

Every push and pull request to the tap runs `brew test-bot` (`.github/workflows/tests.yml` in the tap) on Linux
and macOS 26, against the latest Homebrew. It can fail for reasons no local check
covers: from 2.3.0 to 2.7.6 every run failed in `brew doctor` because the Linux runner's glibc
was too old for Homebrew, and nothing reported it. To check a run by hand:

    ./check-homebrew-ci.sh                                    # the run for the tap's HEAD
    ./check-homebrew-ci.sh <commit>                           # or a given tap commit
    ./check-homebrew-ci.sh --formula narbs/tap/narbs-ptui <pr-head-commit>

Pushes to the tap's `main` check setup and syntax only. The formula itself is built and tested
(`brew test-bot --only-formulae`) only on pull requests, and only on those that change the
formula: a pull request touching just the workflows builds nothing. That is why releases go
through a pull request: until 2.7.6 they were pushed straight to `main`, never built, and a
missing `pkgconf` build dependency went unnoticed. Make any other change to the formula as a
pull request too.

A green run alone does not prove the formula was built: test-bot also goes green when it
skips a formula, or with a failed step "ignored" (a linkage failure was, once). `--formula`
reads every job's log and passes only if each one ran `brew install --build-bottle` and
`brew test` for the formula, with nothing FAILED, SKIPPED or ignored.

Running `brew doctor` on your own machine is not a stand-in: it checks your Homebrew install,
not the clean one CI uses. Run the Linux checks locally in CI's own container instead:

    ./check-homebrew-linux.sh                 # setup, syntax, and a full build + test (~10 min)
    ./check-homebrew-linux.sh --syntax-only   # setup and syntax only (~1 min)

It runs `brew test-bot`'s Linux steps in `ghcr.io/homebrew/brew:main` against the tap as it is
on disk, uncommitted changes included, and never modifies it. Run it before pushing a change to
how the formula builds. It needs Docker without sudo (membership of the `docker` group). It is
not part of `release.sh`, since it takes about ten minutes and a version bump alone does not
change how the formula builds.

Step 3 - macOS build
--------------------

On a Mac:

    ./release-mac.sh

Builds with `--features fast-jpeg`, packs `ptui-X.Y.Z-mac-<arch>.tar.gz` from the release binary,
and uploads it to the GitHub release for the version in `Cargo.toml`. The architecture comes from
`uname -m`, so an Apple Silicon build is named `arm64` rather than being mislabelled `x86_64`.

The script refuses to run anywhere but macOS, since it would otherwise pack a Linux binary under a
macOS name and publish it.

It needs the tag to be on GitHub already, which Step 1 does. If the release does not exist yet, or
`gh` is missing or not logged in, the tarball is still built and the command to run later is
printed - the upload is the only part that is skipped.

    ./release-mac.sh --no-upload    # build and pack only

Re-running after a rebuild replaces the uploaded asset rather than failing.

After the release
-----------------

- Verify the AUR package: `yay -S ptui-bin`
- Verify Homebrew: `brew install narbs/homebrew-tap/narbs-ptui`
- `NEWS.md` is a per-release announcement file and ships in the packages. Update it when a
  release deserves an announcement.
- If a default in `config.rs` changes, update `docs/example.config.ptui.json` to match;
  `test_example_config_matches_defaults` fails until you do.

Development builds
------------------

Not part of releasing, but adjacent:

    ./build_and_run.sh            # debug build with fast-jpeg + debug-output, stderr to log.txt
    ./release_build_and_run.sh    # release build with fast-jpeg, stderr to log.txt
