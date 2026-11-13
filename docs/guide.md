# BOTUI user guide

**English** · [Русский](guide.ru.md)

[README](../README.md)

## First run

```sh
botui                                      # choose a source
botui --library "$HOME/Books"              # remember a local library folder
botui --local                              # reopen the saved folder
botui "./examples/first-light.fb2"         # open a file directly
botui --resume                             # continue your last unfinished local book
botui --server https://books.example.org   # BookOrbit installation root
botui --opds https://books.example.org/opds
botui --config-path                        # create or locate the config file
```

On the start screen, move the selection to see the description and controls for that source. The local library is selected at startup. Press **Enter** or **L** to open it, **B** for BookOrbit, **P** for OPDS, **D** for BookOrbit offline downloads or **O** for OPDS downloads.

Enter and a click also open the selection.

Click the cover inside a book card, or press **C**, to open it with your system image viewer. On the shelf, clicking still opens the book card. In a local EPUB or FB2, **Space r c** opens the embedded cover.

If a book or catalog has no cover image, BOTUI makes a [cloth jacket](images/generated-jacket.png) from the title and author. For PDF and DjVu, **Space r c** shows the first page. BOTUI uses the desktop’s image association via `xdg-open`.

**F2** opens Settings: Appearance, Reader, Library, BookOrbit, Dictionaries, Keys, About and Legal. **F1** (or **?** on a shelf) opens the shortcut guide for the current screen. Use **[ / ]** to switch groups or type **/** to find a key or command. The footer shortcuts are clickable.

**Q** on shelves and during ordinary reading go back one step. In BookOrbit: reader → book card → the shelf, dashboard or search you opened it from. In the local library: reader → folder → parent folders → library root → source chooser. Selection and scroll position are kept when you return. On the source chooser, **Q** exits BOTUI. When launched with a single file path, closing that file ends the command.

**Esc** also goes back on library screens; in a reader it opens the reading menu. In a search field, **Q** is typed into the query and **Esc** closes the search. Click **Home** on a BookOrbit shelf to go straight to the source chooser.

### Local library

**Local library** uses your saved root, then `~/Books`, then the current directory. Click **P Folder…** to paste a path. **R Set library here** remembers the folder currently on screen. **/** opens a centered search window over the shelf and searches the entire library.

Results update as you type; use **↑/↓** and **Enter** to open a book, or **Esc** to return to the shelf. Partial words and unaccented letters work: `kukai` finds `Kūkai`. Filenames appear first while EPUB/FB2 titles, authors and series are indexed in the background.

**2** opens reading history with percentages and **3** shows recent files by creation time, falling back to modification time.

In a wide terminal the left pane shows the folder tree, currently reading and recently added. Click a folder name to display its books on the right; click its arrow to fold or expand it. **F6** switches focus, **← / →** fold or expand, and **Enter** opens a folder.

Drag the divider to resize it. Small windows keep the numbered tabs. [See the local library in Kitty](images/local-library.png).

## Managing collections

| Action | Local library | BookOrbit |
| --- | --- | --- |
| Create a collection | Press **4**, choose **New collection…**, enter a name and press **Enter**. | Press **5**, then **N**, enter a name and press **Enter**. |
| Add a book | Select a file, press **A** and choose **Add to** beside the collection name. | Open the book card, press **A Collections** and choose **Add to**. |
| Remove a book | Select the file, press **A** and choose **Remove from**. | On the book card, press **A Collections** and choose **Remove from**. |
| Delete a collection | Press **4**, open the collection and press **Del**. | Press **5**, select a collection you own and press **Del**. You can also do this inside the open collection. |

You can also click **Del Delete collection**. Check the name in the confirmation and choose **Delete collection**. **Cancel** is selected by default.

A book can belong to several collections. Deleting a collection keeps its books in the library and in their other collections. Local collections are stored on this device. BookOrbit saves collection changes to your account. Refresh the web library to see them.

## BookOrbit

Choose **Connect to BookOrbit** to sign in. The dashboard shows continue reading, recent additions and discovery shelves. Reading streaks, yearly goals, challenges and Reading DNA appear when the server supplies those widgets. Select a book to choose its file.

Use **0** to revisit and refresh the dashboard.

| Key | Section | Key | Section |
| --- | --- | --- | --- |
| 0 | Dashboard | 4 | Libraries |
| 1 | All books | 5 | Collections |
| 2 | Currently reading | 6 | Authors |
| 3 | Recently added | 7 | Series |

