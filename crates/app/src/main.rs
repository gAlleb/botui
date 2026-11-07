mod cache;
mod connection;
mod cover;
mod picker;
mod reader;
mod scan;
mod shelf;
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

const HELP: &str = "botui [ФАЙЛ | --local | --server URL | --opds URL]\n\nБез аргументов открывает домашний экран BookOrbit. --server подставляет адрес, --local открывает список файлов.\nEPUB и FB2 открываются для чтения, PDF и DjVu — постранично (Poppler / DjVuLibre).\nВ читалке: ←/→ или колесо — листать, 1/2 — страница/разворот, T — цвета, O — главы, / — поиск, B/G — закладки, M или щелчок — меню, Q — закрыть книгу.\n";

struct Screen;

impl Screen {
    fn enter() -> io::Result<(Self, AppTerminal)> {
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
    Local,
    File(PathBuf),
    Help,
}

fn arguments() -> Result<Start> {
    let mut args = env::args_os().skip(1);
    let start = match args.next() {
        None => Start::Shelf(None),
        Some(v) if v == "--help" || v == "-h" => Start::Help,
        Some(v) if v == "--opds" => Start::Opds(
            args.next()
                .ok_or("Укажите адрес ленты OPDS")?
                .into_string()
                .map_err(|_| "Некорректный адрес")?,
        ),
        Some(v) if v == "--local" => Start::Local,
        Some(v) if v == "--server" => {
            let server = args.next().ok_or("Укажите адрес сервера после --server")?;
            Start::Shelf(Some(
                server
                    .into_string()
                    .map_err(|_| "Некорректный адрес сервера")?,
            ))
        }
        Some(v) if v == "--" => Start::File(PathBuf::from(
            args.next().ok_or("После -- укажите путь к книге")?,
        )),
        Some(v) if v.to_string_lossy().starts_with('-') => {
            return Err(format!(
                "Неизвестный параметр {}. См. botui --help",
                v.to_string_lossy()
            )
            .into());
        }
        Some(v) => Start::File(PathBuf::from(v)),
    };
    if args.next().is_some() {
        return Err("Лишний аргумент. Пути с пробелами заключайте в кавычки.".into());
    }
    Ok(start)
}

fn open_book(
    term: &mut AppTerminal,
    path: &std::path::Path,
    notices: &mut Vec<String>,
) -> Result<()> {
    if let Some(kind) = botui_formats::FileKind::from_path(path).filter(|k| !k.readable()) {
        scan::read(term, path, kind, notices)?;
    } else {
        reader::read(term, path, botui_formats::open_document(path)?, notices)?;
    }
    Ok(())
}

fn local_library(term: &mut AppTerminal, notices: &mut Vec<String>) -> Result<()> {
    let mut picker = picker::Picker::new(env::current_dir()?)?;
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
                if let Err(e) = open_book(term, &f, notices) {
                    shelf.set_error(e.to_string());
                }
            }
            shelf::Selection::Local => local_library(term, notices)?,
            shelf::Selection::Opds => return Ok(()),
        }
    }
}
fn bookorbit_shelf(
    term: &mut AppTerminal,
    server: Option<String>,
    notices: &mut Vec<String>,
) -> Result<()> {
    let settings = connection::Connection::load();
    let warning = settings
        .as_ref()
        .err()
        .map(|e| format!("Не удалось прочитать настройки подключения: {e}"));
    let settings = settings
        .unwrap_or_default()
        .with_server(server.or_else(|| env::var("BOTUI_SERVER").ok()));
    let mut shelf = shelf::Shelf::new(settings.server, settings.username);
    shelf.grid_view = !settings.list_view;
    if let Some(warning) = warning {
        shelf.set_error(warning);
    }
    loop {
        match shelf.choose(term)? {
            shelf::Selection::Quit => {
                if let Some(message) = shelf.take_exit_warning() {
                    notices.push(message);
                }
                return Ok(());
            }
            shelf::Selection::Opds => opds_shelf(term, String::new(), notices)?,
            shelf::Selection::Local => local_library(term, notices)?,
            shelf::Selection::Book(file, base) => {
                let at = notices.len();
                let read = (|| -> Result<Option<botui_bookorbit::progress::Move>> {
                    if let Some(k) =
                        botui_formats::FileKind::from_path(&file).filter(|k| !k.readable())
                    {
                        Ok(scan::read_server(term, &file, k, notices, base.as_ref())?)
                    } else {
                        Ok(reader::read_server(
                            term,
                            &file,
                            botui_formats::open_document(&file)?,
                            notices,
                            base.as_ref(),
                        )?)
                    }
                })();
                match read {
                    Ok(Some(change)) => {
                        if let Some(p) = base {
                            shelf.save_move(p, change)
                        }
                    }
                    Ok(None) => {
                        if let Some(msg) = notices.get(at) {
                            shelf.set_error(msg.clone());
                        }
                    }
                    Err(e) => shelf.set_error(format!("Не удалось открыть книгу: {e}")),
                }
            }
        }
    }
}

fn run() -> Result<()> {
    let start = arguments()?;
    if matches!(start, Start::Help) {
        print!("{HELP}");
        return Ok(());
    }
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
            return Err("Ожидался файл книги".into());
        }
        if io::stdin().is_terminal() && io::stdout().is_terminal() {
            let (_screen, mut terminal) = Screen::enter()?;
            let mut notices = vec![];
            if cover::graphics().protocol_type() != ratatui_image::picker::ProtocolType::Halfblocks
                || picker::show_info(&mut terminal, file, kind)?
            {
                scan::read(&mut terminal, file, kind, &mut notices)?;
            }
            drop(_screen);
            for n in notices {
                eprintln!("botui: {n}");
            }
        } else {
            println!(
                "{} · {} · {} байт. Для просмотра запустите в терминале.",
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
        return Err("Для чтения запустите botui в интерактивном терминале.".into());
    }

    let mut notices = Vec::new();
    {
        let (_screen, mut terminal) = Screen::enter()?;
        match opened {
            Some((path, doc)) => reader::read(&mut terminal, &path, doc, &mut notices)?,
            None => match start {
                Start::Shelf(server) => bookorbit_shelf(&mut terminal, server, &mut notices)?,
                Start::Opds(url) => opds_shelf(&mut terminal, url, &mut notices)?,
                Start::Local => local_library(&mut terminal, &mut notices)?,
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
