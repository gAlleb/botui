# Reader verification checklist

**English** · [Русский](checklist.ru.md)

The table records how each reading or navigation scenario was checked. See the [README](../README.md) for the commands to run the checks.

| Scenario | Verification |
|---|---|
| Sign in to BookOrbit with no personal collections. Choose a library, book and edition, then navigate back. | smoke-shelf |
| Find a book beyond the first batch of results. Check progress on the current shelf, recent additions and continuous navigation in list and grid views. | smoke-shelf |
| Open a shelf from the dashboard. Find an author or series beyond the first batch. Check counts before opening the index and verify volume order. | smoke-shelf and a separate Kitty session |
| Hold the second page of Authors and Series in grid and list view. Check that the first 24 entries keep their coordinates, click a card at its original position, and use F6 to move from Series to Authors. | smoke-shelf |
| Hold the thumbnail response, then release it. Only the book with a missing cover gets a generated jacket. Delay a directory request, cancel it and check that the shelf, rail and cached cover stayed visible. | smoke-shelf and a pixel check in headless Kitty |
| Delay sign-in and the first dashboard response with saved credentials. Check that both show Loading and that a rejected password opens the sign-in form. | smoke-config |
| Press a shelf shortcut and immediately press Enter. Close a book and click another edition while progress is being saved. Check which book and edition open. | smoke-shelf and smoke-progress |
| Change the Kitty font size with a cover grid open. Compare the settled grid with a fresh launch at the same font size. | Pixel comparison in headless Kitty |
| Check a portrait cover in adjacent cards of odd and even widths. Measure both side margins and the gap above the caption on the rendered image. | Cover geometry test and a headless Kitty capture at 11 × 23 pixels per cell |
| Migrate settings to TOML, start two instances and check plaintext passwords, password commands and account isolation. | smoke-config |
| Adjust page width and gutter independently. Check shortcut alignment and rounded, square and hidden borders. | smoke-reader, smoke-library, layout tests and *Chaos and Amber* in Kitty |
| Paste a folder path with the mouse, find a book in a subfolder, switch views and restart. | smoke-library and a local folder containing *Sign of the Unicorn* |
| Enter folders and list views without waiting for unused image queries. Enter a cover folder, wait for all generated jackets, leave and return; check balanced synchronized frame updates. | smoke-local-startup in an isolated PTY with Kitty protocol replies |
| Change background and theme, switch between regular and Zen reading, use the mouse and restore a saved position. | smoke-reader and Kitty in regular and Zen modes |
| Switch between one page and a spread. Adjust the margins of both columns. Find a word rejoined across a line break and return to its original location. | botui-core and a Gesar EPUB |
| Search an OPDS catalog, authenticate with HTTP Basic, follow feeds and choose an edition. | smoke-sources |
| Display actual PDF and DjVu page images, open a spread, search the text layer and jump to a page. | smoke-sources |
| Create local notes and bookmarks, export Markdown and track time with the book open. | smoke-reader and smoke-sources |
| Restore an exact BookOrbit file position, handle a conflict with another device and list a server bookmark without a location. | smoke-progress |
| Open an edition for the first time and reuse an older offline copy. Read an EPUB from chapter 5 online to chapter 7 offline, then resume in BookOrbit. Check that the file transfers once, the two download shelves stay separate, progress syncs on reconnect and 99% can reach 100% at the same location. | smoke-offline |
| Follow a link and return. Open a footnote and an illustration in EPUB and FB2. | smoke-resources |
| Choose independent themes, opacity and border style. Use help and connection settings, then restart to check persistence. | smoke-preferences |
| Fold the tree with mouse and keyboard, open book details without adding to reading history, put a book on hold, then cancel and confirm a reset. | smoke-details |
| Scroll text by one line. Show parts of two adjacent scanned pages and restore the saved scroll offset. | smoke-scroll |
| Jump to page 137, find text on pages 137 and 139, and see the boundary with page 138 at full width. | Release build in Kitty with a 140-page test PDF. [Page boundary screenshot](images/pdf-scroll.png) |
| Press Q twice to return to the source chooser after saving. Reopen the book without signing in again. | smoke-progress |
| Change the palette in a running terminal without pywal and compare the rendered background colors. | Isolated Kitty and Alacritty sessions |
| Find a phrase after reflow. Navigate between results, contents entries and anchors. | smoke-text-search, botui-core and botui-formats tests |
| Open the server reading log and related books. Delete a single bookmark, including one with an unresolved location, and cancel deletion. | smoke-details and smoke-bookorbit-bookmarks |
| Drag the sidebar edge on the shelf and in both readers. Cancel with Esc and check the saved width on reopening. | smoke-splitters |
| Reopen a PDF at page 111. With enough space, all 24 contents entries are visible and chapter 11 is selected. Shrink and restore the window height with the chapter still selected. | Contents tests and the installed executable with *How to Cook Your Life* in a separate profile |
| Click a PDF or DjVu edition to open it. Reduce page width from 100% to 90%, restore it and use the small page-number dialog. | smoke-shelf, smoke-sources, smoke-scan-controls and a Kitty pixel check |
| Download a 790 MiB EPUB through BookOrbit and OPDS without Content-Length. Cancel a stalled transfer, retry and compare SHA-256. | scripts/check-downloads.py using the original file and temporary local HTTP servers |
| Expand the local folder tree and open a book on the right. Collapse the tree, resize it and return to the book and reading history. | smoke-local-navigation and a Kitty session in Local library |
| Click the publisher’s footnote [1] in *Sign of the Unicorn* or follow it with F6. Return to the same passage using Esc or the publisher’s ↩ link. | The EPUB in an isolated profile. smoke-resources also checks glossary entries, backlinks and resizing. |
| Jump to a help section or search for a command with readable columns at 42 × 14 cells. | smoke-preferences |
| Open the reader description in About and the adjacent Legal tab. Follow the attribution link with the mouse and read the full licenses from the CLI. | smoke-about |
| Open a styled EPUB with a missing sheet, local imports, a media query, merged cells, code and equations at three widths. Search the visible text. | botui-formats |
| Open a BookOrbit position inside a long merged cell, scroll, close, remove the device position and open the server position again. | smoke-progress, using an independently authored UTF-16 EPUB CFI |
| Resolve BOTUI locations in the BookOrbit web engine and pass them back. Check exact characters and source offsets. | BookOrbit 3.6.0 Foliate in headless Chromium, 10 positions in a novel and 11 in a technical EPUB |
| Search an illustrated 790 MiB EPUB and jump to the matching passage. | *The Joy of Sanskrit*, isolated tmux |
| Open a Cyrillic novel, jump via the contents, turn a spread and reopen it. Check headings and paragraph gaps. | *Sign of the Unicorn* in isolated tmux, plus botui-formats |
| Advance in one window, then change the appearance of the stale window. Deliberately navigate back afterwards. | botui-store |