**Authors** and **Series** show account-wide totals and the number of books in each entry. Open an author to see their books, or a series to read its volumes in server order. **/** opens a centered search window. Results update as you type; **Esc** or a click outside closes it.

In **Authors** and **Series** it filters that directory. On the dashboard or a book shelf, it searches your whole BookOrbit catalog by title, author or series. Try `желз` for `Желязны`, or `znak edin` for `Знак Единорога`.

Results appear while BOTUI fetches catalog metadata in the background. Select with **↑/↓** and press **Enter**, or click a result. **Esc** from a book card restores the query and selection.

**V** switches between cover grid and list on the dashboard and shelves. Both use the same saved view. **Tab / Shift+Tab** change sections in display order. Arrows, the wheel and Page Up / Page Down navigate a shelf. More items load as you scroll. Enter or a click opens the selection. **Esc** goes back.

### Offline reading and sync

**Read** downloads a BookOrbit edition on first use and keeps it on this device. Open it from its card again to read the saved file and load the account’s reading position and text bookmarks. **D Save offline** downloads ahead of time or checks a saved copy for updates.

While reading a connected BookOrbit edition, pause for two seconds after turning a page to sync your place. Returning to the terminal checks for progress from another device. Choose **Go to BookOrbit position** to jump there, or **Stay here** to keep reading at your current spot. BOTUI also checks every 15 seconds while idle. The open prompt keeps checking for changes. Choosing **Go to BookOrbit position** fetches the latest saved location again.

If BookOrbit reports a changed edition, BOTUI checks the server version before replacing the file.

Without a connection, select **BookOrbit offline downloads** on the start screen. If you left an EPUB at chapter 5 online and read to chapter 7 offline, closing it queues chapter 7 for the same account and edition.

BOTUI retries while running with an active session or a configured password or password command. Pending progress survives a restart. If you enter your password manually, signing in sends queued progress. If both positions changed, open the downloaded edition in the connected BookOrbit library. Choose **Use offline position** to send your place to BookOrbit, or **Use BookOrbit position** to resume at the server’s place. **Esc** keeps both positions for later.

On either download shelf, select a file and press **Del** to remove its copy from this device. BookOrbit keeps the server edition, and BOTUI keeps queued reading progress for the next sync.

Offline bookmarks are local. **OPDS downloads** has its own shelf and uses device-local progress.

Wide screens have a separately framed navigation tree. Click **Libraries** or **Collections** to fold them, or press **F6**, move with **↑/↓** and fold or expand with **←/→**. **4 / 5** open the full directories. Folded groups are remembered.

### Book details

Before opening an edition, its card shows the author, series, synopsis, publisher, date, language, ISBN and reading progress when supplied by the server. **R** reads the selected edition, **D** saves it offline and **I** opens the full synopsis.

**H Put on hold** removes a book from active reading and keeps your place; **H Resume reading** puts it back. **X Reset reading state** requires confirmation, clears personal progress across the book’s editions and changes it to Unread on BookOrbit. It keeps files, bookmarks and notes.

Reaching the end sends 100%; BookOrbit applies its own completion policy. Use `botui --check-bookmarks "Title"` to check edition-specific bookmark addresses without changing progress.

See the [book details before reading](images/book-details.png) and [navigation with the libraries folded](images/navigation-folded.png).

**L Reading log** opens sessions recorded by BookOrbit, with dates, devices, time and progress. **B Related books** opens the author or series. The local timer under **Space r s** counts time in BOTUI on this device.

### OPDS catalogs

**OPDS** accepts OPDS 1 Atom and OPDS 2 JSON. Browse navigation, groups and facets; choose an edition, search through an advertised template or OpenSearch, and download direct acquisitions. Use HTTPS for HTTP Basic accounts. Loan-only and DRM entries remain catalog information.

Positions and notes are local.

## Formats

| Format | Reading features | Runtime requirements |
| --- | --- | --- |
| EPUB / FB2 | Reflowing text, contents, illustrations, search and passage annotations. EPUB also renders styled tables, MathML/marked TeX and static SVG. | Built in |
| Markdown (`.md`, `.markdown`) | Headings, lists, quotes, task lists, tables, code highlighting and local images. **F9** switches formatted/source views; selection and notes work in both. | Built in |
| PDF | Fixed pages, contents, zoom, rotation and page notes. A text layer enables occurrence search, word selection and highlights. | Poppler utilities (`pdfinfo`, `pdftoppm`, `pdftotext`), or Python 3 with Poppler GI and Cairo |
| DjVu | Fixed pages, contents, zoom, rotation and page notes; search when a text layer is available. | DjVuLibre (`djvused`, `ddjvu`, `djvutxt`) |
| CBZ / CBR | Image pages in natural filename order, contents, zoom, rotation, bookmarks and page notes. | Built-in ZIP/RAR decoders; no external unpacker |

PDF contents require Python 3, PyGObject and the Poppler GI bindings. Image-only EPUBs use the fixed-page reader. There is no OCR: scans and comic images do not provide word selection, definitions or translation.

Comic pages may be JPEG, PNG or WebP, up to 16 MiB each. Password-protected and multi-volume RAR archives are not supported. Files with a `.cbz` or `.cbr` extension are recognized as ZIP or RAR by their contents, so mislabeled archives also open without renaming.

Kitty displays covers and pages as images, including inside tmux with graphics passthrough. Terminals without an image protocol can use colored half-blocks. [Detailed format limits](checklist.md#coverage-limits)

## Reading

Press **Space**, then the command keys **in sequence**. Menu hints appear after 150 ms by default; commands work immediately. In a submenu, **q / Esc / Backspace** go back; Escape closes the root menu.

Books open without a cursor. **Space c** enables Vim navigation and selection; **v / V** select only in this mode. Escape cancels a pending command or selection, then returns from idle Normal mode to ordinary reading. **Space c** also returns to reading directly. Explicit `[keys]` overrides apply to ordinary EPUB/FB2 reading and do not replace Space commands or Vim motions.

Commands depend on the document: text actions need selectable text; page zoom, rotation and page notes apply to PDF, DjVu, comics and image-only EPUBs. Text-reader commands below also apply to Markdown unless a format is named explicitly. In ordinary reading, q/Q returns to the source; in Vim use Space q.

| Keys | Action |
| --- | --- |
| `Space c` | Toggle Vim cursor (Normal ↔ reading) |
| `Space m` | Reading menu |
| `Space o` | Contents |
| `Space z` | Zen mode |
| `Space p` | Dim progress footer in zen |
| `Space f` | Find in book |
| `Space b a` | Add/remove bookmark |
| `Space b l` | Bookmark list |
| `Space a y` | Copy selection; Normal: word under cursor |
| `Space a h` | Color highlight |
| `Space a n` | Comment on passage |
| `Space a a` | Highlights and notes |
| `Space a e` | Export annotations to Markdown |
| `Space a s` | BookOrbit annotation sync status / retry |
| `F9` / `Space r v` | Markdown: formatted view / original source |
| `Space a v` | Show/hide note body (text reader) |
| `Space a p` | Page note (PDF/DjVu/comics) |
| `Space d d` | Word definition |
| `Space d t` | Translate selected text |
| `Space l 1` | Single page |
| `Space l 2` | Two-page spread |
| `Space l 3` | Continuous scroll |
| `Space l w` | Text width / column gap; fixed-page zoom / spread gap |
| `Space l f` | Opaque/transparent background |
| `Space l u` | Border style |
| `Space t` | Theme picker with live preview; fixed pages also have Original / Inverted |
| `Space s` | Settings |
| `Space ?` | Help |
| `q` / `Q` / `Space q` | Close book and return to its source |
| `Space r c` | Open cover in external viewer |
| `Space r s` | Reading statistics |
| `Space r [` | Jump back |
| `Space r ]` | Jump forward |
| `Space r i` | Links and illustrations (text reader) |
| `Space r f` | Follow visible links (text reader) |
| `Space r p` | Go to page (PDF/DjVu/comics) |
| `Space r +` | Zoom in (PDF/DjVu/comics) |
| `Space r -` | Zoom out (PDF/DjVu/comics) |
| `Space r 0` | Fit page / fit width in scroll (PDF/DjVu/comics) |
| `Space r r` | Rotate clockwise (PDF/DjVu/comics) |
| `Space r R` | Rotate counterclockwise (PDF/DjVu/comics) |

**Direct navigation in ordinary reading**

| Keys | Action |
| --- | --- |
| `← → / PgUp PgDn` | Navigate pages; arrows also pan fixed pages |
| `↑ ↓ / j k` | Move through text; one line in continuous text scroll |
| `h l` | Previous/next page or horizontal movement |
| `1 / 2 / 3` | Single page / spread / scroll; in Vim: command count |
| `+ / = / -` | PDF/DjVu/comics: zoom in / zoom in / zoom out |
| `Home End` | Start/end (fixed pages: document; Vim: line) |
| `Tab` | Focus visible contents; arrows fold/expand, Enter opens |
| `F1 / F2` | Help / settings |
| `F3 / F4 / F5` | Next match / previous match / result list |
| `Esc` | Back/close panel; reading menu |
| `Ctrl+C` | Copy selected text |

PDF search highlights each occurrence and brings it into view, including multiple matches on one page. In Vim mode, `n` follows the search direction and `N` reverses it. Search requires an embedded text layer; image-only scans need OCR outside BOTUI.

**Vim — after Space c**

| Keys | Action |
| --- | --- |
| `Space c` | Enter Normal; repeat to return to reading from Normal or Visual |
| `v / V` | Visual / logical line selection |
| `Esc` | Cancel pending command or Visual; idle Normal → reading |
| `h j k l` | Cursor left/down/up/right; j/k follow visible lines |
| `w b e` | Next word / previous word / word end |
| `0 ^ $` | Logical line start / first text / end |
| `{ }` | Previous/next paragraph (PDF: text block) |
| `gj gk / g0 g$` | Screen rows / screen row edges (text reader) |
| `gg / G` | Start/end of text (PDF: current page) |
| `y` | Copy selection in Visual; begin copy operator in Normal |
| `yw ye yy y$` | Copy to next word / word end / logical line / line end |
| `yiw yaw yip yap` | Copy inner word / word with space / paragraph |
| `3j / 3yw / y3w` | Repeat a motion or copy operation; counts can precede the operator or motion |
| `viw / vaw / vip / vap / gv` | Select inner word / word with space / paragraph / restore previous selection |
| `/ ? / n N` | Search forward/backward / next/previous match |
| ``ma / 'a / `a`` | Set named mark a / return to it |
| `Ctrl+O / Ctrl+I` | Jump back / forward |
| `K` | Definition of word/selection |

**Inside panels and editors**

| Keys | Action |
| --- | --- |
| `↑ ↓ / Enter / click` | Choose and activate an item |
| `Esc` | Close panel; editor offers to save/discard unsaved changes |
| `Ctrl+S / Enter` | Note editor: save / new line; clearing the comment keeps its highlight; no text and no color deletes the annotation |
| `Tab` or click a color | Note editor: highlight color; ✓ marks the current choice |
| `[ ] / { }` | Text layout panel: narrow/widen page / decrease/increase column gap |
| `/ / N / H / Y / Del / E` | Annotation list: filter / edit / color / copy / delete / export |
| `P / L / S / Y / N` | Lookup result: provider / target language / source language / copy / save as note |

### Tables, equations and illustrations

Open a technical EPUB as you would a novel. The reader applies styles packaged with the edition, including merged table cells, side-by-side columns, lists and preformatted code. Equations in presentation MathML and TeX explicitly marked as math appear in the passage. A fraction has its numerator above its denominator. Change the page width with **Space l w** to give a wide table more room.

The terminal supplies the typeface and cell size. Justified paragraphs keep natural spaces between words. Headings and right-aligned passages keep their alignment. The reader theme supplies the ink and paper colors.

Search, footnotes, bookmarks and BookOrbit resume use the source text even when a table wraps differently.

Embedded raster illustrations use the available page height. **Space r i** opens an illustration separately. **F2 → Reader → Illustrations** switches to the separate viewer only.

FB2 also preserves italic, bold and struck-through text in ordinary paragraphs. These faces do not change the text used for search or saved positions.

### Working with text

Drag to select text, double-click a word or triple-click a paragraph (a text block in PDF). Right-click a word or selection for **Copy, Highlight, Search, Translate, Define, Add note** in a small menu beside the text. Right-clicking inside a selection keeps the whole passage. Escape or a click outside closes the menu.

Click a saved highlight to open its comment, or choose **Add note** from its context menu. Choose a color with the mouse, enter a comment and click **Save** or press **Ctrl+S**. Enter inserts a newline. Clearing a comment keeps the highlight; deleting both text and color removes the annotation.

The highlight and comment editor offers all ten BookOrbit preset colors, with colored names and swatches. Imported custom RGB colors keep their exact value. Click a color or use Tab / Shift+Tab; **No fill** keeps a comment marked by an underline without a colored background.

**Space a a** opens the searchable list of highlights and notes. Select an entry to see its text and position in the book/section or its page number; use **Go**, **Edit**, **Color**, **Delete** or **Export**. **Space a p** adds a page note to a fixed-page document. Notes and highlights remain available locally. When you open an edition from a connected BookOrbit shelf, BOTUI synchronizes passage comments, colors and deletions in both directions: text uses exact CFI ranges, and PDF text highlights use page coordinates. Changes made offline are sent when you reopen that edition through BookOrbit. **Space a s** shows synchronization status and offers **Sync now / retry unconfirmed uploads**.

Concurrent edits keep both versions. A lost upload response is reconciled against the server before another create is attempted. Marks that cannot be located exactly stay on their original side; status reports them. Whole-page notes, selections spanning PDF pages or EPUB spine documents, and formats without a usable CFI stay local. Browser styles and colors outside BOTUI’s five-color palette are preserved on the server when only the comment is edited; BOTUI displays the closest available highlight color.

**Define** and **Translate** use the connections selected in [Configuration](#dictionaries-translation-and-saved-catalogs). They send the chosen text only when requested. Keyboard equivalents and Vim motions are listed below.

### Dictionaries, translation and saved catalogs

**F2 → Dictionaries** separates **Word definitions** and **Translation**. Free Dictionary and Google Translate are enabled by ordinary HTTP profiles in the default TOML; both can be edited or deleted. Add a connection, give it your own name and optional description, then configure **HTTP API**, **OpenAI-compatible API** or **Local command**. Existing connections can be edited, tested, selected as default and deleted. Deleting the last connection leaves the list empty after restarting.

The HTTP editor exposes the address, method, query parameters, headers, JSON body and response text path. Use `{text}`, `{source}`, `{target}` and `{key}` in templates. A path such as `/items/*/text` collects strings from array items; the separator joins them. Plain-text responses are also supported. This covers JSON/text HTTP services; other protocols or authentication flows can be implemented through a local command. Service examples live in [config.example.toml](../config.example.toml), not in the Add menu. Older service-specific configurations remain readable.

Choose **Save changes & back** to keep edits. Leaving unsaved edits offers Save, Discard or Keep editing. Local programs and password-manager commands have separate program and argument fields. Descriptions belong to your configuration: clear one to hide it. API keys are masked.

Source and output languages are below the **Translation** connection list. **Explanation language** belongs to an OpenAI-compatible dictionary connection. The result window offers **P** to change connection, **L/S** to change target/source, **Y** to copy, **N** to save a passage note, and **Esc** to close. **Space menu delay** is under **F2 → Reader** and defaults to 150 ms; it delays hints, never commands.

Requests run only when invoked, with a 20-second limit and a bounded in-memory result cache. An external service receives the chosen word/passage; no automatic background translation occurs. Closing the window dismisses the request; an HTTP request already sent may finish in the worker. No provider is silently substituted. API keys can come from a plain value or an argument-array command; the command takes precedence. Keys are masked in settings.

Everything is in the same `config.toml`: `[lookup]`, `[[lookup.profiles]]`, `[[opds]]`, `[bookorbit]`, reader and appearance fields. [The example config](../config.example.toml) documents each provider and credential option. Changes made in settings preserve unrelated values and comments. Existing `lookup_command` remains compatible.

**F2 → Library → OPDS catalogs** manages named catalogs, addresses and optional credentials. They appear when choosing OPDS on the home screen; `botui --opds URL` also uses a saved connection with the same URL. Public catalogs need no credentials.

### Fixed pages: PDF, DjVu, comics and image-only EPUBs

These formats share zoom, rotation, spread and scroll controls. **Space l w** opens zoom and spread spacing. The **[ / ]** keys or gap buttons set **0–32 cells**; zero joins facing pages. `scan_gap` remembers the choice independently of text columns.

**+ / −** or **Space r + / −** change zoom, including below 100%. In single-page and spread modes, 100% fits the page; enlarging fills the available margins and clips only at the reader edge. In scroll mode, 100% fits the sheet width. Arrows pan enlarged pages; **Space r 0** restores fit and **Space r p** opens a page-number prompt. **Space r r / R** rotates the page.

**Space t** opens a theme picker over the page with Original, Inverted and reading palettes. See [Themes](#themes). **Space o** opens contents; arrows fold/expand entries. Scroll mode separates sheets with a dotted line.

Search needs a text layer. PDF also supports word selection and highlights; their coordinates follow zoom and rotation. Extraction order may differ from visual order in complex multi-column PDFs. **Space a p** creates a page note, including in comics without text.

BOTUI saves position within a sheet locally; BookOrbit receives the page number.

### Markdown: formatted and source views

Open `.md` or `.markdown` like other supported files. The reader displays headings with a contents tree, emphasis, quotes, ordered/unordered lists, task boxes, tables, fenced code and local illustrations. **F9**, **Space r v**, or the **Source / Formatted** footer button switches between the formatted document and its original source. Both are reading views; the file is not edited.

Headings align left and distinguish levels 1–6 with numbers and colors. Code panels show their language; quotes have a side rule, and tables have shaded headers and row separators. Lists retain numbering, bullets and nesting. NOTE/TIP/IMPORTANT/WARNING/CAUTION render as labeled, colored panels. Colors and backgrounds follow the light or dark reading palette. Source view also highlights Markdown; glyph size remains the terminal font size.

Mouse selection, Copy, Search, Translate, Define, highlights, comments, bookmarks and Vim navigation work in both views. Notes keep the same source addresses when switching views, changing width or reopening the file. Formatted copy omits Markdown delimiters, and phrase search crosses inline formatting. Source view copies the original characters. As with other book formats, changing the file externally can change saved passage positions.

Code uses a small built-in lexical highlighter for Markdown, Rust, Python, JavaScript/TypeScript, JSON, TOML, shell, C/C++, Go, Java, YAML and SQL. Unknown languages still get a readable code block; no syntax package needs installing. Very large code blocks are rendered without token coloring. Local images resolve relative to the Markdown file and must stay inside its directory (subdirectories are supported); remote images are not downloaded. Raw HTML is shown as text. This is a CommonMark reader with tables, strikethrough and task lists, not an MDX or Obsidian plugin runtime. Markdown text is limited to 4 MiB and each illustration to 16 MiB.

## Configuration

Run `botui --config-path` to print the exact path. The default is `~/.config/botui/config.toml`. The same TOML file holds the library root, interface, reader bindings and BookOrbit account. The interface is in English. See the [commented configuration](../config.example.toml).

On startup, older configs gain missing dictionary/translation defaults, the Space hint delay, mouse-wheel step and an empty OPDS list. Existing values, connections and comments are preserved.

In **F2 → Reader**, **First-line indent** moves the first line of a paragraph to the right by 0–8 cells. **Paragraph spacing** sets the vertical gap between paragraphs in EPUB and FB2. **From book** follows the EPUB layout and uses one blank line for plain FB2 prose. Choose **0 blank lines**, **1 blank line** or **2 blank lines** to set your own gap. The choice also applies to the book you have open.

In an open book, drag the vertical border between the contents on the left and the page to change the width of the contents sidebar. You can also enter a width in **F2 → Reader → Contents width**. On the bookshelf, drag the border between navigation and the book list or use **F2 → Library → Sidebar width**.

**F2 → Library → Download limit** sets the maximum size per BookOrbit or OPDS edition. The default is **4096 MiB**. Set **0** for unlimited downloads. Downloads stream to disk.

[Keyboard help](images/help.png).

### Themes

**F2 → Appearance → Interface theme** controls shelves, settings and other interface screens. **Space t** changes the reading palette over the open page: ↑/↓ preview a palette, ←/→ choose light/dark, Enter or a click applies, and Escape cancels. The interface starts in Catppuccin Mocha; **Terminal** follows your terminal’s colors.

For PDF, DjVu and image-only EPUB, the picker also offers **Original** and **Inverted**. Reading palettes recolor neutral paper and ink while retaining saturated illustration colors. The choice is saved in `scan_theme`, also editable through **F2 → Reader → PDF / image theme**. Text reading uses its own `theme` setting.

**CBZ/CBR artwork always keeps its original colors.** In comics, **Space t** previews only the surrounding background; **F2 → Reader → Comic background** edits the same `comic_background` setting. PDF inversion and recoloring do not affect comics.

**Space l f** switches opaque/transparent page backgrounds; window opacity belongs to the terminal.

### Configuration file

```toml
# Local library. Uncomment to use a specific folder.
# library_root = "/path/to/books"
local_grid = false
# Width of the navigation sidebar on the bookshelf, in cells. Omit for Auto.
# sidebar_width = 28
# Width of the contents sidebar in an open book, in cells. Omit for Auto.
# contents_width = 24
# Per downloaded edition, in MiB (BookOrbit and OPDS). 0 removes the limit.
max_download_mib = 4096

