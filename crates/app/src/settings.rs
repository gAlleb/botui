//! One editable TOML document. UI changes patch individual values to retain user comments.
mod legacy;
mod lookup_migration;
use crate::connection::Connection;
use ratatui::{Frame, style::Color};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    env, fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};
use toml_edit::{DocumentMut, TableLike};

const EXAMPLE: &str = include_str!("../../../config.example.toml");
fn default_download_limit() -> u32 {
    4096
}
#[derive(Default, Deserialize, Serialize)]
#[serde(default)]
pub struct Settings {
    pub library_root: Option<PathBuf>,
    #[serde(default = "default_download_limit")]
    pub max_download_mib: u32,
    pub zen: bool,
    pub zen_progress: bool,
    pub local_grid: bool,
    pub preserve_printed_lines: bool,
    pub hide_inline_images: bool,
    pub opaque_background: bool,
    pub theme: Option<String>,
    pub interface_theme: Option<String>,
    pub opaque_color: Option<String>,
    pub borders: crate::appearance::BordersStyle,
    pub page_width: Option<u16>,
    pub scan_zoom: Option<u16>,
    pub paragraph_indent: u8,
    pub paragraph_spacing: Option<u8>,
    pub sidebar_width: Option<u16>,
    pub contents_width: Option<u16>,
    pub reading_layout: Option<String>,
    pub column_gap: Option<u16>,
    pub scroll_lines: Option<u8>,
    pub bookorbit: Connection,
    pub opds: Vec<crate::opds_settings::Catalog>,
    pub keys: BTreeMap<char, String>,
    /// Executable and arguments; {text} is replaced without invoking a shell.
    pub lookup_command: Vec<String>,
    pub lookup: crate::lookup::Config,
    pub leader_delay_ms: Option<u16>,
}
thread_local! {
    static SHELL_THEME: std::cell::Cell<usize> = const {std::cell::Cell::new(0)};
    static SOLID_COLOR: std::cell::Cell<Option<Color>> = const {std::cell::Cell::new(None)};
    static TERMINAL: std::cell::Cell<crate::shell_theme::TerminalPaper> = Default::default();

}
pub(crate) fn rgb(s: &str) -> Option<Color> {
    let s = s.strip_prefix('#')?;
    if s.len() != 6 || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let n = u32::from_str_radix(s, 16).ok()?.to_be_bytes();
    Some(Color::Rgb(n[1], n[2], n[3]))
}
pub(crate) fn detect_terminal() {
    TERMINAL.set(crate::shell_theme::TerminalPaper::detect());
}
pub(crate) fn read_event() -> io::Result<crossterm::event::Event> {
    Ok(read_event_for(None)?.expect("a blocking read returns an event"))
}
pub(crate) fn read_event_for(
    timeout: Option<std::time::Duration>,
) -> io::Result<Option<crossterm::event::Event>> {
    read_scan_event(timeout, false)
}
pub(crate) fn read_scan_event(
    timeout: Option<std::time::Duration>,
    paper: bool,
) -> io::Result<Option<crossterm::event::Event>> {
    let event = crate::terminal_input::read(&TERMINAL, timeout, opaque() || paper)?;
    if matches!(event, Some(crossterm::event::Event::Resize(..))) {
        crate::cover::resized();
    }
    Ok(event)
}
pub(crate) fn scan_colors(theme: crate::theme::Theme) -> ([u8; 3], [u8; 3]) {
    let t = TERMINAL.get();
    let rgb = |c| match c {
        Some(Color::Rgb(r, g, b)) => Some([r, g, b]),
        _ => None,
    };
    let style = theme.style();
    let bg = rgb(if opaque() {
        if theme == crate::theme::Theme::TERMINAL {
            Some(solid_surface(0))
        } else {
            style.bg
        }
    } else {
        t.bg
    })
    .unwrap_or([30, 30, 46]);
    let fg = rgb(if theme == crate::theme::Theme::TERMINAL {
        t.fg
    } else {
        style.fg
    })
    .unwrap_or([222, 224, 230]);
    let lum = |v: [u8; 3]| {
        let [r, g, b] = v.map(|c| {
            let f = f64::from(c) / 255.;
            if f <= 0.04045 {
                f / 12.92
            } else {
                ((f + 0.055) / 1.055).powf(2.4)
            }
        });
        0.2126 * r + 0.7152 * g + 0.0722 * b
    };
    let (a, b) = (lum(bg) + 0.05, lum(fg) + 0.05);
    let ink = if a.max(b) / a.min(b) >= 4.5 {
        fg
    } else if a > 0.23 {
        [13, 14, 18]
    } else {
        [245, 245, 245]
    };
    (bg, ink)
}
pub(crate) fn opaque() -> bool {
    OPAQUE.load(Ordering::Relaxed)
}
static OPAQUE: AtomicBool = AtomicBool::new(false);
pub fn path() -> io::Result<PathBuf> {
    let base = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| {
            env::var_os("HOME")
                .map(PathBuf::from)
                .filter(|p| p.is_absolute())
                .map(|p| p.join(".config"))
        })
        .ok_or_else(|| io::Error::other("Cannot locate the botui config directory"))?;
    Ok(base.join("botui/config.toml"))
}
fn read(path: &Path) -> io::Result<String> {
    let mut s = String::new();
    fs::File::open(path)?
        .take(512 * 1024 + 1)
        .read_to_string(&mut s)?;
    if s.len() > 512 * 1024 {
        return Err(io::Error::other("botui config is too large"));
    }
    Ok(s)
}
fn decode(text: &str) -> io::Result<Settings> {
    toml_edit::de::from_str(text).map_err(|e| {
        // TOML errors normally include the entire source line, which can contain a password.
        let line = e.span().map(|r| {
            text.as_bytes()[..r.start.min(text.len())]
                .iter()
                .filter(|&&b| b == b'\n')
                .count()
                + 1
        });
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "Invalid config.toml{}; check TOML syntax and setting types",
                line.map_or(String::new(), |n| format!(" at line {n}"))
            ),
        )
    })
}
fn document(text: &str) -> io::Result<DocumentMut> {
    text.parse::<DocumentMut>().map_err(|e| {
        let line = e.span().map(|r| {
            text.as_bytes()[..r.start.min(text.len())]
                .iter()
                .filter(|&&b| b == b'\n')
                .count()
                + 1
        });
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "Invalid config.toml{}; check TOML syntax and setting types",
                line.map_or(String::new(), |n| format!(" at line {n}"))
            ),
        )
    })
}
fn save(path: &Path, doc: &DocumentMut) -> io::Result<()> {
    let mut tmp = tempfile::NamedTempFile::new_in(path.parent().unwrap())?;
    tmp.write_all(doc.to_string().as_bytes())?;
    tmp.as_file().sync_all()?;
    tmp.persist(path)?;
    Ok(())
}
fn lock_dir(path: &Path) -> io::Result<fs::File> {
    let dir = path.parent().unwrap();
    fs::create_dir_all(dir)?;
    let lock = fs::File::open(dir)?;
    lock.lock()?;
    Ok(lock)
}

