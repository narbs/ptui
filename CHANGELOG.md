PTUI - Picture TUI - CHANGELOG
==============================

Aug 6, 2026
-----------

PTUI 2.3.0 released. Copy and move files without leaving the browser. Press c to copy or m to move
the selected file, then choose a destination folder by number: the folder you last copied or moved
to, followed by the standard folders that exist on your system (home, desktop, downloads, documents,
pictures, and a projects folder if you have one). The final number switches to free-text path entry,
where ~ is expanded and relative paths resolve against the folder you are browsing. The transfer
happens as soon as the folder is chosen, except when a file of the same name is already there, which
asks for confirmation before overwriting. The last destination is remembered between sessions, so
repeating a copy to the same folder takes a couple of key presses. Available in all six supported
languages.

Jan 23, 2026
------------

PTUI 2.2.4 released. Fix divide by zero when calculating image dimensions on iTerm2

Jan 23, 2026
------------

PTUI 2.2.3 released. Fix the delete file dialog appearing behind graphical previews, and center images
vertically in graphical mode when horizontal space is constrained

Jan 20, 2026
------------

PTUI 2.2.2 released. Fix slideshow keyboard navigation and remove the delay when pressing space to
start a slideshow

Jan 19, 2026
------------

PTUI 2.2.1 released. Protect the cursor position in the preview to avoid artifacts in the status bar

Jan 19, 2026
------------

PTUI 2.2.0 released. Move to ratatui 0.30 and update ratatui-image and its dependencies. Add a custom
Kitty preview implementation that adapts the aspect ratio to speed up rendering, allowing higher image
quality. Fix image placement and preview window sizing, and fix the slideshow pausing at start and
showing artifacts in its status bar

Jan 17, 2026
------------

PTUI 2.1.0 released. Declare jp2a as a package dependency and move zune-jpeg behind the debug-output
feature flag

Jan 16, 2026
------------

PTUI 2.0.0 released. Add graphical mode, displaying real images in terminals that support them, using
ratatui-image and viuer with optional turbojpeg acceleration behind the fast-jpeg feature. Detect
terminal capabilities and support Kitty, Ghostty, iTerm2 and macOS Terminal. Add Tab to cycle between
converters (chafa, jp2a and graphical). Remember the selected position in a directory when returning
from a subdirectory. Left-align text files while keeping images centered. Add timing and graphics
logging behind a debug-output feature flag

Jan 7, 2026
-----------

PTUI 1.0.10, 1.0.11, 1.0.12 released. Update the help text shown when the app starts

Nov 11, 2025
------------

PTUI 1.0.9 released. Update the PTUI logo shown at app startup, and add Homebrew tap install details
and a Homebrew publishing script

Sep 24, 2025
------------

PTUI 1.0.7, 1.0.8 released. Add --version command line argument for homebrew compatibility

Sep 23, 2025
------------

PTUI 1.0.6 released. Better text rendering

Sep 14, 2025
------------

PTUI 1.0.5 released. Better SVG support

Sep 14, 2025
------------

PTUI 1.0.4 released. Implement text scrolling using space bar to go down and u to go up

Sep 13, 2025
------------

PTUI 1.0.2 and 1.0.3 released. Minor fixes and optimizations for file examination

Sep 12, 2025
------------

PTUI 1.0.1 released. It's a useful Picture TUI for commandline fanatics, and has support for image to text conversion using chafa and jp2a, slideshow mode, window resizing and basic file management features