# Reader defaults. Use "terminal" to follow your terminal's colors.
theme = "terminal"
# PDF/DjVu and image-only EPUB: original, inverted, terminal, or a reading
# palette ID such as sepia-light or ember-dark. F2 and Space t save the same choice.
scan_theme = "original"
# CBZ/CBR background only; artwork always retains its original colors.
# Use terminal or a reading palette ID. F2 and Space t save the same choice.
comic_background = "terminal"
interface_theme = "catppuccin-mocha"
# Optional override. By default, solid Terminal paper follows the active terminal background.
# opaque_color = "#1f2125"
zen = false
# Show a dim page count and percentage in zen mode. Toggle with Space p.
zen_progress = false
# PDF/DjVu zoom in percent. In scroll mode 100 fits the reading pane.
scan_zoom = 100
# Space between PDF/DjVu/comic pages in a spread (0–32 terminal cells).
scan_gap = 2
# Preferred width of one text page and the space between facing pages, in terminal cells.
# page_width = 78
column_gap = 4
# First line of a prose paragraph in EPUB/FB2, in cells (0–8; 0 disables).
paragraph_indent = 0
# Blank lines between EPUB/FB2 paragraphs (0, 1 or 2). Omit to follow the book.
# paragraph_spacing = 1
# Default layout for books without a saved mode: auto, single, spread, scroll.
reading_layout = "auto"
opaque_background = false
# Window frames, cover cards and jackets: "rounded", "square", or "none".
borders = "rounded"
preserve_printed_lines = false
hide_inline_images = false
# Lines per mouse-wheel tick in continuous scroll (EPUB/FB2/PDF/DjVu, 1–12).
scroll_lines = 1
# Delay before the Space command menu appears. Fast sequences execute immediately.
leader_delay_ms = 150
# Legacy lookup_command is still read; use lookup.profiles for new configurations.