// The before/after tables were both generated by serde, so formatting differences in the
// user's document cannot trigger an edit. Only keys changed by the caller are touched.
fn patch(doc: &mut dyn TableLike, before: &dyn TableLike, after: &dyn TableLike) {
    for (key, new) in after.iter() {
        let old = before.get(key);
        if old.is_some_and(|old| old.to_string() == new.to_string()) {
            continue;
        }
        if let (Some(target), Some(old), Some(new)) = (
            doc.get_mut(key).and_then(|v| v.as_table_like_mut()),
            old.and_then(|v| v.as_table_like()),
            new.as_table_like(),
        ) {
            patch(target, old, new);
        } else {
            let mut item = new.clone();
            if let (Some(old), Some(new)) =
                (doc.get(key).and_then(|v| v.as_value()), item.as_value_mut())
            {
                *new.decor_mut() = old.decor().clone();
            }
            doc.insert(key, item);
        }
    }
    for (key, _) in before.iter() {
        if !after.contains_key(key) {
            doc.remove(key);
        }
    }
}
fn apply(
    doc: &mut DocumentMut,
    settings: &mut Settings,
    change: impl FnOnce(&mut Settings),
) -> io::Result<()> {
    let serial = |s: &Settings| {
        toml_edit::ser::to_document(s)
            .map_err(|_| io::Error::other("Could not encode botui config"))
    };
    let before = serial(settings)?;
    change(settings);
    patch(
        doc.as_table_mut(),
        before.as_table(),
        serial(settings)?.as_table(),
    );
    Ok(())
}
// Materialize newly introduced defaults in older configs without replacing user tables.
// In particular an explicit empty provider list must stay empty.
fn extend_config(doc: &mut DocumentMut) -> io::Result<bool> {
    let example = document(EXAMPLE)?;
    let mut changed = false;
    for name in ["leader_delay_ms", "scroll_lines", "lookup"] {
        if !doc.contains_key(name) {
            doc.insert(name, example[name].clone());
            changed = true;
        }
    }
    if !doc.contains_key("opds") {
        let value = toml_edit::Value::Array(toml_edit::Array::new());
        doc.insert("opds", toml_edit::Item::Value(value));
        changed = true;
    }
    Ok(changed)
}

