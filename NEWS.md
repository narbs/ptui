PTUI - Picture TUI - NEWS
=========================

Sep 12, 2025
------------

https://github.com/narbs/ptui

PTUI v1.0.1 is released with these features:

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

Install From AUR (Arch Linux):

    yay -S ptui-bin

Controls:
```
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
```

Author: Christian Clare

