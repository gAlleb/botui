<p align="center">
  <img src="docs/images/mascot.png" width="30%" alt="BOTUI’s owl perched on two books">
  <img src="docs/images/wordmark.svg" width="60%" alt="BOTUI">
</p>

# botui

**English** · [Русский](README.ru.md)

BOTUI is a terminal book reader for Linux. Read books, documents and comics from local folders, your [BookOrbit library](https://github.com/bookorbit/bookorbit) or OPDS catalogs. Supported formats are EPUB, FB2, Markdown, PDF, DjVu, CBZ and CBR.

[Highlights](#highlights) · [Screenshots](#screenshots) · [Installation](#installation) · [First run](#first-run) · [Formats](#formats) · [Reading](#reading) · [Controls](#controls) · [Configuration](#configuration) · [User guide](docs/guide.md)

## Highlights

- Cover grids, collections and library search as you type
- BookOrbit offline downloads and reading-position sync across devices
- Single pages, two-page spreads and continuous scrolling, with adjustable margins, spacing and zoom
- Independent interface and reading themes, live preview and a distraction-free zen mode
- Text selection, colored highlights, editable comments, bookmarks and Markdown export
- Word definitions and translation through editable HTTP, OpenAI-compatible or local connections
- Mouse controls, a Space command menu and an optional Vim cursor
- EPUB tables, equations and illustrations; formatted Markdown with code highlighting and a source view

## Screenshots

Click any image to open it at full size.

<table>
  <tr>
    <td width="50%" valign="top">
      <a href="docs/images/start.png"><img src="docs/images/start.png" alt="Start screen"></a><br>
      <strong>Start screen</strong><br><sub>Catppuccin Mocha · 16 pt</sub>
    </td>
    <td width="50%" valign="top">
      <a href="docs/images/dashboard.png"><img src="docs/images/dashboard.png" alt="BookOrbit dashboard"></a><br>
      <strong>BookOrbit dashboard</strong><br><sub>Gruvbox Dark · 12 pt</sub>
    </td>
  </tr>
  <tr>
    <td width="50%" valign="top">
      <a href="docs/images/bookorbit-search.png"><img src="docs/images/bookorbit-search.png" alt="BookOrbit search as you type"></a><br>
      <strong>BookOrbit search as you type</strong><br><sub>Tokyo Night · 13 pt</sub>
    </td>
    <td width="50%" valign="top">
      <a href="docs/images/reading-spread.png"><img src="docs/images/reading-spread.png" alt="Two-page EPUB spread"></a><br>
      <strong>Two-page EPUB spread</strong><br><sub>Sepia Dark · 12 pt</sub>
    </td>
  </tr>
</table>

<table>
  <tr>
    <td width="50%" valign="top">
      <a href="docs/images/shelves-overview-en.jpg"><img src="docs/images/shelves-overview-en.jpg" alt="Home and libraries"></a><br>
      <strong>Home and libraries</strong>
    </td>
    <td width="50%" valign="top">
      <a href="docs/images/reading-overview-en.jpg"><img src="docs/images/reading-overview-en.jpg" alt="Reading modes"></a><br>
      <strong>Reading modes</strong>
    </td>
  </tr>
</table>

## Installation

### Prebuilt Linux binaries

Download the executable for your architecture (`botui-x86_64` or `botui-aarch64`). Each release includes `SHA256SUMS`. [Linux releases](https://github.com/gAlleb/botui/releases)

Branch builds contain the same files in `botui-linux.tar.gz`. [GitHub Actions](https://github.com/gAlleb/botui/actions/workflows/build.yml)

To install the downloaded binary for your user (substitute `botui-aarch64` on ARM64):

```sh
install -Dm755 ./botui-x86_64 "$HOME/.local/bin/botui"
```

Make sure `~/.local/bin` is on your `PATH`. You can also run the executable by its full path.

### With mise

To build from source, run the commands from the repository root. Install a C compiler, a linker and Python 3. The EPUB layout engine uses `python3` during compilation. mise supplies Rust and CMake.

```sh
mise trust
mise install rust cmake
mise run install
```

The install task puts the native build in `./dist/botui`. To build static binaries for **both x86_64 and aarch64**, run `mise install` and `mise run dist`. The two binaries and `SHA256SUMS` appear in `./dist/`.

Cargo caches the native build and each static target separately under `target/` by default. The first build of each target compiles its dependencies. Repeating the same task reuses that work.

### With Cargo

Install rustup, a C compiler, a linker, CMake and Python 3. From the repository root, Cargo uses the Rust version in `rust-toolchain.toml`.

```sh
cargo build --release --locked -p botui --target-dir target &&
  install -Dm755 target/release/botui ./dist/botui
```

## First run

```sh
botui                          # Choose a library
botui --library "$HOME/Books"  # Remember a local folder
botui ./examples/first-light.fb2
botui --resume                 # Continue your last unfinished local book or download
```

Press **Enter/L** for the local library, **B** for BookOrbit or **P** for OPDS. Use arrows and **Enter**, or click an item. **F1** shows controls for the current screen; **F2** opens settings. The interface labels are in English.

## Libraries

All supported formats use the same readers regardless of their source. BookOrbit and OPDS must provide a downloadable edition in one of these formats.

| Source | How to use it |
| --- | --- |
| Local folders | Browse covers or filenames, search with **/** and manage collections. **P** chooses a folder; **R** makes the current folder your library root. |
| BookOrbit | Sign in, browse books, authors, series and collections, then open an edition from its card. **Read** keeps a downloaded copy; **D Save offline** prepares it for later. Reading position syncs with your account. |
| OPDS | Open an OPDS 1 or 2 catalog, browse or search, and choose a downloadable edition. Saved catalogs are managed in **F2 → Library → OPDS catalogs**. Reading positions stay on this device. |

On a shelf, **V** switches cover grid/list and **/** searches. **Q** goes back one step: from a BookOrbit reader to its card and original shelf, or from a local book to its folder and parent folders. BOTUI keeps the selection and scroll position. **Q** on the source chooser exits; closing a file opened directly from the command line ends that command.

Offline copies are available through **D BookOrbit offline** and **O OPDS downloads** on the start screen. [Offline sync](docs/guide.md#offline-reading-and-sync) · [Managing collections](docs/guide.md#managing-collections)

## Formats

| Format | Reading features | Runtime requirements |
| --- | --- | --- |
| EPUB / FB2 | Reflowing text, contents, illustrations, search and passage annotations. EPUB also renders styled tables, MathML/marked TeX and static SVG. | Built in |
| Markdown (`.md`, `.markdown`) | Headings, lists, quotes, task lists, tables, code highlighting and local images. **F9** switches formatted/source views; selection and notes work in both. | Built in |
| PDF | Fixed pages, contents, zoom, rotation and page notes. A text layer enables search; `pdftotext` also enables word selection and highlights. | Poppler utilities (`pdfinfo`, `pdftoppm`, `pdftotext`), or Python 3 with Poppler GI and Cairo for rendering |
| DjVu | Fixed pages, contents, zoom, rotation and page notes; search when a text layer is available. | DjVuLibre (`djvused`, `ddjvu`, `djvutxt`) |
| CBZ / CBR | Image pages in natural filename order, contents, zoom, rotation, bookmarks and page notes. | Built-in ZIP/RAR decoders; no external unpacker |

PDF contents require Python 3, PyGObject and the Poppler GI bindings. Image-only EPUBs use the fixed-page reader. There is no OCR: scans and comic images do not provide word selection, definitions or translation.

Formatted Markdown distinguishes heading levels with numbers and colors, preserves list numbering and nesting, and uses reading-theme panels for code, quotes and table headers. NOTE/TIP/IMPORTANT/WARNING/CAUTION blocks have their own labels and colors. **F9** opens the source with Markdown highlighting.

Markdown reads local images within the document’s directory or its subdirectories; remote images are not downloaded. Raw HTML is shown as text, and MDX/Obsidian plugins are not executed. Notes keep their positions when switching views or resizing; external edits to the source file can shift them. Limits: 4 MiB of Markdown text and 16 MiB per image.

Comic pages may be JPEG, PNG or WebP, up to 16 MiB each. Password-protected and multi-volume RAR archives are not supported. Files with a `.cbz` or `.cbr` extension are recognized as ZIP or RAR by their contents, so mislabeled archives also open without renaming.

Kitty displays covers and pages as images, including inside tmux with graphics passthrough. Terminals without an image protocol can use colored half-blocks. [Detailed format limits](docs/checklist.md#coverage-limits)

## Reading

### Layout and text size

Use **1 / 2 / 3** for a single page, a two-page spread or continuous scroll. In Vim these digits are counts; use **Space l 1 / 2 / 3** to change layout. **Space o** opens contents, **Space z** toggles zen mode, and **Space p** toggles its progress footer.

**Space l w** adjusts text page width and the gap between columns. For PDF, DjVu, comics and image-only EPUBs, the same panel controls zoom and spread spacing. Its gap buttons or **[ / ]** set **0–32 terminal cells**; zero joins facing pages. `scan_gap` saves this independently of text spacing. **+ / −** zoom fixed pages, arrows pan, and **Space r 0** restores Fit page (Fit width in scroll mode). Enlarged pages fill the available margins and are clipped only at the reading pane edge.

Text size and typeface come from your terminal: use its font-size controls to enlarge or reduce text. BOTUI reflows the page. **F2 → Reader** controls first-line indentation, paragraph spacing and mouse-wheel step. Drag the contents divider to resize the sidebar.

### Themes

**F2 → Appearance → Interface theme** controls shelves, settings and other interface screens. **Space t** changes the reading palette over the open page: ↑/↓ preview a palette, ←/→ choose light/dark, Enter or a click applies, and Escape cancels. The interface starts in Catppuccin Mocha; **Terminal** follows your terminal’s colors.

For PDF, DjVu and image-only EPUB, the picker also offers **Original** and **Inverted**. Reading palettes recolor neutral paper and ink while retaining saturated illustration colors. The choice is saved in `scan_theme`, also editable through **F2 → Reader → PDF / image theme**. Text reading uses its own `theme` setting.

**CBZ/CBR artwork always keeps its original colors.** In comics, **Space t** previews only the surrounding background; **F2 → Reader → Comic background** edits the same `comic_background` setting. PDF inversion and recoloring do not affect comics.

**Space l f** switches opaque/transparent page backgrounds; window opacity belongs to the terminal.

### Text, highlights and notes

Drag to select text, double-click a word or triple-click a paragraph (a text block in PDF). Right-click a word or selection for **Copy, Highlight, Search, Translate, Define, Add note** in a small menu beside the text. Right-clicking inside a selection keeps the whole passage. Escape or a click outside closes the menu.

Click a saved highlight to open its comment, or choose **Add note** from its context menu. Choose a color with the mouse, enter a comment and click **Save** or press **Ctrl+S**. Enter inserts a newline. Clearing a comment keeps the highlight; deleting both text and color removes the annotation.

**Space a a** opens the searchable list of highlights and notes. Select an entry to see its text and position in the book/section or its page number; use **Go**, **Edit**, **Color**, **Delete** or **Export**. **Space a p** adds a page note to a fixed-page document. Notes and highlights are stored on this device; BookOrbit range-annotation sync is not implemented.

**Define** and **Translate** use the connections selected in [Configuration](#dictionaries-and-translation). They send the chosen text only when requested. Keyboard equivalents and Vim motions are listed below.

## Controls

Press **Space**, then the command keys **in sequence**. Menu hints appear after 150 ms by default; commands work immediately. In a submenu, **q / Esc / Backspace** go back; Escape closes the root menu.

Books open without a cursor. **Space c** enables Vim navigation and selection; **v / V** select only in this mode. Escape cancels a pending command or selection, then returns from idle Normal mode to ordinary reading. **Space c** also returns to reading directly. Explicit `[keys]` overrides apply to ordinary EPUB/FB2 reading and do not replace Space commands or Vim motions.

<details>
<summary>All reading, Vim and panel commands</summary>

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

</details>

<details>
<summary>Libraries, book cards and command-line options</summary>

| Screen | Keys |
| --- | --- |
| Home | Enter/L: local library; B: BookOrbit; P: OPDS; D: BookOrbit offline; O: OPDS downloads; Q: quit |
| Shelves | /: search; V: grid/list; U: borders; F: background; Enter/click: open; Q/Esc: back; F6: focus navigation |
| BookOrbit | 0: dashboard; 1: books; 2: reading; 3: recent; 4: libraries; 5: collections; 6: authors; 7: series |
| Local library | 1: folders; 2: reading; 3: recent; 4: collections; P: choose folder; R: use as root; A: collection membership; I: file details |
| Collections | BookOrbit: N creates; local: New collection in section 4; Del deletes the selected collection after confirmation |
| BookOrbit card | R: read; D: save; C/cover: external cover viewer; I: synopsis; H: hold/resume; X: reset reading; A: collections; L: reading log; B: related books |
| Offline downloads | Del: delete downloaded copy after confirmation |

```sh
botui [FILE | DIRECTORY]
botui --local [DIRECTORY]
botui --library DIRECTORY
botui --resume
botui --server URL
botui --opds URL
botui --config-path
botui --check-bookmarks TITLE
botui --help
botui --license
```

</details>

## Configuration

**F2** edits settings in the same TOML file used for manual configuration. Run `botui --config-path` to locate it. Reader, appearance, library, BookOrbit and lookup settings are kept together; saving preserves unrelated values and comments.

### Dictionaries and translation

**F2 → Dictionaries** has separate **Word definitions** and **Translation** lists. Add, edit, test, delete or select a default connection, then choose **Save changes & back**. Free Dictionary and Google Translate are enabled as editable HTTP profiles in the default config. They are not hardcoded providers; you can replace or remove them. An explicitly empty list stays empty.

Choose the connection type that fits your service:

| Type | Configuration |
| --- | --- |
| HTTP API | `provider = "http"`, a URL, the request parameters and the path to text in the response |
| OpenAI-compatible API | `provider = "openai"`, the chat-completions URL and a model; optional key and explanation language |
| Local command | `provider = "command"` and a program/argument array; install the program and its dictionaries separately |

`[lookup]` holds the default `dictionary` and `translation` names and shared languages. Each `[[lookup.profiles]]` block defines one connection; its unique `name` is what the defaults refer to. An empty default name selects the first profile of that purpose. Remove `profiles = []` before adding profile blocks.

A short profile and a longer one use **the same schema**: the longer one supplies optional values or parameters required by that service. For HTTP:

| Field | Meaning |
| --- | --- |
| `name`, `provider`, `endpoint` | Connection name, `"http"`, full service URL |
| `purpose` | `"dictionary"` by default, or `"translation"` |
| `method`, `response_format` | `"GET"` and `"json"` by default; omit unless different |
| `response_path` | Path to JSON strings, such as `/translatedText` or `/*/shortdef/*`; `*` iterates arrays. Empty selects the root. |
| `response_separator` | Defaults to no separator; use `"\n"` for definitions on separate lines |
| `query`, `headers`, `body` | Only the request fields required by the service |
| `api_key` / `key_command` | A key value or a command returning it; the command takes precedence. HTTP sends it only where `{key}` appears. |
| `description` | Optional label for your reference |

Templates use `{text}`, `{source}`, `{target}` and `{key}`. URL/query values are encoded automatically; JSON body placeholders go inside strings. A language choice affects only templates that use it: selecting Russian does not turn an English-only dictionary into a translator. Plain-text responses use `response_format = "text"`.

The complete example below is identical to [config.example.toml](config.example.toml). Free Dictionary and Google Translate are active; Merriam-Webster Collegiate, a custom translator, an OpenAI-compatible server and `sdcv` are commented templates. Merriam-Webster needs a **Collegiate** API key; the template extracts short definitions (`shortdef`). Uncomment its block, supply your key and select its name in `lookup.dictionary`, or enter the same fields in F2. **Test connection with ‘hello’** sends a real request.

Older service-specific profiles are converted once to explicit HTTP fields. New settings are added to older configs without overwriting existing choices. Additional connection fields, language substitutions and credential examples are documented in the config itself.

<details>
<summary>Complete config.toml example</summary>

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

</details>

## Development

| Command | What it checks |
| --- | --- |
| `mise run check` | Formatting, Clippy and Rust tests |
| `mise run smoke` | Library and reader workflows in isolated terminal sessions |

Smoke tests need tmux, Python 3.11+, the PDF and DjVu helpers above, and `c44` and `djvm` for test fixtures.

[Architecture](docs/architecture.md) · [Verification checklist](docs/checklist.md) · [Coverage limits](docs/checklist.md#coverage-limits)

[Release notes](CHANGELOG.md)

## License

BOTUI and its generated book jackets use [GPL-3.0-or-later](LICENSE). Covers supplied by books and catalogs retain their original rights.

The RAR decoder [rars](https://github.com/bitplane/rars) uses [Apache-2.0](docs/licenses/rars-Apache-2.0.txt). Component notices are available in **F2 → Legal**, through `botui --license` and in [docs/licenses](docs/licenses/).
