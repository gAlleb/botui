# Changelog

## 0.1.8 — 2026-11-12

### Reading and formats

- Read Markdown with styled headings, lists, quotes, tables, code highlighting and local images. Switch between formatted text and the original source with F9, Space r v or the footer while retaining highlight and note addresses. Reuse the ebook layout engine with a small CommonMark parser; no external renderer or syntax package is required.
- Open CBZ and CBR comics with built-in ZIP/RAR decoding, natural page order, covers, contents, zoom, spreads, scrolling and saved page notes. No external unpacker is required.
- Set a persistent 0–32-cell gap between PDF/DjVu/comic pages in Space l w. Zero joins facing pages. Zoom single pages and spreads into the available margins, clipping only at the reading pane; keep PDF text hits, highlights and annotation jumps aligned.
- Keep PDF/DjVu pages visible while a sharper zoom renders. Reuse cached images when reducing or repeating a zoom level.
- Select PDF/image colors through Space t with live preview: Original, Inverted or a reading palette. F2 → Reader → PDF / image theme edits the same saved scan_theme setting. Reading and interface palettes remain independent; cancel restores the previous appearance.
- Add Graphite, Midnight, Pine and Plum dark interface themes, plus Parchment, Sage, Dusk and Slate reading palettes in light/dark variants.

### Selection, highlights and notes

- Select and copy text, create colored highlights and attach multiline comments in EPUB, FB2, Markdown and PDFs with embedded text. Source ranges survive reflow; PDF highlights follow page coordinates through zoom, rotation and reopening. Image-only scans are not OCRed.
- Right-click a word or selection for a compact menu beside the text: Copy, Highlight, Search, Translate, Define and Add note. Clicking inside a selection preserves it; Escape or an outside click closes the menu.
- Click a saved annotation to open it and attach a comment to an existing highlight. Choose colors with clickable buttons. Clearing a comment preserves its colored highlight.
- Search, edit, recolor, delete, navigate to and export annotations from Space a a. Lists show book/section progress or PDF page/total; filtered PDF exports match the visible list.
- Finish Visual selection after saving a note and restore it correctly with gv. Handle pasted searches and navigation keys consistently in annotation panels.

### Navigation and controls

- Space opens a contextual command menu with a configurable delay. Space c enables an optional Vim cursor with counts, motions, yank operators, Visual selection, search and named marks. The cursor stays hidden during ordinary reading; Escape clears a selection before returning to reading.
- Q goes back one step through BookOrbit cards and their source shelf, or through local folders to the library root. Keep selection and scroll position when returning, and finish outstanding progress saves before leaving.
- Restore direct q/Q reader navigation, PDF/DjVu +/=/- zoom and 1/2/3 page layouts in ordinary reading. Digits remain counts in Vim. Q, Escape and Backspace return one level within Space submenus.
- Scroll text and scans by one line per mouse-wheel tick by default; configure 1–12 lines in F2 → Reader → Mouse wheel step. Vim j/k follow visible text rows, and braces move by paragraphs.
- Keep settings on the interface palette and reader popups on the reading palette. Align dialog fields and clickable action buttons; show all supported formats on the source screen.

### Dictionaries and configuration

- Configure separate word-definition and translation connections using generic HTTP, OpenAI-compatible APIs or local commands. Edit connections, defaults, language parameters, keys or key commands in F2 and TOML; copy results or save them as passage notes.
- Remove hardcoded dictionary/translation adapters. Free Dictionary and Google Translate remain enabled as ordinary editable HTTP profiles in the default configuration and example. Migrate older named providers once; deleting the last connection keeps the list empty.
- Connection tests contact the provider instead of using cached responses. Preserve literal placeholder text in local commands and the selected language when reselecting the same connection.
- Configure named OPDS catalogs and credentials in [[opds]] and F2 → Library. Backfill missing configuration settings without overwriting existing values or comments.
- Synchronize both READMEs, user guides, help and configuration examples. Explain required versus optional dictionary fields and include a Merriam-Webster Collegiate template.

## 0.1.7 — 2026-11-10

- Fixed a regression in 0.1.6 that stretched spaces between words in justified EPUB prose, especially in narrow columns and two-page spreads. Words now keep their natural spacing across page widths. Headings, right-aligned passages and code retain their layout.
- Set the space between EPUB and FB2 paragraphs in **F2 → Reader → Paragraph spacing**. Choose **From book** or 0, 1 or 2 blank lines. The change applies to the open book, keeps your place and carries over to the next session.
- Settings now show the hint for the selected row. The sidebar width controls identify the divider you can drag in the reader or on the bookshelf.

## 0.1.6 — 2026-11-10

- EPUB displays static inline and linked SVG illustrations, including labels, shapes, transforms and embedded archive images, in the ordinary reader and image-only albums. Visible illustrations rasterize on existing background workers and reuse the image cache; source text addresses remain unchanged.

- Generated jackets reuse a bounded disk cache between launches. Font discovery uses the system font index when available, without blocking shelf navigation or flashing temporary artwork.
- PDF and DjVu contents retain nested sections and support folding with the mouse or arrow keys. Outline levels use shades from the active theme in text and scan readers; reopening a book keeps the current section selected without hiding a short outline above it.
- Continuous PDF/DjVu scrolling marks page boundaries across the reading pane with a muted dotted line. Kitty reuses bounded page uploads even in wide windows; neighboring pages prepare their graphics on render workers instead of blocking at page boundaries.
- FB2 preserves italic, bold and struck-through passages across wrapping and resume, using the lightweight text path and the original progress/link addresses.

