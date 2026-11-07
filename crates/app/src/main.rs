mod appearance;
mod cache;
mod catalog;
mod connection;
mod cover;
mod cover_viewer;
mod diagnostic;
mod live_search;
mod offline;
mod picker;
mod preferences;
mod prompt;
mod reader;
mod scan;
mod settings;
mod shelf;
mod shell_theme;
mod splitter;
mod stats;
mod terminal_input;
mod theme;

use std::env;
use std::error::Error;
use std::io::{self, IsTerminal};
use std::path::PathBuf;
use std::process::ExitCode;

use crossterm::{
    cursor::{Hide, Show},
    event::{DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture},
    execute,
    style::ResetColor,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend, layout::Rect};

type AppTerminal = Terminal<CrosstermBackend<io::Stdout>>;
type Result<T> = std::result::Result<T, Box<dyn Error>>;

const HELP: &str = "botui [FILE | DIRECTORY | --local [DIRECTORY] | --resume | --library DIRECTORY | --server URL | --opds URL]\n\nOpen a book or remember a local library folder. Use --resume for the last unfinished local book or offline download. With no arguments, choose a source. F2 settings and About, ? shortcuts, --license for license texts. Use --config-path to locate config.toml or --check-bookmarks TITLE to inspect server bookmarks without editing them.\nBookOrbit: 0 dashboard, 1 all books, 2 reading, 3 recent, 4 libraries, 5 collections, 6 authors, 7 series. / search, V grid/list, U borders.\nIn EPUB or FB2: 1/2/3 single/spread/scroll, arrows or wheel to read, W page layout, [/] width, {/} column gap, Z zen, O contents, T theme, F background, B/G bookmarks, M menu, Q close.\nPDF and DjVu: 1/2/3 single/spread/scroll, P go to page (Ctrl+G also works), / find text, N next match, W width, +/- zoom, 0 fit, Z zen, G bookmarks.\n";

struct Screen;

impl Screen {
    fn enter() -> io::Result<(Self, AppTerminal)> {
        offline::reconnect::start();
        crate::settings::detect_terminal();
        enable_raw_mode()?;
        // Keep the guard alive through partial setup, input errors and unwinding.
        let guard = Self;
        execute!(
            io::stdout(),
            EnterAlternateScreen,
            Hide,
            EnableMouseCapture,
            EnableBracketedPaste
        )?;
        let terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
        Ok((guard, terminal))
    }
}

impl Drop for Screen {
    fn drop(&mut self) {
        let _ = execute!(
            io::stdout(),
            DisableBracketedPaste,
            DisableMouseCapture,
            ResetColor,
            Show,
            LeaveAlternateScreen
        );
        let _ = disable_raw_mode();
    }
}

fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    )
}

enum Start {
    Shelf(Option<String>),
    Opds(String),
    Local(Option<PathBuf>),
    File(PathBuf),
    Resume,
    Help,
    License,
    ConfigPath,
    CheckBookmarks(String),
}

fn arguments() -> Result<Start> {
    let mut args = env::args_os().skip(1);
    let start = match args.next() {
        None => Start::Shelf(None),
        Some(v) if v == "--resume" => Start::Resume,
        Some(v) if v == "--check-bookmarks" => Start::CheckBookmarks(
            args.next()
                .ok_or("Supply a book title after --check-bookmarks")?
                .into_string()
                .map_err(|_| "Invalid title")?,
        ),
        Some(v) if v == "--license" => Start::License,
        Some(v) if v == "--config-path" => Start::ConfigPath,
        Some(v) if v == "--help" || v == "-h" => Start::Help,
        Some(v) if v == "--opds" => Start::Opds(
            args.next()
                .ok_or("Supply the OPDS feed URL")?
                .into_string()
                .map_err(|_| "Invalid address")?,
        ),
        Some(v) if v == "--local" => Start::Local(args.next().map(PathBuf::from)),
        Some(v) if v == "--library" => Start::Local(Some(PathBuf::from(
            args.next().ok_or("Supply a library directory")?,
        ))),
        Some(v) if v == "--server" => {
            let server = args.next().ok_or("Supply the server URL after --server")?;
            Start::Shelf(Some(
                server.into_string().map_err(|_| "Invalid server address")?,
            ))
        }
        Some(v) if v == "--" => {
            let path = PathBuf::from(args.next().ok_or("Supply a file or directory after --")?);
            if path.is_dir() {
                Start::Local(Some(path))
            } else {
                Start::File(path)
            }
        }
        Some(v) if v.to_string_lossy().starts_with('-') => {
            return Err(format!("Unknown option {}. See botui --help", v.to_string_lossy()).into());
        }
        Some(v) if PathBuf::from(&v).is_dir() => Start::Local(Some(PathBuf::from(v))),
        Some(v) => Start::File(PathBuf::from(v)),
    };
    if args.next().is_some() {
        return Err("Unexpected extra argument. Quote paths containing spaces.".into());
    }
    Ok(start)
}

