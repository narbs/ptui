PTUI - Picture TUI - NEWS
=========================

Oct 2, 2026
-----------

https://github.com/narbs/ptui

PTUI v2.6.1 is released. A terminal image viewer with a file browser and live previews.
See CHANGELOG.md for the full history of changes.

Highlights since v1.0:

- Graphical previews with the kitty and iTerm2 protocols (Ghostty, kitty, iTerm2), alongside
  chafa and jp2a, switched at any time with TAB
- Star ratings with 0-5, stored as XMP sidecars that darktable, digiKam and Lightroom read,
  and sorting by rating with s
- Copy (c) and move (m) files to a remembered or standard folder, taking ratings along
- r re-reads the file list, preview and rating, keeping the selection
- A configuration file that cannot be parsed is left untouched and reported, never replaced
- Configuration files only need the keys they change; see example.config.ptui.json

Features:

- Support for common image formats
- Real-time image preview using ANSI terminal graphics
- Slide show mode with arrow-key support and transitions (transitions only with jp2a)
- Navigate with arrow keys or vim-style j/k
- Multilingual support (English, German, Spanish, French, Japanese, Chinese)
- Dynamic window resizing with [ and ] keys and when the terminal changes
- Text file preview, open in the system file browser, delete, save picture to ascii
- Sort by date, name or star rating
- Dynamic reloading of configuration

Install from the AUR (Arch Linux):

    yay -S ptui-bin

Install from Homebrew (Linux or Mac):

    brew install narbs/homebrew-tap/narbs-ptui

Configuration:

The configuration file is created on first run at ~/.config/ptui/ptui.json (on a Mac,
"$HOME/Library/Application Support/ptui/ptui.json"). Packages also install a copy of the
defaults at /usr/share/doc/ptui/example.config.ptui.json.

Controls:
```
    Arrow Keys / j,k  - Navigate file list
    Page Up/Page Down - Jump by a page
    Enter             - Enter directory
    Backspace         - Go to parent directory
    [ / ]             - Resize preview window
    r                 - Re-read the file list, preview and rating
    space             - Start slideshow (arrows work here too)
    x                 - Delete file
    c / m             - Copy / move file to a folder
    i                 - Save file to ascii
    0-5 / *           - Rate the selected file
    d, n, s           - Sort by date, name or rating
    o                 - Open in system file browser (if available)
    TAB               - Cycle between converters
    ?                 - Help
    q / Esc / Ctrl+C  - Quit
```

See README.md for the full list of controls and settings.

Author: Christian Clare