[bookorbit]
# Remember which navigation groups are folded on a wide screen.
# libraries_collapsed = false
# collections_collapsed = false
server = ""
username = ""
list_view = false
# Uncomment one way to sign in with this account.
# password_command = ["pass", "show", "bookorbit"]
# password = "your password"

# Reader commands use Space; Space c toggles the optional Vim cursor.
# Direct reader navigation and Vim mappings are documented in README and F1.
# Optional explicit single-letter overrides for EPUB/FB2 reading only.
# They never override Space commands or Vim motions. No overrides by default.
[keys]
# x = "next"
# Supported action names:
# next, previous, menu, zen, zen-progress, background, borders, cover, resources,
# statistics, note, notes, export-notes, jump-back, jump-forward, theme, contents,
# search, search-results, next-match, previous-match, bookmark, bookmarks,
# single, spread, scroll, page-layout, wider-gap, narrower-gap,
# wider-margins (alias narrower-page), narrower-margins (alias wider-page), close

# Multiple OPDS catalogs. Credentials are optional for public catalogs.
# [[opds]]
# name = "My catalog"
# url = "https://books.example.org/opds"
# username = "reader"
# password_command = ["pass", "show", "books/opds"]
# password = "plain-text alternative"

# F2 → Dictionaries has two independent lists: Word definitions and Translation.
# Add/edit/delete a profile in its list, then Save changes & back.
# These names select the default connection in each list; names must be unique.
[lookup]
dictionary = "English dictionary"
translation = "Google Translate"
source_language = "auto"
target_language = "ru"

