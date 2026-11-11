<p align="center"><img src="docs/images/mascot.png" width="200" height="200" alt="BOTUI’s owl perched on two books"></p>

# botui

**English** · [Русский](README.ru.md)

BOTUI is a terminal book reader for Linux. Open an EPUB, FB2, PDF or DjVu from disk, an OPDS catalog or your BookOrbit library. Read one page at a time, open a two-page spread or scroll continuously.

[Highlights](#highlights) · [Screenshots](#screenshots) · [Installation](#installation) · [First run](#first-run) · [User guide](docs/guide.md) · [Release notes](CHANGELOG.md)

## Highlights

- EPUB, FB2, PDF and DjVu from local folders, OPDS catalogs and BookOrbit
- Cover grid, collections and library search as you type
- Offline reading with BookOrbit progress sync across devices
- Single pages, two-page spreads and continuous scrolling, with an optional contents sidebar
- In-book search, bookmarks, jump history and reading stats
- EPUB/FB2 text selection, clipboard copy, colored highlights and comments with Markdown export
- EPUB tables with merged cells, MathML and TeX formulas, SVG diagrams and inline illustrations
- PDF and DjVu zoom, fit to width and page colors that match the reader theme
- Keyboard and mouse controls, themes, adjustable margins and zen mode

## Libraries

- **Local folders.** Browse by cover or title, find books as you type and organize them into collections. BOTUI remembers your place and caches the jackets it generates for books without covers.
- **BookOrbit.** Browse authors, series and collections. Opening an edition saves a copy on your device. Continue offline and sync your reading position when you reconnect. [BookOrbit project](https://github.com/bookorbit/bookorbit)
- **OPDS.** Browse OPDS 1 and 2 catalogs. Downloaded editions stay on the OPDS downloads shelf and keep their reading positions on this device.

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

### More themes and font sizes

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

### Format support

| Format | Tools needed for reading |
| --- | --- |
| EPUB, FB2 | Built in |
| PDF | Poppler utilities (`pdfinfo`, `pdftoppm`, `pdftotext`) or Python 3 with Poppler GI and Cairo |
| DjVu | DjVuLibre (`djvused`, `ddjvu`, `djvutxt`) |

The PDF contents tree uses Python 3 with PyGObject and the Poppler GI bindings. Searching a PDF or DjVu requires a text layer.

Kitty displays pages and covers as images, including inside tmux with graphics passthrough. Terminals without an image protocol can use colored half-blocks.

## First run

```sh
botui                          # Choose a library
botui --library "$HOME/Books"  # Remember a local folder
botui ./examples/first-light.fb2
botui --resume                 # Continue your last unfinished local book
```

On the start screen, press **Enter** for your local library, **B** for BookOrbit or **P** for OPDS. On a shelf, press **/** to search as you type. Use the arrows and **Enter**, or click a book.

| While reading | Control |
| --- | --- |
| Single page, two-page spread, continuous scroll | **1**, **2**, **3** |
| Toggle zen mode or show the contents sidebar | **Z** |
| Show or hide page and percentage in zen mode | **%** |
| Adjust text width and column gap, or zoom a scan | **W** |
| Search inside the book | **/** |
| Next match / previous match | **F3** / **F4** |
| Set or remove a bookmark / open bookmarks | **B** / **G** |
| Go to a PDF or DjVu page | **P** |
| Return to the shelf | **Q** |

## Settings and help

Press **F2** for settings or **?** for shortcuts on the current screen. The interface starts in **Catppuccin Mocha**. Choose **Terminal** to follow your terminal palette.

In **F2 → Reader → Paragraph spacing**, follow the book’s layout or choose 0, 1 or 2 blank lines between paragraphs in EPUB and FB2.

Run `botui --config-path` to locate the TOML file. The [configuration example](config.example.toml) covers library paths, appearance, key bindings and BookOrbit sign-in with a password or password command.

[Create and delete collections, add books and remove them](docs/guide.md#managing-collections)

See the [user guide](docs/guide.md) for offline sync, bookmarks, notes, footnotes and PDF controls.

## Development

| Command | What it checks |
| --- | --- |
| `mise run check` | Formatting, Clippy and Rust tests |
| `mise run smoke` | Library and reader workflows in isolated terminal sessions |

Smoke tests need tmux, Python 3.11+, the PDF and DjVu helpers above, and `c44` and `djvm` for test fixtures.

[Architecture](docs/architecture.md) · [Verification checklist](docs/checklist.md) · [Coverage limits](docs/checklist.md#coverage-limits)

## License

BOTUI and its generated book jackets use [GPL-3.0-or-later](LICENSE). Covers supplied by books and catalogs retain their original rights.
