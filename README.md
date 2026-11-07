<p align="center"><img src="docs/images/mascot.png" width="200" height="200" alt="BOTUI’s owl perched on two books"></p>

# botui

**English** · [Русский](README.ru.md)

BOTUI is a terminal book reader for Linux. Open a book from disk, browse an OPDS catalog or connect a BookOrbit account.

[Screenshots](#screenshots) · [Installation](#installation) · [First run](#first-run) · [User guide](docs/guide.md)

## Libraries and reading

- **Local folders.** Browse covers or a list, find books as you type and return to your last reading position.
- **BookOrbit.** Browse authors, series and collections. Opening an edition saves it on your device. Read offline and sync your position when you reconnect.
- **OPDS.** Browse OPDS 1 and 2 catalogs, download an edition and find it later on the OPDS downloads shelf.
- **The reader.** Read EPUB and FB2 as text, or view the original pages of a PDF or DjVu. Choose a single page, a spread or continuous scrolling. Follow footnotes, bookmark a passage or export notes to Markdown.

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

Run these commands from the repository root. Each method below puts the Linux executable at `./dist/botui`.

### With mise

```sh
mise trust
mise install rust
mise run install
```

The install task builds the release for your machine. To build static binaries for **both x86_64 and aarch64**, run `mise install` and `mise run dist`. The two binaries and `SHA256SUMS` appear in `./dist/`.

### With Cargo

The repository pins Rust in `rust-toolchain.toml`. With rustup, a C compiler and a linker installed, run:

```sh
cargo build --release --locked -p botui --target-dir target &&
  install -Dm755 target/release/botui ./dist/botui
```

### Prebuilt Linux binaries

Download `botui-x86_64` or `botui-aarch64` for your machine, together with `SHA256SUMS`, from [Releases](https://github.com/gAlleb/botui/releases) or [branch builds](https://github.com/gAlleb/botui/actions/workflows/build.yml). With both files in the current directory, verify the download and install it:

```sh
sha256sum --ignore-missing -c SHA256SUMS &&
  install -Dm755 "botui-$(uname -m)" ./dist/botui
```

To run `botui` by name in this shell, add the build directory to your `PATH`:

```sh
export PATH="$PWD/dist:$PATH"
```

### Format support

| Format | Tools needed for reading |
| --- | --- |
| EPUB, FB2 | Built in |
| PDF | Poppler utilities (`pdfinfo`, `pdftoppm`, `pdftotext`) or Python 3 with Poppler GI and Cairo |
| DjVu | DjVuLibre (`djvused`, `ddjvu`, `djvutxt`) |

Kitty displays pages and covers as raster images. Other terminals can use colored half-blocks. Searching a PDF or DjVu requires a text layer.

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
| Adjust text width and column gap, or zoom a scan | **W** |
| Search inside the book | **/** |
| Go to a PDF or DjVu page | **P** |
| Return to the shelf | **Q** |

## Settings and help

Press **F2** for settings or **?** for shortcuts on the current screen. The interface starts in **Catppuccin Mocha**. Choose **Terminal** to follow your terminal palette.

Run `botui --config-path` to locate the TOML file. The [configuration example](config.example.toml) covers library paths, appearance, key bindings and BookOrbit sign-in with a password or password command.

See the [user guide](docs/guide.md) for offline sync, bookmarks, notes, footnotes and PDF controls.

## Development

| Command | What it checks |
| --- | --- |
| `mise run check` | Formatting, Clippy and Rust tests |
| `mise run smoke` | Library and reader workflows in isolated terminal sessions |

Smoke tests need tmux, Python 3.11+, the PDF and DjVu helpers above, and `c44` and `djvm` for test fixtures.

[Architecture](docs/architecture.md) · [Verification checklist](docs/checklist.md) · [Known gaps](docs/upstream-audit.md) · [Changelog](CHANGELOG.md)

## License

BOTUI and its generated book jackets use [GPL-3.0-or-later](LICENSE). Covers supplied by books and catalogs retain their original rights.