- Release binaries use size optimization while EPUB reflow and image resizing retain speed optimization.
- Author and series cards keep their positions as more results arrive. F6 focuses the current BookOrbit section, including after navigation by number.
- Shelf cards center the cover itself and leave a small gap above its title. Downloaded covers and generated jackets use the same spacing across BookOrbit, OPDS and local grids.
- BookOrbit shows a single loading dialog during sign-in with saved credentials. A rejected password opens the form for retry.
- BookOrbit remembers Enter while a shelf loads and a click on an edition while the previous reading position is being saved. Clicking a book still visible during navigation opens that book.
- Visible thumbnails load continuously across two workers. When the shelf changes, queued thumbnails and generated jackets give way to the books now on screen. Cover art and scanned pages follow changes to the terminal font size.
- BookOrbit keeps the current shelf, navigation and covers visible while the next section loads. A book with an incoming cover shows its title immediately and displays the image as soon as it arrives. The local library and OPDS use the same cover behavior.
- Changing width or opening the contents sidebar in a multi-chapter EPUB lays out the visible spread first. BOTUI finishes the book-wide page count in the background and keeps the same reading position.
- Toggling zen progress leaves the text in place and no longer repaginates the book. EPUB resizing reuses parsed chapters and a bounded cache of recent layouts; chapter layout runs in parallel, with a faster allocator in static musl builds.
- Local folders and list views open without waiting for image protocol discovery. Switching to cover view still detects terminal graphics when needed.
- Generated jackets are prepared in the background while the shelf opens. Synchronized terminal updates present navigation and covers in one frame.
- Styled EPUB chapters use embedded CSS layout in the regular reader. Merged cells, equations in MathML or marked TeX, and preformatted listings share the book’s navigation, search, bookmarks and BookOrbit positions.
- Inline EPUB and FB2 illustrations grow with the reading pane. In single-page and two-page layouts, a picture that fits on a fresh page moves there whole. Continuous scroll reveals successive parts of the same picture at a constant scale.
- Changing appearance in a second local reader preserves the position saved by the window used for reading.
- Justified EPUB prose keeps words intact even when a word crosses a change of emphasis or falls on a half-cell boundary.

- Author and series lists align book counts in a separate column, including live search results.

## 0.1.5 — 2026-11-08

- Connected BookOrbit readers sync position after a pause in navigation and offer to resume changes from another device while the book stays open. The prompt refreshes while open and checks the latest position again when selected.

- The BookOrbit dashboard shares the shelf view. Press V or click the view button to switch between covers and book lists.

- Covers, illustrations and scanned pages use terminal graphics in supported tmux sessions, including Kitty.

## 0.1.4 — 2026-11-08

- Press `%` to toggle a dim page count and reading percentage in zen mode. The preference is shared by the text and scan readers and can also be changed in Settings → Reader.
- Text from a BookOrbit reading position starts on the first line in BOTUI, even when the saved word falls midway through a terminal line. Page turns that land between paragraphs save the first visible passage.
- The EPUB and FB2 bookmark list (`G`) shows a passage from the book alongside the BookOrbit bookmark title.

## 0.1.3 — 2026-11-08

- The text reader preserves nested EPUB and FB2 contents. Fold a book or part, jump to a chapter within it, or open supplementary EPUB notes directly from the outline.

## 0.1.2 — 2026-11-08

- Read image-only EPUBs as scanned books. Each scan fills its page, with spreads, continuous scrolling, zoom and local resume. Large albums no longer cycle through thumbnails while idle.
- Open EPUBs whose chapters use a self-closing XHTML title. These editions could appear empty even when they contained readable text.

## 0.1.1 — 2026-11-08

- Panel, dialog and book card outlines follow the active theme’s accent color. Focused navigation and selected cards use a stronger outline.
- In grid view, the selected title has an accent-colored background, including when borders are hidden.
- Create personal collections, add or remove books, and delete a collection with confirmation. BookOrbit saves changes to your account. Local collections work across folders on this device.
- BookOrbit offline downloads now shows progress in **Currently reading** even for books first opened in the connected library.
- When both the account and the offline copy have a new reading position, choose which one to keep when opening the edition in BookOrbit.
- Remove individual files from BookOrbit or OPDS downloads. Pending BookOrbit progress survives removal of the file.
- Native release builds strip symbols to reduce executable size. The guides use `botui` by name for reader commands.
- The architecture and verification checklist are available in English and Russian. Each README links to the matching language. The standalone upstream comparison has been removed.
- Publishing a tagged release triggers the BOTUI package build in omvoid-repo.

## 0.1.0 — 2026-11-07

Read EPUB, FB2, PDF and DjVu from a local library, an OPDS catalog or a BookOrbit account. Switch between a single page, a two-page spread and continuous scrolling.

- Browse books by cover or in a list, search local and BookOrbit libraries as you type and resume a saved reading position. The BookOrbit dashboard includes current reads, recent additions, authors and series.
- The first time you read a BookOrbit edition, BOTUI saves it on your device. Continue from **BookOrbit offline downloads** without a connection and sync your place when you reconnect.
- Navigate chapters and footnotes, search inside books, set bookmarks and export passage notes. For scanned documents, adjust page width, rotate pages and switch between original colors, inverted colors and paper matching the reading pane. Page turns prefetch both directions, and Kitty scrolling reuses the page image.
- Open cover art in your system image viewer from a book card or reading menu. Missing covers use BOTUI’s cloth jacket with a foil-stamped sprig. The source chooser selects the local library first and keeps shortcut hints in place as you browse. The pixel owl follows the interface palette, with Catppuccin Mocha as the default.
- Choose a terminal palette or a built-in theme and set page width, gutter, margins and first-line indentation. The keyboard guide and settings are also accessible with a mouse.
- Download static Linux binaries for x86_64 and aarch64, with SHA-256 checksums. For a native build, `mise run install` places the executable in `./dist/botui`. The English and Russian guides cover installation and reading controls.