impl Settings {
    pub fn load() -> io::Result<Self> {
        let mut doc = document(&read(&path()?)?)?;
        lookup_migration::migrate(&mut doc)?;
        decode(&doc.to_string())
    }
    pub fn initialize() -> io::Result<Self> {
        let path = path()?;
        let _lock = lock_dir(&path)?;
        let settings = match read(&path) {
            Ok(text) => {
                // Validate before touching the user's document; do not print credentials on errors.
                let mut doc = document(&text)?;
                let migrated = lookup_migration::migrate(&mut doc)?;
                decode(&doc.to_string())?;
                let changed = extend_config(&mut doc)? | migrated;
                let settings = decode(&doc.to_string())?;
                if changed {
                    save(&path, &doc)?;
                }
                settings
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                let mut doc = document(EXAMPLE)?;
                let mut settings = decode(EXAMPLE)?;
                legacy::import(path.parent().unwrap(), &mut doc, &mut settings)?;
                save(&path, &doc)?;
                settings
            }
            Err(e) => return Err(e),
        };
        legacy::retire_backed_up_files(path.parent().unwrap())?;
        settings.activate();
        Ok(settings)
    }
    pub fn update(change: impl FnOnce(&mut Self)) -> io::Result<Self> {
        let path = path()?;
        let _lock = lock_dir(&path)?;
        let text = read(&path)?;
        let mut doc = document(&text)?;
        lookup_migration::migrate(&mut doc)?;
        let mut settings = decode(&doc.to_string())?;
        apply(&mut doc, &mut settings, change)?;
        if doc.to_string() != text {
            save(&path, &doc)?;
        }
        settings.activate();
        Ok(settings)
    }
    pub fn activate(&self) {
        SHELL_THEME.set(crate::shell_theme::index(self.interface_theme.as_deref()));
        SOLID_COLOR.set(self.opaque_color.as_deref().and_then(rgb));
        self.borders.activate();
        OPAQUE.store(self.opaque_background, Ordering::Relaxed);
    }
}
pub fn paint(frame: &mut Frame) {
    paint_with(frame, 0);
}
pub fn paint_book(frame: &mut Frame, theme: crate::theme::Theme) {
    if theme == crate::theme::Theme::TERMINAL {
        paint(frame);
        return;
    }
    let style = theme.style();
    let ink = style.fg.unwrap_or(Color::Reset);
    let paper = TERMINAL.get().solid(style.bg.unwrap_or(Color::Reset));
    let accent = theme.link();
    let solid = opaque();
    for cell in &mut frame.buffer_mut().content {
        cell.fg = match cell.fg {
            Color::Reset => ink,
            Color::Cyan | Color::Blue | Color::Yellow => accent,
            c => c,
        };
        if cell.bg == Color::Reset && solid {
            cell.bg = paper;
        } else if matches!(cell.bg, Color::Blue | Color::Cyan | Color::Yellow) {
            cell.bg = accent;
        } else if solid {
            cell.bg = TERMINAL.get().solid(cell.bg);
        }
    }
}
pub(crate) fn interface_palette() -> crate::shell_theme::Palette {
    crate::shell_theme::ALL[SHELL_THEME.get()]
}
// Reader dialogs inherit book paper; settings and library screens keep their own palette.
thread_local! {
    static DIALOG_THEME: std::cell::Cell<Option<crate::theme::Theme>> = const { std::cell::Cell::new(None) };
}
pub struct DialogTheme(Option<crate::theme::Theme>);
impl Drop for DialogTheme {
    fn drop(&mut self) {
        DIALOG_THEME.set(self.0);
    }
}
pub fn reader_dialogs(theme: crate::theme::Theme) -> DialogTheme {
    DialogTheme(DIALOG_THEME.replace(Some(theme)))
}
pub fn interface_dialogs() -> DialogTheme {
    DialogTheme(DIALOG_THEME.replace(None))
}
pub fn paint_dialog(frame: &mut Frame) {
    if let Some(theme) = DIALOG_THEME.get() {
        paint_book(frame, theme);
    } else {
        paint_interface(frame);
    }
}
pub fn paint_interface(frame: &mut Frame) {
    paint_with(frame, SHELL_THEME.get());
}
fn solid_surface(index: usize) -> Color {
    let term = TERMINAL.get();
    let color = if index == 0 {
        SOLID_COLOR.get().or(term.bg).unwrap_or(Color::Rgb(0, 0, 0))
    } else {
        crate::shell_theme::ALL[index].colors().1
    };
    term.solid(color)
}
fn paint_with(frame: &mut Frame, index: usize) {
    let solid = opaque();
    if !solid && index == 0 {
        return;
    }
    let term = TERMINAL.get();
    let (fg, _, accent) = crate::shell_theme::ALL[index].colors();
    let paper = solid_surface(index);
    for cell in &mut frame.buffer_mut().content {
        if index != 0 {
            let named = |c| match c {
                Color::Cyan | Color::Yellow | Color::Blue => accent,
                _ => c,
            };
            cell.fg = if cell.fg == Color::Reset {
                fg
            } else {
                named(cell.fg)
            };
            cell.bg = named(cell.bg);
        }
        if cell.bg == Color::Reset {
            if solid {
                cell.bg = paper;
            }
        } else if solid {
            cell.bg = term.solid(cell.bg);
        }
    }
}