fn open_book(
    term: &mut AppTerminal,
    path: &std::path::Path,
    notices: &mut Vec<String>,
) -> Result<()> {
    open_local_doc(term, path, None, notices)
}

fn open_local_doc(
    term: &mut AppTerminal,
    path: &std::path::Path,
    document: Option<botui_core::Document>,
    notices: &mut Vec<String>,
) -> Result<()> {
    let open = || -> Result<botui_core::Document> {
        match document {
            Some(doc) => Ok(doc),
            None => botui_formats::open_document(path),
        }
    };
    let absolute = path.canonicalize()?;
    let path = absolute.as_path();
    if let Some(copy) = offline::Copy::open(path)? {
        let base = copy.starting_position();
        let (change, percent) =
            if let Some(k) = botui_formats::FileKind::from_path(path).filter(|k| !k.readable()) {
                scan::read_server(term, path, k, notices, Some(&base))?
            } else {
                let (change, pct) =
                    reader::read_server(term, path, open()?, notices, Some(&base), None)?;
                (change, Some(pct))
            };
        if let Some(p) = percent {
            botui_store::HistoryEntry::record(path, p)?;
        }
        if let Some(m) = change {
            copy.queue(path, m)?;
        }
        return Ok(());
    }
    if let Some(kind) = botui_formats::FileKind::from_path(path).filter(|k| !k.readable()) {
        scan::read(term, path, kind, notices)?;
    } else {
        reader::read(term, path, open()?, notices)?;
    }
    Ok(())
}

fn local_library(
    term: &mut AppTerminal,
    notices: &mut Vec<String>,
    root: Option<PathBuf>,
) -> Result<()> {
    let saved = settings::Settings::load()?.library_root;
    let dir = if let Some(path) = root {
        let path = path.canonicalize()?;
        if !path.is_dir() {
            return Err("Expected a library directory".into());
        }
        settings::Settings::update(|s| s.library_root = Some(path.clone()))?;
        path
    } else {
        saved
            .or_else(|| {
                env::var_os("HOME")
                    .map(|h| PathBuf::from(h).join("Books"))
                    .filter(|p| p.is_dir())
            })
            .unwrap_or(env::current_dir()?)
    };
    browse(term, notices, dir)
}

fn browse(term: &mut AppTerminal, notices: &mut Vec<String>, dir: PathBuf) -> Result<()> {
    let mut picker = match picker::Picker::new(dir.clone()) {
        Ok(p) => p,
        Err(e) => {
            let mut p = picker::Picker::new(env::current_dir()?)?;
            p.error = Some(format!("{} — {e}", dir.display()));
            p
        }
    };
    browse_picker(term, notices, &mut picker)
}

fn browse_downloads(
    term: &mut AppTerminal,
    notices: &mut Vec<String>,
    source: cache::Source,
) -> Result<()> {
    let dir = cache::downloads()?;
    std::fs::create_dir_all(&dir)?;
    let mut picker = picker::Picker::downloads(dir, source)?;
    browse_picker(term, notices, &mut picker)
}

fn browse_picker(
    term: &mut AppTerminal,
    notices: &mut Vec<String>,
    picker: &mut picker::Picker,
) -> Result<()> {
    while let Some(path) = picker.choose(term)? {
        if let Err(err) = open_book(term, &path, notices) {
            picker.error = Some(format!("{} — {err}", path.display()));
        }
    }
    Ok(())
}