# Active defaults are ordinary editable HTTP profiles, not built-in providers.
# Remove a block to remove a connection. Set profiles = [] under [lookup]
# only if there are no [[lookup.profiles]] blocks and you want an empty list.
[[lookup.profiles]]
name = "English dictionary"
purpose = "dictionary"
provider = "http"
endpoint = "https://api.dictionaryapi.dev/api/v2/entries/en/{text}"
response_path = "/*/meanings/*/definitions/*/definition"
response_separator = "\n"

[[lookup.profiles]]
name = "Google Translate"
purpose = "translation"
provider = "http"
endpoint = "https://translate.googleapis.com/translate_a/single"
query = { client = "gtx", sl = "{source}", tl = "{target}", dt = "t", q = "{text}" }
response_path = "/0/*/0"
# Translation segments must join without extra separators.
response_separator = ""

# Additional recipes below are inactive. Add your own in F2 > Dictionaries or below.
# The active Google endpoint is unofficial; availability is not guaranteed.
# Minimal HTTP profile: name, provider, endpoint, response_path for nested JSON.
# purpose defaults to "dictionary"; method defaults to GET; response_format to json.
# description, query, headers, body, api_key and key_command are optional.
# response_separator defaults to "" (concatenation); use "\n" for definitions.
# command profiles need name, provider = "command" and command instead of HTTP fields.
# OpenAI profiles need name, provider = "openai" and model; endpoint has a local default.
# name is unique; description is optional and editable. purpose selects the group.
# HTTP templates: {text}, {source}, {target}, {key}. URL/query values are encoded;
# JSON string values are escaped safely. Headers are passed as configured.
# response_path uses /object/key/0 and * for array items. Empty = JSON root.
# response_format = "text" reads a plain UTF-8 response instead of JSON.
# Supported methods: GET (default), POST, PUT, PATCH, DELETE.
# Optional JSON source: "{source?}" omits its object field for auto/empty source.
# {source:upper}/{target:upper} uppercase language codes; {source:upper?} combines both.