#[cfg(test)]
mod dialog_tests {
    use super::*;
    use ratatui::{
        Terminal,
        backend::TestBackend,
        widgets::{Clear, Paragraph},
    };
    #[test]
    fn scan_paper_uses_reading_theme_independently_of_interface() {
        let old_shell = SHELL_THEME.get();
        let old_opaque = OPAQUE.swap(true, Ordering::Relaxed);
        let theme = crate::theme::Theme::from_id("sepia-dark");
        for shell in ["rose-pine", "catppuccin-mocha", "nord"] {
            SHELL_THEME.set(crate::shell_theme::index(Some(shell)));
            assert_eq!(scan_colors(theme), ([52, 46, 37], [255, 213, 149]));
        }
        assert_eq!(
            scan_colors(crate::theme::Theme::from_id("sepia-light")),
            ([241, 232, 208], [91, 70, 54])
        );
        SHELL_THEME.set(old_shell);
        OPAQUE.store(old_opaque, Ordering::Relaxed);
    }
    #[test]
    fn config_upgrade_preserves_custom_values_and_is_idempotent() {
        let original = "# My paper\ninterface_theme = \"nord\"\nleader_delay_ms = 240\n[bookorbit]\npassword = \"test-only\" # keep comment\n[keys]\nx = \"next\"\n";
        let mut doc = document(original).unwrap();
        assert!(extend_config(&mut doc).unwrap());
        let saved = doc.to_string();
        assert!(saved.contains("# My paper"));
        assert!(saved.contains("# keep comment"));
        let settings = decode(&saved).unwrap();
        assert_eq!(settings.leader_delay_ms, Some(240));
        assert_eq!(settings.keys.get(&'x').unwrap(), "next");
        assert_eq!(settings.interface_theme.as_deref(), Some("nord"));
        assert_eq!(settings.lookup.profiles.len(), 2);
        assert!(settings.opds.is_empty());
        assert!(!extend_config(&mut doc).unwrap());
        assert_eq!(saved, doc.to_string());
        let mut disabled = document("[lookup]\nprofiles = []\n").unwrap();
        extend_config(&mut disabled).unwrap();
        assert!(
            decode(&disabled.to_string())
                .unwrap()
                .lookup
                .profiles
                .is_empty()
        );
    }