## Coverage limits

### Reader performance

Compare release binaries on the same machine, book and terminal dimensions, with separate temporary profiles. Include the last release before a rendering change, not just the broken build. Measure zen progress, sidebar, page modes, width, gap, themes, page turns and contents. Let image loading finish before measuring steady-state input; record cold image loading separately.

The core benchmark reports the time until the visible spread is laid out and the total time until the page count is ready. Terminal drawing and image decoding are measured separately.

When changing the release profile, compare uncompressed executables built for the same architecture and C library. Check page-width changes and sidebar opening on a novel and an illustrated textbook, and compare the rendered table and equation in a technical EPUB.

```sh
cargo run --locked --release -p botui-formats --example reflow-bench -- /path/to/book.epub
```

For distribution timings:

```sh
cargo zigbuild --locked --release -p botui-formats --example reflow-bench --target x86_64-unknown-linux-musl
target/x86_64-unknown-linux-musl/release/examples/reflow-bench /path/to/book.epub
```

Avoid concurrent builds or other benchmarks. Repeated layouts must match a fresh layout, including CSS media queries, table spans, equations, images, search and source positions. `smoke-reader` also checks that toggling zen progress leaves every text row unchanged.

The canvas regression compares grouped rendering with individual shaping clusters across widths, justification, word spacing, case transformations, Cyrillic, combining characters, CJK and bidirectional text. Compare text, styles and source addresses, not only page counts.

### Functional limits

- **BookOrbit.** Bookmark responses were checked against a personal server for specific EPUB and FB2 editions. A local HTTP fixture exercises status changes, resets and progress conflicts. A full web → iPhone → TUI check requires a physical device and the same edition file.
- **EPUB layout.** Tests cover local CSS, Grid, Flexbox, table spans, fractions, roots, scripts, matrices and marked TeX. The terminal supplies one font size and the reader supplies the palette. Pixel-faithful fixed-layout pages and arbitrary SVG diagrams are outside this coverage. FB2 has its own text and raster-image reader.
- **OPDS 1 and 2.** Tests cover direct file acquisition. DRM and separate lending workflows are unsupported.
- **PDF and DjVu.** Image-only pages are readable. Search requires an existing text layer. The server syncs the page number, and the device also saves the scroll offset.

## Next reader improvements

1. **SVG illustrations in EPUBs.** Display drawings stored as inline SVG or linked SVG files. Test a labelled diagram, a formula supplied as a picture and a fixed-layout spread with text over an image. The reader already typesets equations written in MathML or marked TeX.
2. **Search in scans.** Offer on-demand OCR for PDF and DjVu pages without text. Cache results per edition and page so a second search works offline.
3. **Quoting a passage.** Let a reader select exact text with the mouse or keyboard, then copy it or attach a note to the selection. Keep both endpoints across a resize. [Bookokrat selection command](https://github.com/bugzmanov/bookokrat/blob/9c02ee3b3a0f86791b3917cb1a93436d92a8603e/src/keybindings/action.rs#L354)

### Follow-up checks

- Repeat the cover-spacing check in Alacritty at two font sizes. In Authors and Series, click the same card before and after the next batch arrives and confirm that both clicks open the same entry.
- Read the same EPUB edition in BOTUI, the BookOrbit web reader and on an iPhone, then return to BOTUI. Compare the first visible phrase at each handoff and open the same bookmark on all three devices.
