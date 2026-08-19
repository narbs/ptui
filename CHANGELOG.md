PTUI - Picture TUI - CHANGELOG
==============================

Aug 19, 2026
------------

PTUI 2.6.0 released. A configuration file that cannot be parsed is no longer replaced with the
defaults. ptui now leaves it exactly as it is, runs on defaults for that session, and reports the
problem with its line and column in the messages pane, so a stray comma no longer costs you the
settings in the file. Editing the file while ptui is running already behaved this way; only starting
up did not.

Configuration files may also now set only the keys they care about, with everything else taking its
default. Unknown keys were already ignored, so requiring every known one was the odd rule out.

Ctrl+C now quits, from anywhere including while a dialog is open. It used to open the copy dialog,
because c is the copy shortcut and the binding took no modifier. The README had said Ctrl+C quits
for some time, so this makes the program agree with its own documentation rather than the other way
round.

Ctrl+F and Ctrl+B are named in the help. They have been bound for a long time but appeared nowhere
in it. The README's list of controls has been brought back in line with the keys the app actually
binds: it was missing Page Up, Ctrl+F/Ctrl+B, f/b, r, u and Esc.

Aug 19, 2026
------------

PTUI 2.5.0 released. Sort by star rating with s. The first press puts the best first and unrated
files last, and pressing it again reverses that so unrated files lead and the best come last; files
sharing a rating are ordered by name. d and n still sort by date and name.

Saving an ASCII file now redraws the screen immediately. The new file appeared in the listing only
after the next keypress, which made the save look like it had done nothing. The same applied to a
few other messages, including the refusal to delete a directory and any failure reported while
saving, copying or deleting.

Note that s previously saved an ASCII file. That has moved to i, on the other side of the keyboard,
so the three sort keys now sit together on d, n and s.

Aug 18, 2026
------------

PTUI 2.4.1 released. Fixes a rating lost when copying or moving a file in a folder where you
declined XMP sidecars. Those ratings are kept in ptui's own state file, keyed by the file's full
path, and were not updated when the file went somewhere else: after a move the rating was stranded
under the old path and the file showed as unrated at its destination, and a copy arrived unrated
where a sidecar would have been carried across. Ratings stored in sidecars, which is the default,
were never affected, and deleting a file already cleared both kinds.

Aug 18, 2026
------------

PTUI 2.4.0 released. Star ratings. Press 1 to 5 to rate the selected file, 0 to clear it, or * for
five; the rating shows as a star and a number beside the file name. Ratings are written to XMP
sidecar files using the standard xmp:Rating property, the same one darktable, digiKam, RawTherapee,
Bridge and Lightroom read, so a rating made in ptui is visible in those programs and syncs with the
image rather than being stranded on one machine. Because that means creating files in your image
folders, ptui asks the first time you rate something in a folder and remembers the answer; declining
keeps that folder's ratings in ptui's own state file instead. Set stars.sidecars in the config to
"always" or "never" to skip the question entirely. Both sidecar naming conventions are read, and
ptui writes whichever one a folder already uses. An existing sidecar is updated rather than
rewritten, so an edit history left in one by another program survives being rated. Deleting a file
deletes its sidecar, and copying or moving a file brings its sidecar along.

The file list pane is wider to make room for the rating without shortening file names, and no
longer narrows as the terminal grows: a cutoff at 120 columns used to drop its share from 15% to
10%, so a 140-column terminal gave a narrower pane than a 120-column one. It now takes 12% of the
width with a minimum of 21 columns, which keeps roughly a dozen characters of file name readable
at any size.

Aug 7, 2026
-----------

PTUI 2.3.1 released. Copying and moving a file now happens as soon as the destination folder is
chosen, instead of asking for a further confirmation. Replacing a file of the same name still asks
first. Confirmation dialogs now accept Enter as well as y. After a copy, move, delete or ascii save
the file list is reselected by name rather than by position, so files added or removed in the
background no longer drag the highlight onto an unrelated file: a copy keeps the highlight on the
file that was copied, a move or delete puts it on the file that followed it, and saving an ascii
file keeps it on the image it was made from. Confirmation dialogs also size themselves to their
contents, fixing prompts that were cut off in languages with longer text than English. Pressing n
again now reverses the name sort between A to Z and Z to A, matching how d toggles the date sort,
and reports the new order in the messages pane

Aug 6, 2026
-----------

PTUI 2.3.0 released. Copy and move files without leaving the browser. Press c to copy or m to move
the selected file, then choose a destination folder by number: the folder you last copied or moved
to, followed by the standard folders that exist on your system (home, desktop, downloads, documents,
pictures, and a projects folder if you have one). The final number switches to free-text path entry,
where ~ is expanded and relative paths resolve against the folder you are browsing. Transfers are
confirmed before anything is written, and you are asked again before overwriting an existing file.
The last destination is remembered between sessions, so repeating a copy to the same folder takes a
couple of key presses. Available in all six supported languages.

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