    #[test]
    fn documented_configuration_matches_example() {
        let settings = decode(EXAMPLE).unwrap();
        assert_eq!(settings.leader_delay_ms, Some(150));
        assert!(settings.keys.is_empty());
        for doc in [
            include_str!("../../../README.md"),
            include_str!("../../../README.ru.md"),
            include_str!("../../../docs/guide.md"),
            include_str!("../../../docs/guide.ru.md"),
        ] {
            assert!(
                doc.contains(EXAMPLE.trim()),
                "Configuration reference is outdated"
            );
        }
    }

    #[test]
    fn reader_popups_keep_paper_and_restore_shell_context() {
        let mut term = Terminal::new(TestBackend::new(12, 4)).unwrap();
        let theme = crate::theme::Theme::at(3, true);
        let old = SHELL_THEME.replace(crate::shell_theme::index(Some("catppuccin-mocha")));
        for solid in [false, true] {
            OPAQUE.store(solid, Ordering::Relaxed);
            {
                let _scope = reader_dialogs(theme);
                term.draw(|f| {
                    f.render_widget(Clear, f.area());
                    f.render_widget(Paragraph::new("Dictionary"), f.area());
                    paint_dialog(f);
                })
                .unwrap();
                let cell = &term.backend().buffer()[(0, 0)];
                assert_eq!(cell.fg, theme.style().fg.unwrap());
                assert_eq!(
                    cell.bg,
                    if solid {
                        theme.style().bg.unwrap()
                    } else {
                        Color::Reset
                    }
                );
                {
                    let _settings = interface_dialogs();
                    assert_eq!(DIALOG_THEME.get(), None);
                    let mut reference = Terminal::new(TestBackend::new(12, 4)).unwrap();
                    reference
                        .draw(|f| {
                            f.render_widget(Paragraph::new("Settings"), f.area());
                            paint_interface(f);
                        })
                        .unwrap();
                    term.draw(|f| {
                        f.render_widget(Paragraph::new("Settings"), f.area());
                        paint_dialog(f);
                    })
                    .unwrap();
                    assert_eq!(term.backend().buffer(), reference.backend().buffer());
                }
                assert_eq!(DIALOG_THEME.get(), Some(theme));
                {
                    let _nested = reader_dialogs(crate::theme::Theme::TERMINAL);
                }
                assert_eq!(DIALOG_THEME.get(), Some(theme));
            }
            assert_eq!(DIALOG_THEME.get(), None);
        }
        SHELL_THEME.set(old);
        OPAQUE.store(false, Ordering::Relaxed);
    }
}
