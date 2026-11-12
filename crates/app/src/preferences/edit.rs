use crate::{
    AppTerminal,
    appearance::BordersStyle,
    connection::PasswordCommand,
    prompt,
    settings::{self, Settings},
    shell_theme,
    theme::Theme,
};
use std::{io, path::PathBuf};
fn choose(
    term: &mut AppTerminal,
    name: &str,
    items: &[&str],
    i: usize,
) -> io::Result<Option<usize>> {
    prompt::choose(
        term,
        name,
        &items.iter().map(|s| format!(" {s}")).collect::<Vec<_>>(),
        i,
    )
}
fn edit(term: &mut AppTerminal, name: &str, value: &str) -> io::Result<Option<String>> {
    prompt::edit(term, name, "[ Save ↵ ]", value, |_| {})
}
pub(super) fn setting(
    term: &mut AppTerminal,
    tab: usize,
    i: usize,
    p: &Settings,
) -> io::Result<()> {
    match (tab, i) {
        (1, 12) => {
            if let Some(value) = edit(
                term,
                "Space menu delay · milliseconds (0–1000)",
                &p.leader_delay_ms.unwrap_or(150).to_string(),
            )? {
                let ms = value
                    .parse::<u16>()
                    .ok()
                    .filter(|n| *n <= 1000)
                    .ok_or_else(|| io::Error::other("Enter a delay from 0 to 1000 milliseconds"))?;
                Settings::update(|s| s.leader_delay_ms = Some(ms))?;
            }
        }

        (0, 0) => {
            if let Some(i) = interface(term, p)? {
                Settings::update(|s| s.interface_theme = Some(shell_theme::ALL[i].0.into()))?;
            }
        }
        (0, 1) => {
            Settings::update(|s| s.opaque_background = !s.opaque_background)?;
        }
        (0, 2) => {
            let styles = [
                BordersStyle::Rounded,
                BordersStyle::Square,
                BordersStyle::None,
            ];
            if let Some(n) = choose(
                term,
                "Borders & covers",
                &["Rounded", "Square", "None"],
                p.borders as usize,
            )? {
                Settings::update(|s| s.borders = styles[n])?;
            }
        }
        (0, 3) => {
            if let Some(v) = edit(
                term,
                "Solid terminal color · #RRGGBB or blank for Auto",
                p.opaque_color.as_deref().unwrap_or(""),
            )? {
                if !v.is_empty() && settings::rgb(&v).is_none() {
                    return Err(io::Error::other(
                        "Enter #RRGGBB, or clear the field for the terminal color.",
                    ));
                }
                Settings::update(|s| s.opaque_color = (!v.is_empty()).then_some(v))?;
            }
        }
        (1, 0) => {
            use crate::reader::theme_picker::{Edit, Picker};
            let mut picker = Picker::new(Theme::from_id(p.theme.as_deref().unwrap_or("terminal")));
            loop {
                term.draw(|f| picker.draw_named(f, " Reading theme "))?;
                if let Some(e) = picker.input(crate::settings::read_event()?) {
                    match e {
                        Edit::Cancel => break,
                        Edit::Accept(t) => {
                            Settings::update(|s| s.theme = Some(t.id()))?;
                            break;
                        }
                        _ => picker.edit(e),
                    }
                }
            }
        }
        (1, 10) => {
            Settings::update(|s| s.zen_progress = !s.zen_progress)?;
        }
        (1, 1) => {
            Settings::update(|s| s.zen = !s.zen)?;
        }
        (1, 2) | (1, 3) | (1, 7) | (1, 11) => {
            let (val, lo, hi, title) = if i == 11 {
                (
                    u16::from(p.scroll_lines.unwrap_or(1).clamp(1, 12)),
                    1,
                    12,
                    "Mouse wheel step · 1–12 lines",
                )
            } else if i == 2 {
                (
                    p.page_width.unwrap_or(78),
                    32,
                    240,
                    "Page width · 32–240 cells",
                )
            } else if i == 7 {
                (
                    u16::from(p.paragraph_indent.min(8)),
                    0,
                    8,
                    "First-line indent · 0–8 cells",
                )
            } else {
                (p.column_gap.unwrap_or(4), 0, 32, "Column gap · 0–32 cells")
            };
            if let Some(v) = edit(term, title, &val.to_string())? {
                let Some(n) = v.parse::<u16>().ok().filter(|v| (lo..=hi).contains(v)) else {
                    return Err(io::Error::other(format!(
                        "Enter a number from {lo} to {hi}."
                    )));
                };
                Settings::update(|s| {
                    if i == 11 {
                        s.scroll_lines = Some(n as u8)
                    } else if i == 2 {
                        s.page_width = Some(n)
                    } else if i == 7 {
                        s.paragraph_indent = n as u8;
                    } else {
                        s.column_gap = Some(n)
                    }
                })?;
            }
        }
        (1, 4) => {
            Settings::update(|s| s.preserve_printed_lines = !s.preserve_printed_lines)?;
        }
        (1, 5) => {
            Settings::update(|s| s.hide_inline_images = !s.hide_inline_images)?;
        }
        (1, 6) => {
            let modes = ["auto", "single", "spread", "scroll"];
            let selected = modes
                .iter()
                .position(|v| Some(*v) == p.reading_layout.as_deref())
                .unwrap_or(0);
            if let Some(i) = choose(term, "Default page layout", &modes, selected)? {
                Settings::update(|s| s.reading_layout = Some(modes[i].into()))?;
            }
        }
        (1, 8) => {
            if let Some(choice) = choose(
                term,
                "Paragraph spacing",
                &super::PARAGRAPH_SPACING,
                super::spacing_index(p),
            )? {
                Settings::update(|s| s.paragraph_spacing = choice.checked_sub(1).map(|n| n as u8))?;
            }
        }
        (1, 9) | (2, 4) => {
            let reader = tab == 1;
            let val = if reader {
                p.contents_width
            } else {
                p.sidebar_width
            };
            if let Some(v) = edit(
                term,
                if reader {
                    "Contents width · 12–100 cells or blank for Auto"
                } else {
                    "Sidebar width · 12–100 cells or blank for Auto"
                },
                &val.map_or(String::new(), |n| n.to_string()),
            )? {
                let v = v.trim();
                let n = if v.is_empty() {
                    None
                } else {
                    Some(
                        v.parse::<u16>()
                            .ok()
                            .filter(|n| (12..=100).contains(n))
                            .ok_or_else(|| {
                                io::Error::other("Enter 12–100 cells or clear the field.")
                            })?,
                    )
                };
                Settings::update(|s| {
                    if reader {
                        s.contents_width = n
                    } else {
                        s.sidebar_width = n
                    }
                })?;
            }
        }
        (2, 3) => {
            if let Some(s) = edit(
                term,
                "Download limit · MiB (0 = unlimited)",
                &p.max_download_mib.to_string(),
            )? {
                let n = s
                    .trim()
                    .parse::<u32>()
                    .map_err(|_| io::Error::other("Enter a non-negative number of MiB."))?;
                Settings::update(|s| s.max_download_mib = n)?;
            }
        }
        (2, 0) => {
            if let Some(v) = edit(
                term,
                "Library folder · blank for default",
                &p.library_root
                    .as_deref()
                    .map_or(String::new(), |p| p.display().to_string()),
            )? {
                let mut v = (!v.is_empty()).then(|| PathBuf::from(v));
                if let Some(path) = &mut v {
                    if let Ok(tail) = path.strip_prefix("~")
                        && let Some(h) = std::env::var_os("HOME")
                    {
                        *path = PathBuf::from(h).join(tail);
                    }
                    *path = path.canonicalize()?;
                    if !path.is_dir() {
                        return Err(io::Error::other("Choose a folder."));
                    }
                }
                Settings::update(|s| s.library_root = v)?;
            }
        }
        (2, 1) => {
            Settings::update(|s| s.local_grid = !s.local_grid)?;
        }
        (2, 2) => {
            crate::stats::library(term)?;
        }
        (3, 0) | (3, 1) => {
            let old = &p.bookorbit;
            let value = if i == 0 { &old.server } else { &old.username };
            if let Some(v) = edit(
                term,
                if i == 0 {
                    "BookOrbit server URL"
                } else {
                    "BookOrbit username"
                },
                value,
            )? && v != *value
            {
                if i == 0 && !v.is_empty() {
                    botui_bookorbit::BookOrbit::validate_address(&v)
                        .map_err(|e| io::Error::other(e.to_string()))?;
                }
                if old.has_credentials()
                    && choose(
                        term,
                        "Changing account clears its saved credentials",
                        &[
                            "Keep current account",
                            "Change account and clear credentials",
                        ],
                        0,
                    )? != Some(1)
                {
                    return Ok(());
                }
                Settings::update(|s| {
                    s.bookorbit.password = None;
                    s.bookorbit.password_command = None;
                    if i == 0 {
                        s.bookorbit.server = v
                    } else {
                        s.bookorbit.username = v
                    }
                })?;
            }
        }
        (3, 2) | (3, 3) => {
            if p.bookorbit.server.is_empty() || p.bookorbit.username.is_empty() {
                return Err(io::Error::other("Set the server and username first."));
            }
            if i == 2 {
                if let Some(v) = prompt::field(
                    term,
                    "Replace saved password · blank to clear",
                    "[ Save ↵ ]",
                    "",
                    true,
                    |_| {},
                )? {
                    Settings::update(|s| {
                        if !v.is_empty() {
                            s.bookorbit.password_command = None;
                        }
                        s.bookorbit.password = (!v.is_empty()).then_some(v)
                    })?;
                }
            } else if let Some(v) = edit(
                term,
                "Replace password command · shell or JSON argv · blank clears",
                "",
            )? {
                let c = if v.is_empty() {
                    None
                } else if v.starts_with('[') {
                    let args: Vec<String> = serde_json::from_str(&v).map_err(|_| {
                        io::Error::other("Enter a JSON array of command arguments.")
                    })?;
                    if args.first().is_none_or(|a| a.is_empty()) {
                        return Err(io::Error::other("Missing command."));
                    }
                    Some(PasswordCommand::Argv(args))
                } else {
                    Some(PasswordCommand::Shell(v))
                };
                Settings::update(|s| {
                    if c.is_some() {
                        s.bookorbit.password = None;
                    }
                    s.bookorbit.password_command = c
                })?;
            }
        }
        (3, 4) => {
            Settings::update(|s| s.bookorbit.list_view = !s.bookorbit.list_view)?;
        }
        _ => (),
    };
    Ok(())
}
// Draw the whole choice in its palette, including a sample selection, before committing it.
fn interface(term: &mut AppTerminal, p: &Settings) -> io::Result<Option<usize>> {
    use crossterm::event::{Event, KeyCode, KeyEventKind, MouseButton, MouseEventKind};
    use ratatui::{
        layout::{Position, Rect},
        style::Style,
        widgets::{Block, List, ListState, Paragraph},
    };
    let mut selected = shell_theme::index(p.interface_theme.as_deref());
    let mut list = ListState::default();
    loop {
        let mut rows = Rect::default();
        let mut cancel = Rect::default();
        let mut save = Rect::default();
        list.select(Some(selected));
        term.draw(|f| {
            let palette = shell_theme::ALL[selected];
            let (fg, bg, accent) = palette.colors();
            let style = if selected == 0 {
                Style::new()
            } else {
                Style::new().fg(fg).bg(if settings::opaque() {
                    bg
                } else {
                    ratatui::style::Color::Reset
                })
            };
            f.render_widget(Block::new().style(style), f.area());
            let area = crate::centered(f.area(), 54, shell_theme::ALL.len() as u16 + 4);
            let edge = crate::appearance::block()
                .title(" Interface theme · preview ")
                .style(style);
            let inner = edge.inner(area);
            f.render_widget(edge, area);
            rows = Rect {
                height: inner.height.saturating_sub(1),
                ..inner
            };
            let accent = if selected == 0 {
                ratatui::style::Color::Cyan
            } else {
                accent
            };
            f.render_stateful_widget(
                List::new(shell_theme::ALL.iter().map(|p| format!("  {}", p.1)))
                    .style(style)
                    .highlight_style(style.fg(accent).reversed()),
                rows,
                &mut list,
            );
            let y = inner.bottom().saturating_sub(1);
            save = Rect::new(inner.x, y, inner.width.min(14), 1);
            cancel = Rect::new(save.right(), y, inner.width.saturating_sub(save.width), 1);
            f.render_widget(
                Paragraph::new("[Enter Apply]").style(style.fg(accent)),
                save,
            );
            f.render_widget(
                Paragraph::new("[Esc Cancel]").style(style.fg(accent)),
                cancel,
            );
            if selected == 0 {
                crate::settings::paint(f);
            }
        })?;
        let delta = match crate::settings::read_event()? {
            Event::Key(k) if k.kind != KeyEventKind::Release => match k.code {
                KeyCode::Esc | KeyCode::Char('q') => return Ok(None),
                KeyCode::Enter => return Ok(Some(selected)),
                KeyCode::Up | KeyCode::Char('k') => -1,
                KeyCode::Down | KeyCode::Char('j') => 1,
                KeyCode::Home => isize::MIN,
                KeyCode::End => isize::MAX,
                KeyCode::PageUp => -(rows.height as isize),
                KeyCode::PageDown => rows.height as isize,
                _ => continue,
            },
            Event::Mouse(m) => {
                let pos = Position::new(m.column, m.row);
                match m.kind {
                    MouseEventKind::ScrollUp => -1,
                    MouseEventKind::ScrollDown => 1,
                    MouseEventKind::Down(MouseButton::Left) if cancel.contains(pos) => {
                        return Ok(None);
                    }
                    MouseEventKind::Down(MouseButton::Left) if save.contains(pos) => {
                        return Ok(Some(selected));
                    }
                    MouseEventKind::Down(MouseButton::Left) if rows.contains(pos) => {
                        selected = (list.offset() + usize::from(pos.y - rows.y))
                            .min(shell_theme::ALL.len() - 1);
                        continue;
                    }
                    _ => continue,
                }
            }
            _ => continue,
        };
        selected = selected
            .saturating_add_signed(delta)
            .min(shell_theme::ALL.len() - 1);
    }
}
