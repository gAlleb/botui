# Changelog

## Unreleased

- Remove service-specific dictionary/translation adapters. Keep Free Dictionary and Google Translate enabled as editable HTTP profiles in the default TOML, and migrate old named providers once using declarative conversion data. Language controls follow profile parameters instead of service names.
- PDF/DjVu themed paper and ink now use the reading theme, independently of interface colors. Preserve original/inverted modes, saturated image colors and opacity controls; refresh the open page after changing the reading theme in F2.
- Right-click a word or selection in EPUB/FB2 and text-layer PDF for Copy, Highlight, Search, Translate, Define and Add note. Preserve existing selections when clicking inside them; load PDF text lazily before showing the menu.

- Finish EPUB/FB2 Visual selection after saving an annotation; restore `gv` with the cursor on the final selected character. PDF annotation lists accept pasted searches, uppercase action keys and Home/End/PageUp/PageDown, and export the filtered list consistently with EPUB/FB2.
- Connection tests always contact the provider instead of reusing cached results. Local dictionary commands preserve literal placeholder text inside a selection. Choosing the same connection preserves the language selected in the lookup window.
- Explain minimal and optional dictionary fields, legacy adapters and generic HTTP profiles in both READMEs. Keep all configuration examples identical and add a tested Merriam-Webster Collegiate short-definition template.

- Restore direct `q` / `Q` back navigation in ordinary EPUB/FB2 and PDF/DjVu reading. Closing a book returns to its source; Space submenus still go back one level without closing the book. Q also closes page-width dialogs.

- Restore direct PDF/DjVu zoom with `+`, `=` and `-`. Keep `1 / 2 / 3` for single-page, spread and scroll in ordinary reading; digits remain command counts in Vim.

- Dictionary and translation lists start empty. Add an HTTP API, OpenAI-compatible API or local command, with an editable name and optional description. Generic HTTP connections expose method, query, headers, JSON body and response extraction in F2 and TOML; existing service-specific profiles remain compatible.
- F2 and its nested editors consistently use the interface palette, then restore the reading palette. Settings and dialogs share horizontal padding and aligned label/value columns, including connection counts.
- Explanation language is configured on an OpenAI-compatible dictionary connection. Deleting the last connection keeps the list empty after restarting.

- Older TOML files gain missing dictionary/translation settings, Space hint delay, wheel step and OPDS list on startup, preserving existing values and comments.

- Split dictionary settings into Word definitions and Translation, each with add/edit/test/delete, service-specific fields, visible default addresses, default selection and unsaved-change handling. Move Space menu delay to Reader settings.

- Complete the Space migration in readers: legacy command letters no longer act outside Vim, `v/V` select only after `Space c`, and `Space t` changes PDF colors. Escape opens the reading menu consistently in EPUB and PDF. Return from Space submenus with `Q`, `Esc` or `Backspace`. Explicit EPUB/FB2 key overrides do not override Space or Vim.
- Publish complete reading and Vim command tables in both READMEs and guides, checked against the command registry. Update help, footer hints, CLI help and the TOML example.

- Reader popups for dictionaries, highlights and notes retain the reading palette. Vim j/k move by visible text rows; braces move between paragraphs.

- Space opens a contextual command menu with a configurable short delay. Space c toggles an optional Vim cursor; Normal supports yank operators and counts, Visual selections, word/line motions, search and named marks. Escape clears a selection or pending command; another Escape hides the cursor and returns to reading. The cursor stays hidden during ordinary reading.
- Separate dictionary and translation connections, source/target languages, local commands and configurable APIs. Results can be copied or saved as passage notes. Connections, API keys or key commands are editable in F2 and TOML.
- Named OPDS catalogs and credentials are configurable in `[[opds]]` and F2 → Library, alongside the existing BookOrbit and reader settings.

- PDFs with embedded text support word selection with the mouse or keyboard, copying, colored highlights and multiline comments. The **Space a a** panel searches, edits, deletes and exports notes to Markdown. Saved highlights stay attached to the page through zoom, rotation and reopening. Text extraction runs on demand; image-only scans are not OCRed.
- Saving a completely cleared comment deletes the annotation. Action buttons keep consistent brackets and spacing, wrap whole in narrow windows and support mouse clicks, including **Esc Clear**.
- EPUB and FB2 support mouse and keyboard text selection, exact clipboard quotes, five highlight colors and multiline comments. Annotations keep their source ranges through resizing, survive a restart and can be searched, edited, deleted and exported to Markdown. Existing passage notes remain accessible.
- Click the text to place a cursor, drag to select, double-click a word or triple-click a paragraph. **Space c** enables Normal; **v / V** select text or a logical line, **w / b / e** move by words and **m a / ' a** save and return to named mark `a`. **Space m / Esc** retain the reading menu; text clicks no longer open it. **K** runs a configured dictionary command for the selection.
- Continuous scrolling now moves one line per mouse-wheel tick in EPUB, FB2, PDF and DjVu. Change **F2 → Reader → Mouse wheel step** or `scroll_lines` to choose 1–12 lines. Arrow keys remain single-line and Page Up / Down remain screen-sized.

- PDF and DjVu zoom keeps the current page visible while a sharper image renders in the background. Reducing or repeating a zoom level reuses the larger cached image, including in spreads and continuous scrolling. A failed refinement preserves the readable page.

- **Q** now goes back one step in BookOrbit and the local library. Closing a book returns through its card to the original shelf, dashboard or search; local folders go up to the library root before leaving. Returning keeps the selected item and scroll position, including after refreshing reading progress. Back waits for an outstanding progress save instead of cancelling it.

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