# Example only: an English definitions API, not an installed connection.
# [[lookup.profiles]]
# name = "My English dictionary"
# description = "English definitions"
# purpose = "dictionary"
# provider = "http"
# endpoint = "https://api.dictionaryapi.dev/api/v2/entries/en/{text}"
# method = "GET"
# response_format = "json"
# response_path = "/*/meanings/*/definitions/*/definition"
# response_separator = "\n"

# Merriam-Webster Collegiate: short definitions, not full entries or audio.
# Get a key for the Collegiate product: https://dictionaryapi.com/register/index
# Official schema: https://dictionaryapi.com/products/json#sec-2.shortdef
# [[lookup.profiles]]
# name = "Merriam-Webster Collegiate"
# purpose = "dictionary"
# provider = "http"
# endpoint = "https://www.dictionaryapi.com/api/v3/references/collegiate/json/{text}"
# query = { key = "{key}" }
# api_key = "YOUR-COLLEGIATE-API-KEY"
# response_path = "/*/shortdef/*"
# response_separator = "\n"
# Alternative to api_key: key_command = ["pass", "show", "dictionaries/collegiate"]
# If the API returns spelling suggestions instead of entries, this path has no text.
# It reports "No text at the configured response path"; try another spelling.

# Example only: a custom JSON translation API. Match your service's schema.
# [[lookup.profiles]]
# name = "My translator"
# purpose = "translation"
# provider = "http"
# endpoint = "http://localhost:5000/translate"
# method = "POST"
# query = { version = "1" }
# headers = { Authorization = "Bearer {key}" }
# body = '{"q":"{text}","source":"{source}","target":"{target}"}'
# response_format = "json"
# response_path = "/translatedText"
# response_separator = ""
# key_command = ["pass", "show", "translation/api"]
# api_key = "plain-text alternative"
# key_command takes precedence over api_key. Keep only the method you use.

