PTUI - Picture TUI
==================

A terminal-based image viewer written in Rust that provides a file browser
interface with real-time image preview capabilities.

![PTUI - Picture TUI](docs/ptui_image.png)

Features
--------
- Support for common image formats
- Real-time image preview using ANSI terminal graphics
- Multiple picture-to-text converters: chafa and jp2a supported so far
- Slide show mode with arrow-key support and transitions (transitions only with jp2a)
- Navigate with arrow keys or vim-style j/k
- Enter directories with Enter, go back with Backspace
- Multilingual support (English, German, Spanish, French, Japanese, Chinese)
- Dynamic window resizing with [ and ] keys and when terminal changes
- Caching of rendered images for performance
- Scrollable file lists for directories with many files
- Support for both image and text file preview
- Open in file system browser (if available)
- Delete file
- Save picture to ascii
- Sort by date asc/desc or name
- Dynamic reloading of configuration

Requirements
------------
- chafa - For converting images to ANSI/terminal output
- ImageMagick (identify command) - For image dimension detection

Installation
------------

From Source:
    yay -S cargo
    cargo build --release
    cargo install --path .

From AUR (Arch Linux):
    yay -S ptui-bin

Usage
-----
    ptui [directory]

If no directory is specified, ptui starts in the current directory.

Controls:
    Arrow Keys / j,k  - Navigate file list
    Enter             - Enter directory
    Backspace         - Go to parent directory
    [ / ]             - Resize preview window
    space             - Start Slideshow (Arrows work here too)
    x                 - Delete file
    s                 - Save file to ascii
    d, n              - Sort by date (toggle newest/oldest), n: Sort by name
    Home/End          - Home: Go to start, End: Go to end
    o                 - Open in system file browser (if available)
    q / Ctrl+C        - Quit
    ?                 - Help

Configuration
-------------
Configuration file is automatically created at ~/.config/ptui/ptui.json.

Edits refresh in the app automatically.

Example chafa configuration:

```json
{
  "converter": {
    "chafa": {
      "format": "ansi",
      "colors": "full"
    },
    "jp2a": {
      "colors": true,
      "invert": false,
      "dither": "none",
      "chars": null
    },
    "selected": "chafa"
  },
  "locale": "en",
  "slideshow_delay_ms": 2000,
  "slideshow_transitions": {
    "enabled": false,
    "effect": "scattering",
    "frame_duration_ms": 50
  }
}
```

Example jp2a configuration with slide show transitions:

```json
{
  "converter": {
    "chafa": {
      "format": "ansi",
      "colors": "full"
    },
    "jp2a": {
      "colors": true,
      "invert": false,
      "dither": "none",
      "chars": null
    },
    "selected": "jp2a"
  },
  "locale": "en",
  "slideshow_delay_ms": 2000,
  "slideshow_transitions": {
    "enabled": true,
    "effect": "scattering",
    "frame_duration_ms": 50
  }
}
```

Building
--------
    cargo build      - Compile the project
    cargo run        - Build and run
    cargo test       - Run tests
    cargo check      - Quick syntax checking
    cargo clean      - Remove build artifacts

License
-------
MIT License - see LICENSE file for details

Author
------
Christian Clare

Repository
----------
https://github.com/narbs/ptui