fn opds_shelf(term: &mut AppTerminal, url: String, notices: &mut Vec<String>) -> Result<()> {
    let mut shelf = shelf::Shelf::opds(url);
    loop {
        match shelf.choose(term)? {
            shelf::Selection::Quit => return Ok(()),
            shelf::Selection::Book(f, _) => {
                let read = (|| -> Result<()> {
                    match botui_formats::FileKind::from_path(&f) {
                        Some(k) if !k.readable() => {
                            scan::read_server(term, &f, k, notices, None)?;
                        }
                        _ => {
                            reader::read_server(
                                term,
                                &f,
                                botui_formats::open_document(&f)?,
                                notices,
                                None,
                                None,
                            )?;
                        }
                    }
                    Ok(())
                })();
                if let Err(e) = read {
                    shelf.set_error(e.to_string());
                }
            }
            shelf::Selection::Downloads(source) => browse_downloads(term, notices, source)?,
            shelf::Selection::Local => local_library(term, notices, None)?,
            shelf::Selection::Opds => return Ok(()),
        }
    }
}
fn bookorbit_shelf(
    term: &mut AppTerminal,
    server: Option<String>,
    notices: &mut Vec<String>,
) -> Result<()> {
    let connection = settings::Settings::load()?.bookorbit;
    let explicit_server = server.is_some();
    let address = server
        .or_else(|| env::var("BOTUI_SERVER").ok())
        .unwrap_or_else(|| connection.server.clone());
    let user = if address.trim_end_matches('/') == connection.server.trim_end_matches('/') {
        connection.username.clone()
    } else {
        String::new()
    };
    let mut shelf = shelf::Shelf::new(address, user);
    if explicit_server {
        shelf.focus_bookorbit();
    }
    shelf.grid_view = !connection.list_view;
    shelf.credentials = Some(connection);
    loop {
        match shelf.choose(term)? {
            shelf::Selection::Quit => {
                if let Some(message) = shelf.take_exit_warning() {
                    notices.push(message);
                }
                return Ok(());
            }
            shelf::Selection::Opds => opds_shelf(term, String::new(), notices)?,
            shelf::Selection::Downloads(source) => browse_downloads(term, notices, source)?,
            shelf::Selection::Local => local_library(term, notices, None)?,
            shelf::Selection::Book(file, base) => {
                if base.is_none() && offline::Copy::open(&file)?.is_some() {
                    if let Err(e) = open_book(term, &file, notices) {
                        shelf.set_error(e.to_string());
                    }
                    continue;
                }
                let at = notices.len();
                let read = (|| -> Result<Option<botui_bookorbit::progress::Move>> {
                    if let Some(k) =
                        botui_formats::FileKind::from_path(&file).filter(|k| !k.readable())
                    {
                        Ok(scan::read_server(term, &file, k, notices, base.as_ref())?.0)
                    } else {
                        Ok(reader::read_server(
                            term,
                            &file,
                            botui_formats::open_document(&file)?,
                            notices,
                            base.as_ref(),
                            shelf.bookmarks.as_mut(),
                        )?
                        .0)
                    }
                })();
                match read {
                    Ok(Some(change)) => {
                        if let Some(p) = base {
                            shelf.save_move(file.to_path_buf(), p, change)
                        }
                    }
                    Ok(None) => {
                        if let Some(msg) = notices.get(at) {
                            shelf.set_error(msg.clone());
                        }
                    }
                    Err(e) => shelf.set_error(format!("Could not open book: {e}")),
                }
            }
        }
    }
}

fn run() -> Result<()> {
    let start = arguments()?;
    if matches!(start, Start::License) {
        print!(
            "BOTUI · GPL-3.0-or-later\n\n{}",
            include_str!("../../../LICENSE")
        );
        return Ok(());
    }
    if matches!(start, Start::Help) {
        print!("{HELP}");
        return Ok(());
    }
    settings::Settings::initialize()?;
    if let Start::CheckBookmarks(title) = &start {
        return diagnostic::bookmarks(title).map_err(|e| -> Box<dyn Error> { e });
    }
    if matches!(start, Start::ConfigPath) {
        println!("{}", settings::path()?.display());
        return Ok(());
    }
    let start = if matches!(start, Start::Resume) {
        let entry = botui_store::HistoryEntry::last_unfinished()?
            .ok_or("No unfinished local books. Open a file or choose a local library first.")?;
        Start::File(entry.path)
    } else {
        start
    };
    let path = match &start {
        Start::File(p) => Some(p.clone()),
        _ => None,
    };
    if let Some((file, kind)) = path.as_deref().and_then(|p| {
        botui_formats::FileKind::from_path(p)
            .filter(|k| !k.readable())
            .map(|k| (p, k))
    }) {
        let metadata = std::fs::metadata(file).map_err(|e| format!("{} — {e}", file.display()))?;
        if !metadata.is_file() {
            return Err("Expected a book file".into());
        }
        if io::stdin().is_terminal() && io::stdout().is_terminal() {
            let (_screen, mut terminal) = Screen::enter()?;
            let mut notices = vec![];
            open_book(&mut terminal, file, &mut notices)?;
            drop(_screen);
            for n in notices {
                eprintln!("botui: {n}");
            }
        } else {
            println!(
                "{} · {} · {} bytes. Open in a terminal to read.",
                file.display(),
                kind.label(),
                metadata.len()
            );
        }
        return Ok(());
    }
    // Report book errors even if the process was launched without a terminal.
    let opened = path
        .map(|p| {
            let doc =
                botui_formats::open_document(&p).map_err(|e| format!("{} — {e}", p.display()))?;
            Ok::<_, Box<dyn Error>>((p, doc))
        })
        .transpose()?;
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err("Open botui in an interactive terminal to read.".into());
    }

    let mut notices = Vec::new();
    {
        let (_screen, mut terminal) = Screen::enter()?;
        match opened {
            Some((path, doc)) => open_local_doc(&mut terminal, &path, Some(doc), &mut notices)?,
            None => match start {
                Start::Shelf(server) => bookorbit_shelf(&mut terminal, server, &mut notices)?,
                Start::Opds(url) => {
                    opds_shelf(&mut terminal, url, &mut notices)?;
                    bookorbit_shelf(&mut terminal, None, &mut notices)?;
                }
                Start::Local(root) => {
                    local_library(&mut terminal, &mut notices, root)?;
                    bookorbit_shelf(&mut terminal, None, &mut notices)?;
                }
                _ => unreachable!("file and help handled before entering terminal"),
            },
        }
    }
    for notice in notices {
        eprintln!("botui: {notice}");
    }
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("botui: {err}");
            ExitCode::FAILURE
        }
    }
}