# OpenAI-compatible chat completions, including a locally hosted server.
# [[lookup.profiles]]
# name = "My explanations"
# description = "Explain the selected word"
# purpose = "dictionary" # or "translation"
# provider = "openai"
# endpoint = "http://localhost:11434/v1/chat/completions"
# model = "your-installed-model"
# target_language = "ru" # Explanation language; empty uses lookup.target_language.
# api_key = "optional key"

# Offline dictionary program (install it and its dictionaries separately).
# [[lookup.profiles]]
# name = "My local dictionary"
# purpose = "dictionary"
# provider = "command"
# command = ["sdcv", "-n", "{text}"]
# Arguments are passed without a shell. Also available: {source}, {target}.
```

To use a password manager, set `password_command` to an argument array as above. A string runs through `sh -c`. The command must exit successfully and return the password on the first output line. For a stored plaintext password, use `password`. Omit both to type the password at login.

A typed password takes precedence, followed by the command, then the stored password. Saved credentials are bound to the configured server and username. Tokens stay in memory and normal exit closes the server session.

The app creates its config with mode `0600`. On upgrade it imports the earlier JSON preferences, connection and reader bindings, preserving their originals under `$XDG_STATE_HOME/botui/config-backup`. UI changes preserve unrelated TOML values and comments.

| Data | Default location |
| --- | --- |
| Config | `~/.config/botui/config.toml` |
| Positions, local bookmarks, notes and reading history | `~/.local/state/botui/` |
| Offline books and Markdown exports | `~/.local/share/botui/downloads/`, `~/.local/share/botui/exports/` |

The XDG config, state and data environment variables override those defaults. Local reading records use a content hash, so moving a file preserves its position.
