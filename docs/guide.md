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

## Reading

Books open without a cursor. **Space** opens commands; hints appear after **150 ms**, but the next key works immediately. Press keys in sequence, not together. `Space c` enables Vim. Only then do `v`/`V` select text. Legacy letters such as `t`, `b` and `v` have no action in ordinary reading. Explicit user `[keys]` overrides are an exception for EPUB/FB2; they do not override Vim or Space.

In a Space submenu, `q`, `Esc` and `Backspace` return to the root; `Esc` closes the root menu. Shelves and editors have their own contextual keys, listed separately.

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
| `Space a v` | Show/hide note body (EPUB/FB2) |
| `Space a p` | Page note (PDF/DjVu) |
| `Space d d` | Word definition |
| `Space d t` | Translate selected text |
| `Space l 1` | Single page |
| `Space l 2` | Two-page spread |
| `Space l 3` | Continuous scroll |
| `Space l w` | Page width / column gap; PDF zoom |
| `Space l f` | Opaque/transparent background |
| `Space l u` | Border style |
| `Space t` | Reading theme; PDF: original / inverted / match background |
| `Space s` | Settings |
| `Space ?` | Help |
| `q` / `Q` / `Space q` | Close book and return to its source |
| `Space r c` | Open cover in external viewer |
| `Space r s` | Reading statistics |
| `Space r [` | Jump back |
| `Space r ]` | Jump forward |
| `Space r i` | Links and illustrations (EPUB/FB2) |
| `Space r f` | Follow visible links (EPUB/FB2) |
| `Space r p` | Go to page (PDF/DjVu) |
| `Space r +` | Zoom in (PDF/DjVu) |
| `Space r -` | Zoom out (PDF/DjVu) |
| `Space r 0` | Fit page / fit width in scroll (PDF/DjVu) |
| `Space r r` | Rotate clockwise (PDF/DjVu) |
| `Space r R` | Rotate counterclockwise (PDF/DjVu) |

**Direct navigation in ordinary reading**

| Keys | Action |
| --- | --- |
| `← → / PgUp PgDn` | Navigate pages; in PDF arrows also pan |
| `↑ ↓ / j k` | Move through text; one line in continuous text scroll |
| `h l` | Previous/next page or horizontal movement |
| `1 / 2 / 3` | Single page / spread / scroll; in Vim: command count |
| `+ / = / -` | PDF/DjVu: zoom in / zoom in / zoom out |
| `Home End` | Start/end (PDF: document; Vim: line) |
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
| `gj gk / g0 g$` | Screen rows / screen row edges (EPUB/FB2) |
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
| `Ctrl+S / Enter` | Note editor: save / new line; empty saved comment deletes annotation |
| `Tab` | Note editor: change highlight color |
| `[ ] / { }` | Page layout panel: narrow/widen page / decrease/increase column gap |
| `/ / N / H / Y / Del / E` | Annotation list: filter / edit / color / copy / delete / export |
| `P / L / S / Y / N` | Lookup result: provider / target language / source language / copy / save as note |

### Tables, equations and illustrations

Open a technical EPUB as you would a novel. The reader applies styles packaged with the edition, including merged table cells, side-by-side columns, lists and preformatted code. Equations in presentation MathML and TeX explicitly marked as math appear in the passage. A fraction has its numerator above its denominator. Change the page width with **Space l w** to give a wide table more room.

The terminal supplies the typeface and cell size. Justified paragraphs keep natural spaces between words. Headings and right-aligned passages keep their alignment. The reader theme supplies the ink and paper colors.

Search, footnotes, bookmarks and BookOrbit resume use the source text even when a table wraps differently.

Embedded raster illustrations use the available page height. **Space r i** opens an illustration separately. **F2 → Reader → Illustrations** switches to the separate viewer only.

FB2 also preserves italic, bold and struck-through text in ordinary paragraphs. These faces do not change the text used for search or saved positions.

### Working with text

Drag to select a passage, double-click a word, or triple-click a paragraph. For keyboard selection, enter `Space c`, then `v`. `Space a y` copies, `Space a h` highlights, and `Space a n` opens a comment. Normal also supports copy operators such as `yiw` and `yip`.

`Space a a` opens annotations: Del deletes, N edits, H changes color, Y copies the quote, / filters, and E exports. In the editor, Enter inserts a line and Ctrl+S saves; saving an empty comment deletes the annotation. Esc cancels editing.

Annotations are stored on this device. BookOrbit range-annotation sync is not implemented. PDF requires a text layer; no OCR is performed.

### Dictionaries, translation and saved catalogs

**F2 → Dictionaries** separates **Word definitions** and **Translation**. Both lists start empty. Add a connection, give it your own name and optional description, then configure **HTTP API**, **OpenAI-compatible API** or **Local command**. Existing connections can be edited, tested, selected as default and deleted. Deleting the last connection leaves the list empty after restarting.

The HTTP editor exposes the address, method, query parameters, headers, JSON body and response text path. Use `{text}`, `{source}`, `{target}` and `{key}` in templates. A path such as `/items/*/text` collects strings from array items; the separator joins them. Plain-text responses are also supported. This covers JSON/text HTTP services; other protocols or authentication flows can be implemented through a local command. Service examples live in [config.example.toml](../config.example.toml), not in the Add menu. Older service-specific configurations remain readable.

