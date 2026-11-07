# Changelog

## 0.1.0 — 2026-11-07

Read EPUB, FB2, PDF and DjVu from a local library, an OPDS catalog or a BookOrbit account. Switch between a single page, a two-page spread and continuous scrolling.

- Browse books by cover or in a list, search local and BookOrbit libraries as you type and resume a saved reading position. The BookOrbit dashboard includes current reads, recent additions, authors and series.
- The first time you read a BookOrbit edition, BOTUI saves it on your device. Continue from **BookOrbit offline downloads** without a connection and sync your place when you reconnect.
- Navigate chapters and footnotes, search inside books, set bookmarks and export passage notes. For scanned documents, adjust page width, rotate pages and switch between original colors, inverted colors and paper matching the reading pane. Page turns prefetch both directions, and Kitty scrolling reuses the page image.
- Open cover art in your system image viewer from a book card or reading menu. Missing covers use BOTUI’s cloth jacket with a foil-stamped sprig. The source chooser selects the local library first and keeps shortcut hints in place as you browse. The pixel owl follows the interface palette, with Catppuccin Mocha as the default.
- Choose a terminal palette or a built-in theme and set page width, gutter, margins and first-line indentation. The keyboard guide and settings are also accessible with a mouse.
- Download static Linux binaries for x86_64 and aarch64, with SHA-256 checksums. For a native build, `mise run install` places the executable in `./dist/botui`. The English and Russian guides cover installation and reading controls.
