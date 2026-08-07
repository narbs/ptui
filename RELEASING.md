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
- For the Homebrew step, the tap checked out at
  `/home/linuxbrew/.linuxbrew/Homebrew/Library/Taps/narbs/homebrew-tap`.

Before you start
----------------

1. Merge the work into `main` - releases are cut from `main`.
2. Add a `CHANGELOG.md` entry for the new version. `release.sh` does *not* do this; its commit
   contains only `Cargo.toml` and `Cargo.lock`, so the changelog needs to be committed
   beforehand to be included in the tag.
3. Update `README.md` if controls or configuration changed.

Step 1 - Release, tag and publish to the AUR
--------------------------------------------

    ./release.sh --minor     # or --patch / --major

Note that `./release.sh` with no arguments prints help and exits - the bump type is required.

The script:

1. Bumps the version in `Cargo.toml` (patch/minor/major).
2. Builds with `cargo build --release --features fast-jpeg`.
3. Runs `cargo test`. The release aborts if either fails.
4. Commits `Cargo.toml` and `Cargo.lock` as `Bump release to vX.Y.Z`.
5. Creates tag `vX.Y.Z` and **pushes the tag** to origin.
6. Runs `cargo aur`, then `patch-aur-pkgbuild.sh` to add `--features fast-jpeg` to the
   generated PKGBUILD.
7. Copies the PKGBUILD into `../ptui-aur`, regenerates `.SRCINFO`, rewrites
   `pkgbase = ptui-bin` to `pkgbase = ptui`, then commits and pushes the AUR repository.

To rehearse without committing or pushing anything:

    ./release.sh --dry-run --minor

The dry run still edits `Cargo.toml` and builds, leaving the files in place for inspection; it
keeps a `Cargo.toml.backup` and removes it at the end.

Step 2 - Push main
------------------

    git push origin main

`release.sh` pushes the tag but not the branch, and reminds you of this when it finishes.

Step 3 - Update the Homebrew tap
--------------------------------

Only works once the tag is on GitHub, since it downloads the tag tarball.

    ./update-ptui-homebrew.sh 2.3.0

The script downloads `https://github.com/narbs/ptui/archive/refs/tags/vX.Y.Z.tar.gz`, computes
its SHA256, and updates `Formula/narbs-ptui.rb` in the local tap. It does **not** commit -
review and push the tap yourself:

    cd /home/linuxbrew/.linuxbrew/Homebrew/Library/Taps/narbs/homebrew-tap
    git diff Formula/narbs-ptui.rb
    git add Formula/narbs-ptui.rb
    git commit -m "Update ptui to vX.Y.Z"
    git push

Step 4 - macOS build (optional)
-------------------------------

On a Mac:

    ./release-mac.sh

Builds with `--features fast-jpeg` and produces `ptui-X.Y.Z-mac-x86_64.tar.gz` from the release
binary. Distributing that tarball is a manual step.

After the release
-----------------

- Verify the AUR package: `yay -S ptui-bin`
- Verify Homebrew: `brew install narbs/homebrew-tap/narbs-ptui`
- `NEWS.md` is a per-release announcement file and is currently stale (it still describes
  v1.0.1). Update it if the release deserves an announcement.

Development builds
------------------

Not part of releasing, but adjacent:

    ./build_and_run.sh            # debug build with fast-jpeg + debug-output, stderr to log.txt
    ./release_build_and_run.sh    # release build with fast-jpeg, stderr to log.txt