Choose **Save changes & back** to keep edits. Leaving unsaved edits offers Save, Discard or Keep editing. Local programs and password-manager commands have separate program and argument fields. Descriptions belong to your configuration: clear one to hide it. API keys are masked.

Source and output languages are below the **Translation** connection list. **Explanation language** belongs to an OpenAI-compatible dictionary connection. The result window offers **P** to change connection, **L/S** to change target/source, **Y** to copy, **N** to save a passage note, and **Esc** to close. **Space menu delay** is under **F2 → Reader** and defaults to 150 ms; it delays hints, never commands.

Requests run only when invoked, with a 20-second limit and a bounded in-memory result cache. An external service receives the chosen word/passage; no automatic background translation occurs. Closing the window dismisses the request; an HTTP request already sent may finish in the worker. No provider is silently substituted. API keys can come from a plain value or an argument-array command; the command takes precedence. Keys are masked in settings.

Everything is in the same `config.toml`: `[lookup]`, `[[lookup.profiles]]`, `[[opds]]`, `[bookorbit]`, reader and appearance fields. [The example config](../config.example.toml) documents each provider and credential option. Changes made in settings preserve unrelated values and comments. Existing `lookup_command` remains compatible.

**F2 → Library → OPDS catalogs** manages named catalogs, addresses and optional credentials. They appear when choosing OPDS on the home screen; `botui --opds URL` also uses a saved connection with the same URL. Public catalogs need no credentials.

### PDF and DjVu

`Space l 3` enables continuous scrolling. `Space l w` opens width/zoom controls; `Space r +` and `Space r -` adjust zoom, including below 100%. At 100% in scroll mode, the sheet fits the reading pane width. `Space r 0` restores fit and `Space r p` opens a compact page-number dialog.

`Space t` cycles original colors, inversion and match background. `Space l f` changes opacity. `Space o` opens the contents tree; arrows fold/expand sections. Scroll mode separates sheets with a dim dotted line across the pane.

PDF text layers support selection, copying and highlights. Selections snap to words and retain coordinates through zoom and rotation. Extraction order may differ from visual order in complex multi-column PDFs. `Space a p` creates a PDF/DjVu page note. `Space f` searches the text layer.

BOTUI saves position within a sheet locally; BookOrbit receives the page number. Image-only EPUBs open in the same viewer.

## Configuration

Run `botui --config-path` to print the exact path. The default is `~/.config/botui/config.toml`. The same TOML file holds the library root, interface, reader bindings and BookOrbit account. The default UI language is English. See the [commented configuration](../config.example.toml).

On startup, older configs gain missing dictionary/translation defaults, the Space hint delay, mouse-wheel step and an empty OPDS list. Existing values, connections and comments are preserved.

On first launch the interface uses **Catppuccin Mocha**.

In **F2 → Reader**, **First-line indent** moves the first line of a paragraph to the right by 0–8 cells. **Paragraph spacing** sets the vertical gap between paragraphs in EPUB and FB2. **From book** follows the EPUB layout and uses one blank line for plain FB2 prose. Choose **0 blank lines**, **1 blank line** or **2 blank lines** to set your own gap. The choice also applies to the book you have open.

In an open book, drag the vertical border between the contents on the left and the page to change the width of the contents sidebar. You can also enter a width in **F2 → Reader → Contents width**. On the bookshelf, drag the border between navigation and the book list or use **F2 → Library → Sidebar width**.

**F2 → Library → Download limit** sets the maximum size per BookOrbit or OPDS edition. The default is **4096 MiB**. Set **0** for unlimited downloads. Downloads stream to disk.

[Keyboard help](images/help.png).

**F2 → Appearance** sets the interface palette independently of the reading paper (**Space t**). Terminal uses the default foreground, background and ANSI slots, so changes to the emulator’s palette appear without restarting.

Opaque paper follows the emulator’s OSC 11 response; if it cannot report RGB, botui uses ANSI black or your explicit `opaque_color`. Neither pywal nor matugen is required.

Built-in themes include Gruvbox, Catppuccin, Nord, Dracula, Tokyo Night, Solarized, One Dark, Kanagawa, Rosé Pine and Everforest. A named palette stays fixed; transparency lets the window background show through. Set borders and jacket corners to Rounded, Square or None here.

[Appearance settings in Catppuccin Mocha](images/settings.png)

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
interface_theme = "catppuccin-mocha"
# Optional override. By default, solid Terminal paper follows the active terminal background.
# opaque_color = "#1f2125"
zen = false
# Show a dim page count and percentage in zen mode. Toggle with Space p.
zen_progress = false
# PDF/DjVu zoom in percent. In scroll mode 100 fits the reading pane.
scan_zoom = 100
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
dictionary = ""
translation = ""
source_language = "auto"
target_language = "en"
profiles = []

# No built-in connections. Add your own in F2 > Dictionaries or below.
# Remove profiles = [] before enabling any [[lookup.profiles]] blocks.
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

The interface can use the terminal palette or BookOrbit reader themes. **Space l f** sets the background of terminal cells. Whole-window opacity is controlled by the terminal or compositor.

| Data | Default location |
| --- | --- |
| Config | `~/.config/botui/config.toml` |
| Positions, local bookmarks, notes and reading history | `~/.local/state/botui/` |
| Offline books and Markdown exports | `~/.local/share/botui/downloads/`, `~/.local/share/botui/exports/` |

The XDG config, state and data environment variables override those defaults. Local reading records use a content hash, so moving a file preserves its position.
