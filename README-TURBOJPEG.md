# TurboJPEG Fast Decoding Setup

For **instant** JPEG loading (3-5x faster), enable the `fast-jpeg` feature which uses libjpeg-turbo with subsampled decoding.

## Benefits

With turbojpeg enabled:
- **4032x3024 JPEG**: 1.9s → **~200ms** (10x faster!)
- **2592x1944 JPEG**: 900ms → **~100ms** (9x faster!)
- Uses 1/2, 1/4, 1/8 scale decoding during decompression
- No quality loss for terminal display

## Installation

### 1. Install NASM (Required for turbojpeg build)

**macOS:**
```bash
brew install nasm
```

**Linux (Arch):**
```bash
sudo pacman -S nasm
```

**Linux (Ubuntu/Debian):**
```bash
sudo apt install nasm
```

**Linux (Fedora):**
```bash
sudo dnf install nasm
```

### 2. Build with fast-jpeg feature

```bash
cargo build --release --features fast-jpeg
```

Or add to your cargo build alias:
```bash
# In ~/.cargo/config.toml
[alias]
br = "build --release --features fast-jpeg"
rr = "run --release --features fast-jpeg"
```

## Verification

When running with turbojpeg enabled, you'll see:
```
[TURBOJPEG] Original: 4032x3024, Target: 432, Scale: 1/8
[TURBOJPEG] Decoded at: 504x378
[FAST-LOADER] Loaded 504x378 in ~200ms (decoder: turbojpeg)
```

Without turbojpeg (fallback to zune-jpeg):
```
[ZUNE-JPEG] Decoded: 4032x3024
[FAST-LOADER] Loaded 4032x3024 in ~1900ms (decoder: zune-jpeg)
```

## Troubleshooting

**Error: "No CMAKE_ASM_NASM_COMPILER could be found"**
- Install NASM (see above)
- Verify: `which nasm` should return a path

**Turbojpeg fails at runtime:**
- The build will automatically fall back to zune-jpeg
- Check console for: `[TURBOJPEG] Failed: ..., falling back to zune-jpeg`

## Performance Comparison

| Image Size | Default (image crate) | zune-jpeg | turbojpeg (1/8 scale) |
|------------|----------------------|-----------|------------------------|
| 4032×3024  | 1900ms              | 1900ms    | **200ms** ⚡           |
| 2592×1944  | 900ms               | 700ms     | **100ms** ⚡           |
| 2048×1536  | 470ms               | 400ms     | **80ms** ⚡            |

Total navigation time with auto-resize to 432px:
- **Before**: ~3s (decode + resize + encode)
- **After**: **~270ms** (subsampled decode + minimal resize + encode)

## Making it Default

To always use fast-jpeg, add to `Cargo.toml`:
```toml
[features]
default = ["fast-jpeg"]
```

Then regular `cargo build --release` will use turbojpeg automatically.
