mod annotations;
pub(crate) mod bookmarks;
pub(crate) mod clipboard;
pub(crate) mod controls;
mod layout;
mod links;
mod mark_list;
mod markdown_style;
mod navigation;
pub(crate) mod notes;
mod resources;
mod search;
mod study;
mod table_view;
pub(crate) mod theme_picker;
mod word_flash;
use navigation::{Edit, Kind};

use std::{
    io,
    path::Path,
    time::{Duration, Instant},
};

use botui_core::{
    Document, HistoryPosition, LayoutMode, Pager, PaginationMode, ReflowOptions, ViewPosition,
};
use botui_store::{LocalState, Saved};
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use ratatui::{
    Frame,
    layout::{Position, Rect, Size},
    style::Modifier,
    widgets::{Block, Clear, Paragraph, Row, Table, TableState},
};

use crate::{AppTerminal, centered, theme::Theme};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Action {
    AnnotationSync,
    Study(study::Command),
    Menu,
    Zen,
    ZenProgress,
    Background,
    Borders,
    Sidebar(Edit),
    Focus,
    JumpBack,
    JumpForward,
    FindAgain(bool),
    SearchResults,
    HideSearch,
    Note,
    Export,
    Statistics,
    Resources,
    Cover,
    VisibleLinks,
    OpenTable,
    Source,
    Follow(usize),
    ReturnLink,
    KeepLink,
    Settings,
    Help,
    Resume,
    Previous,
    Next,
    Single,
    Spread,
    Scroll,
    Lines(i8),
    Theme,
    Pick(theme_picker::Edit),
    Navigate(Kind),
    NavEdit(Edit),
    Mark,
    Bookmarks,
    FullWidth,
    Alignment,
    Hyphenation,
    Margin(i8),
    Gap(i8),
    Layout,
    Close,
    Up,
    Down,
    Resize,
}

const COMMANDS: [Action; 33] = [
    Action::Resume,
    Action::Previous,
    Action::Next,
    Action::Single,
    Action::Spread,
    Action::Scroll,
    Action::Theme,
    Action::Navigate(Kind::Contents),
    Action::Navigate(Kind::Search),
    Action::FindAgain(true),
    Action::FindAgain(false),
    Action::SearchResults,
    Action::Mark,
    Action::Bookmarks,
    Action::Layout,
    Action::Zen,
    Action::ZenProgress,
    Action::Background,
    Action::Borders,
    Action::Study(study::Command::Normal),
    Action::Study(study::Command::Highlight),
    Action::Note,
    Action::Navigate(Kind::Notes),
    Action::Export,
    Action::Statistics,
    Action::Resources,
    Action::OpenTable,
    Action::Cover,
    Action::JumpBack,
    Action::JumpForward,
    Action::Settings,
    Action::Help,
    Action::Close,
];

struct VisibleFigure {
    id: usize,
    area: Rect,
    first_row: u16,
    height: u16,
}

struct Reader {
    study: study::Study,
    leader: crate::leader::Leader,
    text_rect: Rect,
    scroll_lines: i8,
    zen: bool,
    zen_progress: bool,
    opaque: bool,
    sidebar: navigation::Navigation,
    sidebar_rect: Rect,
    splitter: crate::splitter::Splitter,
    footer: [Rect; 7],
    remote_marked: bool,
    sidebar_focus: bool,
    links: links::Links,
    history: Vec<HistoryPosition>,
    future: Vec<HistoryPosition>,
    source: std::path::PathBuf,
    illustrations: Option<crate::shelf::covers::Covers>,
    doc: std::sync::Arc<Document>,
    pager: Pager,
    mode: LayoutMode,
    layout_panel: Option<layout::Panel>,
    switch_mode: bool,
    page_width: u16,
    gap: u16,
    theme: Theme,
    palette: Option<theme_picker::Picker>,
    menu: Option<TableState>,
    menu_rows: Rect,
    nav: Option<navigation::Navigation>,
    search: search::Search,
    word_flash: word_flash::WordFlash,
    marks: Vec<botui_core::Anchor>,
    margin: i8,
    bindings: std::collections::HashMap<char, Action>,
    dirty: bool,
    finished: bool,
}

impl Reader {
    fn leader_context(&self) -> crate::leader::Context {
        if self.doc.markdown.is_some() {
            crate::leader::Context::Markdown
        } else {
            crate::leader::Context::Text
        }
    }

    fn study_input(&mut self, event: &Event) -> Option<Action> {
        if self.menu.is_some()
            || self.nav.is_some()
            || self.palette.is_some()
            || self.layout_panel.is_some()
            || self.sidebar_focus
        {
            return None;
        }
        if let Event::Mouse(m) = event {
            if self.sidebar_rect.contains(Position::new(m.column, m.row)) && !self.study.dragging()
            {
                return None;
            }
            if !self.study.mapping()
                && matches!(
                    m.kind,
                    MouseEventKind::Down(MouseButton::Left | MouseButton::Right)
                )
                && self.text_rect.contains(Position::new(m.column, m.row))
            {
                self.study.begin_frame();
                let view = self.pager.viewport_rows();
                for c in 0..view.columns() {
                    let x = self.text_rect.x + c as u16 * (view.width() + view.gutter());
                    for (y, line) in view
                        .lines(c)
                        .iter()
                        .take(usize::from(self.text_rect.height))
                        .enumerate()
                    {
                        self.study.map_line(
                            &self.doc,
                            line,
                            Rect::new(
                                x + line.indent,
                                self.text_rect.y + y as u16,
                                view.width().saturating_sub(line.indent),
                                1,
                            ),
                        );
                    }
                }
            }
        }
        let link = if let Event::Mouse(m) = event {
            self.links.hit(Position::new(m.column, m.row))
        } else {
            None
        };
        let action = self.study.input(event, &self.doc, &mut self.pager, link);
        if matches!(event, Event::Key(_))
            && action == Some(Action::Resize)
            && let Some(at) = self.study.cursor
            && !self.study.visible(at)
        {
            self.pager.reveal_cursor(at);
            self.dirty = true;
        }
        action
    }
    fn new(
        doc: Document,
        area: Rect,
        saved: Option<Saved>,
        source: &Path,
        remote: Option<botui_core::Anchor>,
        settings: &crate::settings::Settings,
    ) -> Self {
        let mode = if saved.is_none() && settings.reading_layout.as_deref() == Some("single") {
            LayoutMode::Single
        } else if (saved.is_none() && settings.reading_layout.as_deref() == Some("scroll"))
            || saved.as_ref().is_some_and(|s| s.scrolling)
        {
            LayoutMode::Scroll
        } else if saved.as_ref().is_some_and(|s| !s.spread) {
            LayoutMode::Single
        } else {
            LayoutMode::Spread
        };
        let margin = saved.as_ref().map_or(0, |s| s.margin);
        let page_width = layout::width_setting(settings.page_width.unwrap_or_else(|| {
            if margin == 0 {
                0
            } else {
                78u16.saturating_add_signed(-6 * i16::from(margin))
            }
        }));
        let gap = settings.column_gap.unwrap_or(4).min(32);
        let mut splitter = crate::splitter::Splitter::new(
            crate::splitter::Pane::Contents,
            settings.contents_width,
        );
        let (_, page) = layout::panes(area, &mut splitter, settings.zen);
        let reading_frame =
            crate::appearance::ReadingFrame::new(page, settings.zen, settings.zen_progress);
        let (text, mode_for_size, actual_gap) =
            layout::geometry(&reading_frame, mode, page_width, gap, settings.zen);
        let text = controls::above_status(text, area);
        let image_cell = if doc.image_sizes.is_empty() {
            (1, 2)
        } else {
            let cell = crate::cover::graphics().font_size();
            (cell.width, cell.height)
        };
        let resume = remote
            .or_else(|| saved.as_ref().map(|s| s.anchor))
            .map(|anchor| ViewPosition {
                anchor,
                illustration: saved
                    .as_ref()
                    .filter(|s| s.anchor == anchor)
                    .and_then(|s| s.illustration),
            });
        let pager = Pager::for_display_position_with_pagination(
            &doc,
            (text.width, text.height),
            mode_for_size,
            actual_gap,
            ReflowOptions {
                typography: settings.typography(),
                repair_printed: !settings.preserve_printed_lines,
                inline_images: !settings.hide_inline_images,
                paragraph_indent: settings.paragraph_indent,
                paragraph_spacing: settings.paragraph_spacing,
                compact_spacing: settings.compact_spacing(),
            },
            resume,
            image_cell,
            settings.pagination_mode,
        );
        let mut word_flash = word_flash::WordFlash::default();
        if settings.pagination_mode == PaginationMode::Chapter
            && mode != LayoutMode::Scroll
            && let Some(anchor) = remote
        {
            word_flash.show(&doc, anchor);
        }
        let marks = saved.as_ref().map(|s| s.marks.clone()).unwrap_or_default();
        let theme = settings
            .theme
            .as_deref()
            .or_else(|| saved.as_ref().map(|s| s.theme.as_str()))
            .map_or(Theme::TERMINAL, Theme::from_id);
        let links = links::Links::new(&doc);
        let sidebar = navigation::Navigation::new(Kind::Contents, &doc, pager.anchor());
        Self {
            study: study::Study::default(),
            leader: crate::leader::Leader::new(),
            text_rect: Rect::default(),
            scroll_lines: settings.scroll_lines.unwrap_or(1).clamp(1, 12) as i8,
            zen: settings.zen,
            zen_progress: settings.zen_progress,
            opaque: settings.opaque_background,
            sidebar,
            sidebar_rect: Rect::default(),
            splitter,
            footer: [Rect::default(); 7],
            remote_marked: false,
            sidebar_focus: false,
            links,
            history: vec![],
            future: vec![],
            source: source.into(),
            illustrations: None,
            doc: std::sync::Arc::new(doc),
            pager,
            mode,
            page_width,
            gap,
            layout_panel: None,
            switch_mode: false,
            theme,
            palette: None,
            menu: None,
            menu_rows: Rect::default(),
            nav: None,
            search: Default::default(),
            word_flash,
            marks,
            margin,
            bindings: Default::default(),
            dirty: false,
            finished: saved.as_ref().is_some_and(|s| s.finished),
        }
    }

    fn save_appearance(
        &self,
        action: Action,
        store: Option<&LocalState>,
        notices: &mut Vec<String>,
    ) {
        if let Some(store) = store
            && matches!(
                action,
                Action::Pick(theme_picker::Edit::Accept(_))
                    | Action::Single
                    | Action::Spread
                    | Action::Scroll
            )
        {
            if let Err(e) =
                store.update_book_preferences(|p: &mut crate::settings::book::BookSettings| {
                    match action {
                        Action::Pick(_) => p.theme = Some(self.theme.id()),
                        _ => {
                            p.reading_layout = Some(
                                match self.mode {
                                    LayoutMode::Single => "single",
                                    LayoutMode::Spread => "spread",
                                    LayoutMode::Scroll => "scroll",
                                }
                                .into(),
                            )
                        }
                    }
                })
            {
                notices.push(e.to_string());
            }
            return;
        }
        if matches!(action, Action::Single | Action::Spread | Action::Scroll) {
            return;
        }
        if matches!(action, Action::Margin(_) | Action::FullWidth)
            && let Some(store) = store
        {
            let local = (|| -> io::Result<bool> {
                if crate::settings::book::BookSettings::load(Some(store))?
                    .page_width
                    .is_some()
                {
                    store.update_book_preferences(
                        |p: &mut crate::settings::book::BookSettings| {
                            p.page_width = Some(self.page_width)
                        },
                    )?;
                    return Ok(true);
                }
                Ok(false)
            })();
            match local {
                Ok(true) => return,
                Err(e) => {
                    notices.push(e.to_string());
                    return;
                }
                Ok(false) => (),
            }
        }
        let result = crate::settings::Settings::update(|s| match action {
            Action::Gap(_) => s.column_gap = Some(self.gap),
            Action::Margin(_) | Action::FullWidth => s.page_width = Some(self.page_width),
            Action::Zen => s.zen = self.zen,
            Action::ZenProgress => s.zen_progress = self.zen_progress,
            Action::Background => s.opaque_background = self.opaque,
            Action::Pick(theme_picker::Edit::Accept(_)) => s.theme = Some(self.theme.id()),
            _ => (),
        });
        if let Err(e) = result {
            notices.push(e.to_string());
        }
    }

    fn apply_typography(
        &mut self,
        term: &AppTerminal,
        settings: &crate::settings::Settings,
    ) -> io::Result<()> {
        let size = term.size()?;
        let screen = Rect::new(0, 0, size.width, size.height);
        let (_, reading_frame, _) = self.reading_panes(screen);
        let (text, mode, gap) = layout::geometry(
            &reading_frame,
            self.mode,
            self.page_width,
            self.gap,
            self.zen,
        );
        let text = controls::above_status(text, screen);
        self.pager.reconfigure(
            &self.doc,
            text.width,
            text.height,
            mode,
            gap,
            ReflowOptions {
                typography: settings.typography(),
                repair_printed: !settings.preserve_printed_lines,
                inline_images: !settings.hide_inline_images,
                paragraph_indent: settings.paragraph_indent,
                paragraph_spacing: settings.paragraph_spacing,
                compact_spacing: settings.compact_spacing(),
            },
        );
        self.search.reflow(&self.pager, &self.doc);
        self.flash_chapter_position();
        Ok(())
    }
    fn flash_chapter_position(&mut self) {
        if self.mode != LayoutMode::Scroll
            && self.pager.pagination_mode() == PaginationMode::Chapter
        {
            self.word_flash.show(&self.doc, self.pager.anchor());
        } else {
            self.word_flash.clear();
        }
    }
    fn draw_resource_background(&mut self, frame: &mut Frame) -> bool {
        self.draw(frame, None);
        self.pager.start_pagination();
        self.pager.is_paginating() || self.illustrations.as_ref().is_some_and(|p| p.busy())
    }

    fn reading_panes(&mut self, screen: Rect) -> (Rect, crate::appearance::ReadingFrame, Rect) {
        let controls = controls::last_row(screen);
        let (sidebar, page) = layout::panes(screen, &mut self.splitter, self.zen);
        let frame = crate::appearance::ReadingFrame::new(page, self.zen, self.zen_progress)
            .with_focus(!sidebar.is_empty() && !self.sidebar_focus);
        (sidebar, frame, controls)
    }

    fn prepare_page(&mut self, screen: Rect) -> (crate::appearance::ReadingFrame, Rect) {
        let (sidebar, reading_frame, controls) = self.reading_panes(screen);
        self.sidebar_rect = sidebar;
        if self.sidebar_rect.is_empty() {
            self.sidebar_focus = false;
        }
        let (text, actual, gap) = layout::geometry(
            &reading_frame,
            self.mode,
            self.page_width,
            self.gap,
            self.zen,
        );
        let text = controls::above_status(text, screen);
        self.text_rect = text;
        if !self.doc.image_sizes.is_empty() {
            let cell = crate::cover::graphics().font_size();
            self.pager.set_image_cell(cell.width, cell.height);
        }
        if std::mem::take(&mut self.switch_mode) {
            self.pager
                .switch_mode(&self.doc, text.width, text.height, actual, gap);
            self.flash_chapter_position();
        } else if self
            .pager
            .resize_for_display(&self.doc, text.width, text.height, actual, gap)
        {
            self.flash_chapter_position();
        }
        (reading_frame, controls)
    }

    fn draw_latest(&mut self, term: &mut AppTerminal, notice: Option<&str>) -> io::Result<()> {
        let mut size = term.size()?;
        let mut update = term.begin_resize(size)?;
        // A second resize can arrive while laying out a chapter. Finish the
        // layout for the new grid before Ratatui clears and redraws the screen.
        let (reading_frame, controls) = loop {
            let page = self.prepare_page(size.into());
            let current = term.size()?;
            if current == size {
                break page;
            }
            crate::cover::resized();
            size = current;
        };
        term.publish(
            |frame| {
                if frame.area().as_size() == size {
                    self.draw_page(frame, notice, reading_frame, controls);
                } else {
                    self.draw(frame, notice);
                }
                crate::settings::paint_book(
                    frame,
                    self.palette.as_ref().map_or(self.theme, |p| p.preview()),
                );
            },
            &mut update,
        )?;
        Ok(())
    }

    fn draw(&mut self, frame: &mut Frame, notice: Option<&str>) {
        let (reading_frame, controls) = self.prepare_page(frame.area());
        self.draw_page(frame, notice, reading_frame, controls);
    }

    fn draw_page(
        &mut self,
        frame: &mut Frame,
        notice: Option<&str>,
        reading_frame: crate::appearance::ReadingFrame,
        controls: Rect,
    ) {
        let screen = frame.area();
        let text = self.text_rect;
        self.links.clear();
        self.study.begin_frame();
        let show_progress =
            self.zen && self.zen_progress && screen.height > 1 && !self.links.visiting();
        let view = self.pager.viewport_rows();
        let pages = (!self.pager.is_paginating())
            .then(|| (self.pager.page_number(), self.pager.page_count()));
        let reading_theme = self.palette.as_ref().map_or(self.theme, |p| p.preview());
        let _preview_dialogs = crate::settings::reader_dialogs(reading_theme);
        let mut style = reading_theme.style();
        if !crate::settings::opaque() {
            style.bg = Some(ratatui::style::Color::Reset);
        }
        frame.render_widget(Block::new().style(style), screen);
        if let Some(p) = &mut self.illustrations {
            p.collect();
        }
        let mut figures: Vec<VisibleFigure> = Vec::new();
        for c in 0..view.columns() {
            let x = text
                .x
                .saturating_add(c as u16 * (view.width() + view.gutter()));
            for (y, line) in view
                .lines(c)
                .iter()
                .take(usize::from(text.height))
                .enumerate()
            {
                let rect = Rect::new(
                    x + line.indent,
                    text.y + y as u16,
                    view.width() - line.indent,
                    1,
                );
                for figure in &line.figures {
                    let image = Rect::new(rect.x.saturating_add(figure.x), rect.y, figure.width, 1)
                        .intersection(rect);
                    if image.is_empty() {
                        continue;
                    }
                    if let Some(part) = figures.iter_mut().rev().find(|part| {
                        part.id == figure.reference
                            && part.area.x == image.x
                            && part.area.width == image.width
                            && part.area.bottom() == image.y
                            && part.first_row + part.area.height == figure.row
                    }) {
                        part.area.height += 1;
                    } else {
                        figures.push(VisibleFigure {
                            id: figure.reference,
                            area: image,
                            first_row: figure.row,
                            height: figure.height,
                        });
                    }
                }
                let mut para = Paragraph::new(self.search.line(line, reading_theme)).style(
                    if line.table_header {
                        style.add_modifier(Modifier::BOLD)
                    } else {
                        style
                    },
                );
                if line.heading {
                    para = para.style(
                        style
                            .fg(reading_theme.heading())
                            .add_modifier(Modifier::BOLD),
                    );
                }
                frame.render_widget(para, rect);
                // Paragraph paints the whole row. Apply book decoration afterwards.
                paint_surfaces(frame, line, rect, 0, reading_theme, style);
                self.links.mark_line(frame, &self.doc, line, rect);
                self.word_flash
                    .paint(&self.doc, line, rect, frame.buffer_mut());
                if self.study.mapping() {
                    self.study.map_line(&self.doc, line, rect);
                }
            }
        }
        let upcoming: Vec<_> = self
            .pager
            .nearby_illustrations()
            .take(8usize.saturating_sub(figures.len()))
            .map(|f| (f.reference, Size::new(f.width, f.height), true))
            .collect();
        if !figures.is_empty() || !upcoming.is_empty() {
            let pictures = self
                .illustrations
                .get_or_insert_with(|| crate::shelf::covers::Covers::with_capacity(8));
            pictures.fit_visible(figures.len() + upcoming.len());
            let graphics = crate::cover::graphics();
            let visible = figures
                .iter()
                .map(|f| (f.id, Size::new(f.area.width, f.height), false));
            for (id, bounds, prefetch) in visible.chain(upcoming) {
                if let botui_core::Target::Image(member) = &self.doc.references[id].target {
                    let member = member.clone();
                    let path = self.source.clone();
                    let picker = graphics.clone();
                    pictures.request_with(
                        id as u64,
                        prefetch,
                        move || Ok(botui_formats::illustration(&path, &member).ok()),
                        move |bytes| crate::cover::Picture::inline(bytes, &picker, bounds),
                    );
                }
            }
            for VisibleFigure {
                id,
                area,
                first_row,
                height,
            } in figures
            {
                let entry = &self.doc.references[id];
                if let Some(picture) = pictures.image(id as u64) {
                    if let Some(hit) =
                        picture.draw_inline(frame, area, &graphics, first_row, height)
                    {
                        self.links.image(hit, id);
                    }
                } else {
                    self.links.image(area, id);
                    frame.render_widget(
                        Paragraph::new(format!("Illustration · {}", entry.label))
                            .centered()
                            .style(style.add_modifier(Modifier::DIM)),
                        Rect {
                            y: area.y + area.height / 2,
                            height: 1,
                            ..area
                        },
                    );
                }
            }
        }
        if let Some(pictures) = &mut self.illustrations {
            pictures.finish_frame();
        }
        self.study.extend_drag();
        self.study.paint(frame);
        let show_position = !self.zen || show_progress;
        let position =
            show_position.then(|| crate::appearance::progress_label(pages, self.reading_percent()));
        let chapter = if show_position {
            self.pager
                .chapter_progress()
                .map(|chapter| crate::appearance::SectionPosition {
                    index: chapter.ordinal,
                    total: chapter.total,
                    title: chapter.toc_title.or(chapter.title),
                    percent: if self.finished && self.pager.at_end() {
                        Some(100)
                    } else {
                        chapter.percent
                    },
                })
        } else {
            None
        };
        if !self.zen {
            if !self.sidebar_rect.is_empty() {
                self.sidebar.draw_sidebar(
                    frame,
                    style,
                    self.sidebar_rect,
                    self.pager.anchor(),
                    self.sidebar_focus,
                );
            }
            let foot = Rect::new(
                screen.x,
                screen.bottom().saturating_sub(1),
                screen.width,
                screen.height.min(1),
            );
            let count = match (self.mode, reading_frame.has_border()) {
                (LayoutMode::Scroll, true) => " Scroll ".to_owned(),
                (_, true) => String::new(),
                (mode, false) => {
                    let prefix = if mode == LayoutMode::Scroll {
                        "Scroll · "
                    } else {
                        ""
                    };
                    let mut status =
                        format!(" {prefix}{} ", position.as_deref().unwrap_or_default());
                    if let Some(chapter) = chapter {
                        status.push_str(&chapter.compact_label());
                    }
                    status
                }
            };
            let source = self.doc.markdown.as_ref().map_or("", |md| {
                if md.show_source {
                    "Formatted"
                } else {
                    "Source"
                }
            });
            self.footer = crate::reader_footer::draw(
                frame,
                foot,
                style,
                if reading_frame.has_border() {
                    ""
                } else {
                    &self.doc.title
                },
                &count,
                [
                    ("Space z", "Zen"),
                    ("Space o", "Contents"),
                    (
                        "Space r f",
                        if self.links.has_visible() {
                            "Links"
                        } else {
                            ""
                        },
                    ),
                    ("F9", source),
                    ("Space m", "Menu"),
                    ("F2", "Settings"),
                    ("F1", "Help"),
                ],
            );
        }
        reading_frame.draw(
            frame,
            style,
            reading_frame
                .has_border()
                .then_some(self.doc.title.as_str()),
            position
                .as_deref()
                .filter(|_| self.zen || reading_frame.has_border()),
            chapter,
        );
        if self.links.visiting() && !self.study.controls_visible() {
            let footer = Rect::new(
                screen.x,
                screen.bottom().saturating_sub(1),
                screen.width.min(37),
                screen.height.min(1),
            );
            frame.render_widget(Clear, footer);
            self.links.controls(frame, footer, style);
        }
        if self.study.controls_visible() {
            self.footer.fill(Rect::default());
            self.study.controls(frame, controls, style);
        }
        let context = self.leader_context();
        self.leader.draw(frame, context);
        if let Some(panel) = &mut self.layout_panel {
            panel.draw(
                frame,
                style,
                (view.width(), self.page_width),
                self.pager.reflow_options().typography,
                self.gap,
                (view.columns() == 2).then(|| view.gutter()),
            );
            return;
        }
        if let Some(picker) = &mut self.palette {
            picker.draw(frame);
            return;
        }
        if let Some(nav) = &mut self.nav {
            nav.draw(frame, style);
            return;
        }
        let Some(list) = &mut self.menu else { return };
        let popup = centered(
            screen,
            54,
            COMMANDS.len() as u16 + 3 + u16::from(notice.is_some()),
        );
        let finish_menu = crate::overlay::backdrop(frame, popup);
        frame.render_widget(Clear, popup);
        let block = crate::appearance::block().title(" Reading ").style(style);
        let inner = block.inner(popup);
        frame.render_widget(block, popup);
        let header = inner.height.min(1);
        let status = u16::from(notice.is_some() && inner.height >= 4);
        let counter = pages.map_or_else(String::new, |(page, total)| format!("{page}/{total}"));
        let count_width = inner.width.min(counter.len() as u16);
        frame.render_widget(
            Paragraph::new(self.doc.title.as_str()),
            Rect::new(
                inner.x,
                inner.y,
                inner.width.saturating_sub(count_width + 1),
                header,
            ),
        );
        frame.render_widget(
            Paragraph::new(counter),
            Rect::new(inner.right() - count_width, inner.y, count_width, header),
        );
        self.menu_rows = Rect::new(
            inner.x,
            inner.y + header,
            inner.width,
            inner.height.saturating_sub(header + status),
        );
        let rows = COMMANDS.iter().map(|action| {
            let marked = matches!(
                (action, self.mode),
                (Action::Single, LayoutMode::Single)
                    | (Action::Spread, LayoutMode::Spread)
                    | (Action::Scroll, LayoutMode::Scroll)
            );
            let (label, shortcut): (std::borrow::Cow<'_, str>, _) = match action {
                Action::Resume => ("Resume".into(), "Esc"),
                Action::Previous => ("Previous page".into(), "←"),
                Action::Next => ("Next page".into(), "→"),
                Action::Single => ("Single page".into(), "1 / Space l 1"),
                Action::Spread => ("Two-page spread".into(), "2 / Space l 2"),
                Action::Scroll => ("Continuous scroll".into(), "3 / Space l 3"),
                Action::Theme => (format!("Theme · {}", self.theme.name()).into(), "Space t"),
                Action::Navigate(Kind::Contents) => ("Contents".into(), "Space o"),
                Action::Navigate(Kind::Search) => ("Find in book".into(), "Space f"),
                Action::SearchResults => ("Reopen search results".into(), "F5"),
                Action::FindAgain(true) => ("Next search match".into(), "F3"),
                Action::FindAgain(false) => ("Previous search match".into(), "F4"),
                Action::Mark => (
                    (if self.remote_marked || self.marks.contains(&self.pager.anchor()) {
                        "Remove bookmark"
                    } else {
                        "Add bookmark"
                    })
                    .into(),
                    "Space b a",
                ),
                Action::Bookmarks => ("Bookmarks".into(), "Space b l"),
                Action::Layout => ("Page width & column gap".into(), "Space l w"),
                Action::ZenProgress => (
                    format!(
                        "Zen frame & progress · {}",
                        if self.zen_progress { "On" } else { "Off" }
                    )
                    .into(),
                    "Space p",
                ),
                Action::Zen => (
                    (if self.zen {
                        "Show contents sidebar"
                    } else {
                        "Enter zen mode"
                    })
                    .into(),
                    "Space z",
                ),
                Action::Borders => (
                    format!(
                        "Borders · {}",
                        crate::appearance::BordersStyle::current().label()
                    )
                    .into(),
                    "Space l u",
                ),
                Action::Background => (
                    format!(
                        "Background · {}",
                        if self.opaque { "Opaque" } else { "Transparent" }
                    )
                    .into(),
                    "Space l f",
                ),
                Action::Study(study::Command::Normal) => {
                    ("Text cursor / normal mode".into(), "Space c")
                }
                Action::Study(study::Command::Highlight) => {
                    ("Highlight selected text".into(), "Space a h")
                }
                Action::Note => ("Comment on selected text".into(), "Space a n"),
                Action::Navigate(Kind::Notes) => ("Highlights & notes".into(), "Space a a"),
                Action::Export => ("Export notes to Markdown".into(), "Space a e"),
                Action::Statistics => ("Reading statistics".into(), "Space r s"),
                Action::Settings => ("Settings".into(), "F2"),
                Action::Help => ("Keyboard shortcuts".into(), "F1"),
                Action::Cover => ("View cover".into(), "Space r c"),
                Action::OpenTable => ("Open table on this page".into(), "Space r t"),
                Action::Resources => (
                    format!("Links & illustrations ({})", self.doc.references.len()).into(),
                    "Space r i",
                ),
                Action::JumpBack => ("Jump back".into(), "Space r ["),
                Action::JumpForward => ("Jump forward".into(), "Space r ]"),
                Action::Close => ("Close book".into(), "Q / Space q"),
                _ => unreachable!("visible reader menu command"),
            };
            Row::new([
                ratatui::text::Line::from(if marked { "✓" } else { "" }),
                ratatui::text::Line::from(label),
                ratatui::text::Line::from(shortcut).right_aligned(),
            ])
        });
        use ratatui::layout::Constraint;
        frame.render_stateful_widget(
            Table::new(
                rows,
                [
                    Constraint::Length(2),
                    Constraint::Fill(1),
                    Constraint::Length(9),
                ],
            )
            .row_highlight_style(style.add_modifier(Modifier::REVERSED)),
            self.menu_rows,
            list,
        );
        if let (1, Some(error)) = (status, notice) {
            frame.render_widget(
                Paragraph::new(error),
                Rect::new(inner.x, inner.bottom() - 1, inner.width, 1),
            );
        }
        finish_menu(frame);
    }

    fn input(&self, event: Event) -> Option<Action> {
        self.input_with_bindings(event, true)
    }
    fn input_with_bindings(&self, event: Event, custom: bool) -> Option<Action> {
        if self.menu.is_some()
            && let Event::Key(k) = &event
        {
            return match k.code {
                KeyCode::Char('q' | 'Q') | KeyCode::Esc => Some(Action::Resume),
                KeyCode::Up | KeyCode::BackTab | KeyCode::Char('k') => Some(Action::Up),
                KeyCode::Down | KeyCode::Tab | KeyCode::Char('j') => Some(Action::Down),
                KeyCode::Enter => self
                    .menu
                    .as_ref()?
                    .selected()
                    .and_then(|i| COMMANDS.get(i).copied()),
                KeyCode::F(1) => Some(Action::Help),
                KeyCode::F(2) => Some(Action::Settings),
                KeyCode::Char('c') if k.modifiers.contains(KeyModifiers::CONTROL) => {
                    Some(Action::Close)
                }
                _ => None,
            };
        }
        match event {
            Event::Resize(..) => Some(Action::Resize),
            Event::Key(k)
                if k.kind != KeyEventKind::Release
                    && k.code == KeyCode::Char('c')
                    && k.modifiers.contains(KeyModifiers::CONTROL) =>
            {
                Some(Action::Close)
            }
            _ if self.layout_panel.is_some() => self.layout_panel.as_ref()?.input(event),
            _ if self.nav.is_some() => self.nav.as_ref()?.input(event).map(Action::NavEdit),
            _ if self.palette.is_some() => self.palette.as_ref()?.input(event).map(Action::Pick),
            Event::Key(k)
                if k.kind != KeyEventKind::Release
                    && self.menu.is_none()
                    && !self.sidebar_rect.is_empty()
                    && k.code == KeyCode::Tab =>
            {
                Some(Action::Focus)
            }
            Event::Mouse(m)
                if self.menu.is_none()
                    && self.sidebar_rect.contains(Position::new(m.column, m.row)) =>
            {
                self.sidebar.input(Event::Mouse(m)).map(Action::Sidebar)
            }
            Event::Key(k)
                if self.sidebar_focus
                    && self.menu.is_none()
                    && matches!(
                        k.code,
                        KeyCode::Up
                            | KeyCode::Left
                            | KeyCode::Right
                            | KeyCode::Down
                            | KeyCode::PageUp
                            | KeyCode::PageDown
                            | KeyCode::Enter
                            | KeyCode::Home
                            | KeyCode::End
                    ) =>
            {
                self.sidebar.input(Event::Key(k)).map(Action::Sidebar)
            }
            Event::Key(k) if k.kind != KeyEventKind::Release => match k.code {
                KeyCode::Esc | KeyCode::Char(',' | 'q' | 'Q')
                    if self.links.visiting() && self.menu.is_none() =>
                {
                    Some(Action::ReturnLink)
                }
                KeyCode::Enter if self.links.visiting() && self.menu.is_none() => {
                    Some(Action::KeepLink)
                }
                KeyCode::F(9) if self.doc.markdown.is_some() => Some(Action::Source),
                KeyCode::F(6) if self.menu.is_none() => Some(Action::VisibleLinks),
                KeyCode::F(2) => Some(Action::Settings),
                KeyCode::F(5) => Some(Action::SearchResults),
                KeyCode::F(4) => Some(Action::FindAgain(false)),
                KeyCode::F(3) => Some(Action::FindAgain(
                    !k.modifiers.contains(KeyModifiers::SHIFT),
                )),
                KeyCode::Char('?') | KeyCode::F(1) => Some(Action::Help),
                KeyCode::Char(c)
                    if self.menu.is_none()
                        && !k
                            .modifiers
                            .intersects(KeyModifiers::ALT | KeyModifiers::CONTROL)
                        && custom
                        && self.bindings.contains_key(&c) =>
                {
                    self.bindings.get(&c).copied()
                }
                KeyCode::Char('n') => Some(Action::FindAgain(true)),
                KeyCode::Char('N') => Some(Action::FindAgain(false)),
                KeyCode::Char('c' | 'C')
                    if !k
                        .modifiers
                        .intersects(KeyModifiers::ALT | KeyModifiers::CONTROL) =>
                {
                    Some(Action::Cover)
                }
                KeyCode::Char('i' | 'I') => Some(Action::Resources),
                KeyCode::Char('s' | 'S') => Some(Action::Statistics),
                KeyCode::Char('a' | 'A') => Some(Action::Navigate(Kind::Notes)),
                KeyCode::Char('e' | 'E') => Some(Action::Export),
                KeyCode::Char(',') => Some(Action::JumpBack),
                KeyCode::Char('.') => Some(Action::JumpForward),
                KeyCode::Char('u' | 'U') => Some(Action::Borders),
                KeyCode::Char('%') => Some(Action::ZenProgress),
                KeyCode::Char('z' | 'Z') => Some(Action::Zen),
                KeyCode::Char('f' | 'F') => Some(Action::Background),
                KeyCode::Char('w' | 'W') => Some(Action::Layout),
                KeyCode::Char('{') => Some(Action::Gap(-1)),
                KeyCode::Char('}') => Some(Action::Gap(1)),
                KeyCode::Char('[') => Some(Action::Margin(-1)),
                KeyCode::Char(']') => Some(Action::Margin(1)),
                KeyCode::Char('o' | 'O') => Some(Action::Navigate(Kind::Contents)),
                KeyCode::Char('/') => Some(Action::Navigate(Kind::Search)),
                KeyCode::Char('g' | 'G') => Some(Action::Bookmarks),
                KeyCode::Char('b' | 'B') => Some(Action::Mark),
                KeyCode::Char('q' | 'Q') => Some(Action::Close),
                KeyCode::Esc if self.search.highlighted() => Some(Action::HideSearch),
                KeyCode::Char('m' | 'M') | KeyCode::Esc => Some(Action::Menu),
                KeyCode::Char('1') => Some(Action::Single),
                KeyCode::Char('2') => Some(Action::Spread),
                KeyCode::Char('3') => Some(Action::Scroll),
                KeyCode::Char('t' | 'T') => Some(Action::Theme),
                KeyCode::Up | KeyCode::Char('k') | KeyCode::BackTab if self.menu.is_some() => {
                    Some(Action::Up)
                }
                KeyCode::Down | KeyCode::Char('j') | KeyCode::Tab if self.menu.is_some() => {
                    Some(Action::Down)
                }
                KeyCode::Enter | KeyCode::Char(' ') if self.menu.is_some() => self
                    .menu
                    .as_ref()?
                    .selected()
                    .and_then(|i| COMMANDS.get(i).copied()),
                KeyCode::Tab | KeyCode::Enter => Some(Action::Menu),
                KeyCode::Down | KeyCode::Char('j') if self.mode == LayoutMode::Scroll => {
                    Some(Action::Lines(1))
                }
                KeyCode::Up | KeyCode::Char('k') if self.mode == LayoutMode::Scroll => {
                    Some(Action::Lines(-1))
                }
                KeyCode::Left
                | KeyCode::PageUp
                | KeyCode::Up
                | KeyCode::Char('h' | 'k')
                | KeyCode::Backspace => Some(Action::Previous),
                KeyCode::Right
                | KeyCode::PageDown
                | KeyCode::Down
                | KeyCode::Char('l' | 'j' | ' ') => Some(Action::Next),
                _ => None,
            },
            Event::Mouse(m)
                if self.menu.is_none()
                    && m.kind == MouseEventKind::Down(MouseButton::Left)
                    && self.links.hit(Position::new(m.column, m.row)).is_some() =>
            {
                self.links.hit(Position::new(m.column, m.row))
            }
            Event::Mouse(m)
                if !self.zen
                    && self.menu.is_none()
                    && m.kind == MouseEventKind::Down(MouseButton::Left)
                    && self
                        .footer
                        .iter()
                        .any(|r| r.contains(Position::new(m.column, m.row))) =>
            {
                self.footer
                    .iter()
                    .position(|r| r.contains(Position::new(m.column, m.row)))
                    .map(|i| {
                        [
                            Action::Zen,
                            Action::Navigate(Kind::Contents),
                            Action::VisibleLinks,
                            Action::Source,
                            Action::Menu,
                            Action::Settings,
                            Action::Help,
                        ][i]
                    })
            }
            Event::Mouse(m) => match m.kind {
                MouseEventKind::Down(MouseButton::Left) if self.menu.is_none() => None,
                MouseEventKind::Down(MouseButton::Left) => {
                    if self.menu_rows.contains(Position::new(m.column, m.row)) {
                        let row =
                            self.menu.as_ref()?.offset() + usize::from(m.row - self.menu_rows.y);
                        COMMANDS.get(row).copied()
                    } else {
                        Some(Action::Resume)
                    }
                }
                MouseEventKind::ScrollUp
                    if self.mode == LayoutMode::Scroll && self.menu.is_none() =>
                {
                    Some(Action::Lines(-self.scroll_lines))
                }
                MouseEventKind::ScrollDown
                    if self.mode == LayoutMode::Scroll && self.menu.is_none() =>
                {
                    Some(Action::Lines(self.scroll_lines))
                }
                MouseEventKind::ScrollUp => Some(if self.menu.is_some() {
                    Action::Up
                } else {
                    Action::Previous
                }),
                MouseEventKind::ScrollDown => Some(if self.menu.is_some() {
                    Action::Down
                } else {
                    Action::Next
                }),
                _ => None,
            },
            _ => None,
        }
    }

    fn remember_jump(&mut self, before: HistoryPosition, place: botui_core::Anchor) {
        self.word_flash.clear();
        if self.study.normal {
            self.study.cursor = Some(place);
        }
        if self.pager.history_position() != before {
            self.history.push(before);
            self.future.clear();
        }
    }
    fn jump_to(&mut self, place: botui_core::Anchor) {
        let before = self.pager.history_position();
        if before.logical.anchor != place || before.logical.illustration.is_some() {
            self.pager.seek(place);
        }
        self.remember_jump(before, place);
    }
    fn jump_to_match(&mut self, place: botui_core::Anchor) {
        let before = self.pager.history_position();
        self.pager.seek_chapter(place);
        self.remember_jump(before, place);
        if self.mode != LayoutMode::Scroll {
            self.word_flash.show(&self.doc, place);
        }
    }
    fn apply(&mut self, action: Action) {
        match action {
            Action::Source => {
                if std::sync::Arc::make_mut(&mut self.doc).toggle_markdown() {
                    let options = crate::settings::Settings::load().unwrap_or_default();
                    self.pager.reconfigure(
                        &self.doc,
                        self.text_rect.width,
                        self.text_rect.height,
                        self.mode,
                        self.gap,
                        ReflowOptions {
                            typography: options.typography(),
                            repair_printed: false,
                            inline_images: !options.hide_inline_images,
                            paragraph_indent: 0,
                            paragraph_spacing: None,
                            compact_spacing: true,
                        },
                    );
                    self.search.reflow(&self.pager, &self.doc);
                    self.menu = None;
                }
            }
            Action::ReturnLink => {
                if let Some(position) = self.links.back() {
                    self.word_flash.clear();
                    self.pager.restore_history(&self.doc, position);
                }
            }
            Action::KeepLink => {
                if let Some(origin) = self.links.origin() {
                    self.history.push(origin);
                    self.future.clear();
                }
                self.links.keep();
                self.dirty = true;
            }
            Action::JumpBack | Action::JumpForward => {
                let (from, to) = if action == Action::JumpBack {
                    (&mut self.history, &mut self.future)
                } else {
                    (&mut self.future, &mut self.history)
                };
                if let Some(position) = from.pop() {
                    self.word_flash.clear();
                    to.push(self.pager.history_position());
                    self.pager.restore_history(&self.doc, position);
                    if self.study.normal {
                        self.study.cursor = Some(position.logical.anchor);
                    }
                    self.dirty = true;
                }
                self.menu = None;
            }
            Action::Focus => self.sidebar_focus = !self.sidebar_focus,
            Action::Sidebar(edit) => {
                self.sidebar_focus = true;
                if let Some(pos) = self.sidebar.edit(edit) {
                    self.jump_to(pos);
                    self.dirty = true;
                    self.sidebar_focus = false;
                }
            }
            Action::ZenProgress => {
                self.zen_progress = !self.zen_progress;
                self.menu = None;
            }
            Action::Zen => {
                self.zen = !self.zen;
                self.sidebar_focus = false;
                self.menu = None;
            }
            Action::Borders => {}
            Action::Background => {
                self.opaque = !self.opaque;
                self.menu = None;
            }
            Action::Navigate(kind) => {
                self.menu = None;
                let mut nav = navigation::Navigation::new(kind, &self.doc, self.pager.anchor());
                if kind == Kind::Search {
                    self.pager.finish_resize();
                    nav.index = Some(self.search.index(&self.pager, &self.doc));
                }
                self.nav = Some(nav);
            }
            Action::HideSearch => {
                self.search.hide();
                self.word_flash.clear();
            }
            Action::SearchResults => {
                self.apply(Action::Navigate(Kind::Search));
                if let Some(nav) = &mut self.nav {
                    self.search.restore(nav);
                }
            }
            Action::FindAgain(forward) => {
                self.menu = None;
                self.pager.finish_resize();
                self.search.update(&self.pager, &self.doc);
                let origin = self
                    .study
                    .cursor
                    .filter(|_| self.study.normal)
                    .unwrap_or(self.pager.anchor());
                if let Some(place) = self.search.advance(origin, forward) {
                    self.jump_to_match(place);
                    self.dirty = true;
                } else {
                    self.apply(Action::Navigate(Kind::Search));
                }
            }
            Action::NavEdit(Edit::Back) => self.nav = None,
            Action::NavEdit(e) => {
                if let Some(nav) = &mut self.nav
                    && let Some(place) = nav.edit(e)
                {
                    if let Edit::Choose(selected) = e
                        && nav.index.is_some()
                    {
                        self.search.select(
                            std::mem::take(&mut nav.query),
                            nav.mode,
                            std::mem::take(&mut nav.hits),
                            selected,
                        );
                        self.jump_to_match(place);
                    } else {
                        self.jump_to(place);
                    }
                    self.dirty = true;
                    self.nav = None;
                }
            }
            Action::Layout => {
                self.menu = None;
                self.layout_panel = if self.layout_panel.is_some() {
                    None
                } else {
                    Some(layout::Panel::new(self.doc.markdown.is_none()))
                };
            }
            Action::Gap(d) => {
                self.gap = self.gap.saturating_add_signed(i16::from(d)).min(32);
            }
            Action::FullWidth => {
                self.page_width = 0;
                self.dirty = true;
            }
            Action::Margin(d) => {
                self.page_width = self
                    .pager
                    .viewport_rows()
                    .width()
                    .saturating_add_signed(-2 * i16::from(d))
                    .clamp(32, 240);
                self.margin = (self.margin + d).clamp(-2, 4);
                self.dirty = true;
                self.menu = None;
            }
            Action::Menu => {
                self.menu = if self.menu.is_some() {
                    None
                } else {
                    Some(TableState::default().with_selected(Some(0)))
                };
            }
            Action::Resume => self.menu = None,
            Action::Up | Action::Down => {
                if let Some(list) = &mut self.menu {
                    let index = list.selected().unwrap_or(0);
                    let next = (index
                        + if action == Action::Down {
                            1
                        } else {
                            COMMANDS.len() - 1
                        })
                        % COMMANDS.len();
                    list.select(Some(next));
                }
            }
            Action::Previous | Action::Next | Action::Lines(_) => {
                self.word_flash.clear();
                self.pager.finish_resize();
                let old = self.pager.position();
                let forward = match action {
                    Action::Lines(n) => {
                        self.pager.scroll(isize::from(n));
                        n > 0
                    }
                    Action::Next => {
                        if self.mode == LayoutMode::Scroll {
                            self.pager.scroll(self.pager.viewport() as isize);
                        } else {
                            self.pager.next();
                        }
                        true
                    }
                    _ => {
                        if self.mode == LayoutMode::Scroll {
                            self.pager.scroll(-(self.pager.viewport() as isize));
                        } else {
                            self.pager.previous();
                        }
                        false
                    }
                };
                self.dirty |= old != self.pager.position();
                if forward && !self.links.visiting() && self.pager.at_end() && !self.finished {
                    self.finished = true;
                    self.dirty = true;
                }
                self.menu = None;
            }
            Action::Single | Action::Spread | Action::Scroll => {
                let mode = match action {
                    Action::Single => LayoutMode::Single,
                    Action::Scroll => LayoutMode::Scroll,
                    _ => LayoutMode::Spread,
                };
                if self.mode != mode {
                    self.switch_mode = true;
                    self.mode = mode;
                    self.dirty = true;
                }
                self.menu = None;
            }
            Action::Theme => {
                self.palette = Some(theme_picker::Picker::new(self.theme));
            }
            Action::Pick(theme_picker::Edit::Cancel) => self.palette = None,
            Action::Pick(theme_picker::Edit::Accept(theme)) => {
                self.dirty |= self.theme != theme;
                self.theme = theme;
                self.palette = None;
                self.menu = None;
            }
            Action::Pick(edit) => {
                if let Some(picker) = &mut self.palette {
                    picker.edit(edit);
                }
            }
            Action::Resources
            | Action::Cover
            | Action::OpenTable
            | Action::VisibleLinks
            | Action::Follow(_)
            | Action::Bookmarks
            | Action::Mark
            | Action::Alignment
            | Action::Hyphenation
            | Action::Settings
            | Action::Help
            | Action::Statistics
            | Action::Note
            | Action::Study(_)
            | Action::Export
            | Action::Resize
            | Action::AnnotationSync
            | Action::Close => {}
        }
    }

    fn saved_position(&self) -> ViewPosition {
        self.links
            .origin()
            .map_or_else(|| self.pager.position(), |origin| origin.logical)
    }

    fn snapshot(&self) -> Saved {
        let position = self.saved_position();
        Saved {
            anchor: position.anchor,
            illustration: position.illustration,
            spread: self.mode == LayoutMode::Spread,
            scrolling: self.mode == LayoutMode::Scroll,
            scan_offset: 0,
            scan_appearance: Default::default(),
            finished: self.finished,
            theme: self.theme.id(),
            marks: self.marks.clone(),
            margin: self.margin,
        }
    }

    fn reading_percent(&self) -> f64 {
        if self.finished && self.pager.at_end() {
            100.
        } else {
            self.doc.text_percentage(self.pager.anchor()).min(99.)
        }
    }

    fn flush(&mut self, state: &mut Option<LocalState>, notices: &mut Vec<String>) {
        if !self.dirty {
            return;
        }
        if let Some(store) = state
            && let Err(e) = store.save_position(&self.snapshot())
        {
            notices.push(format!("Could not save reading position: {e}"));
            *state = None;
        }
        self.dirty = false;
    }
}

/// Apply source decorations after Paragraph has painted the row. A table
/// viewer may show a horizontally scrolled slice of the same layout.
fn paint_surfaces(
    frame: &mut Frame,
    line: &botui_core::PageLine,
    rect: Rect,
    left: u16,
    theme: crate::theme::Theme,
    style: ratatui::style::Style,
) {
    for &surface in &line.surfaces {
        match surface {
            botui_core::Surface::Background { x, width, role } => {
                let start = x.max(left);
                let end = x.saturating_add(width).min(left.saturating_add(rect.width));
                if end > start {
                    frame.buffer_mut().set_style(
                        Rect::new(rect.x + (start - left), rect.y, end - start, 1),
                        ratatui::style::Style::new().bg(markdown_style::background(role, theme)),
                    );
                }
            }
            botui_core::Surface::HeadingRule { double } => {
                for col in rect.x..rect.right() {
                    frame.buffer_mut()[(col, rect.y)]
                        .set_symbol(if double { "═" } else { "─" })
                        .set_style(style.fg(theme.heading()));
                }
            }
            botui_core::Surface::TableDivider { x } => {
                if let Some(x) = x.checked_sub(left).filter(|&x| x < rect.width)
                    && let Some(cell) = frame.buffer_mut().cell_mut((rect.x + x, rect.y))
                {
                    let symbol = match cell.symbol() {
                        " " => "│",
                        "─" if line
                            .surfaces
                            .iter()
                            .any(|s| matches!(s, botui_core::Surface::TableHeaderRule { .. })) =>
                        {
                            "┼"
                        }
                        "─" if line.surfaces.iter().any(|s| {
                            matches!(s, botui_core::Surface::TableEdge { top: true, .. })
                        }) =>
                        {
                            "┬"
                        }
                        "─" => "┴",
                        _ => continue,
                    };
                    cell.set_symbol(symbol)
                        .set_style(style.add_modifier(Modifier::DIM));
                }
            }
            botui_core::Surface::TableHeaderRule { x, width } => {
                let end = x.saturating_add(width);
                for col in x.max(left)..end.min(left.saturating_add(rect.width)) {
                    let glyph = if col == x {
                        "├"
                    } else if col + 1 == end {
                        "┤"
                    } else {
                        "─"
                    };
                    frame.buffer_mut()[(rect.x + (col - left), rect.y)]
                        .set_symbol(glyph)
                        .set_style(style.add_modifier(Modifier::DIM));
                }
            }
            botui_core::Surface::TableEdge { x, width, top } => {
                let end = x.saturating_add(width);
                for col in x.max(left)..end.min(left.saturating_add(rect.width)) {
                    let glyph = if col == x {
                        if top { "┌" } else { "└" }
                    } else if col + 1 == end {
                        if top { "┐" } else { "┘" }
                    } else {
                        "─"
                    };
                    frame.buffer_mut()[(rect.x + (col - left), rect.y)]
                        .set_symbol(glyph)
                        .set_style(style.add_modifier(Modifier::DIM));
                }
            }
        }
    }
}

fn bindings() -> io::Result<std::collections::HashMap<char, Action>> {
    let raw = crate::settings::Settings::load()?.keys;
    raw.into_iter()
        .map(|(key, name)| {
            let action = match name.as_str() {
                "next" => Action::Next,
                "previous" => Action::Previous,
                "menu" => Action::Menu,
                "zen" => Action::Zen,
                "zen-progress" => Action::ZenProgress,
                "background" => Action::Background,
                "borders" => Action::Borders,
                "cover" => Action::Cover,
                "resources" => Action::Resources,
                "open-table" => Action::OpenTable,
                "statistics" => Action::Statistics,
                "note" => Action::Note,
                "notes" => Action::Navigate(Kind::Notes),
                "export-notes" => Action::Export,
                "jump-back" => Action::JumpBack,
                "jump-forward" => Action::JumpForward,
                "theme" => Action::Theme,
                "contents" => Action::Navigate(Kind::Contents),
                "search" => Action::Navigate(Kind::Search),
                "search-results" => Action::SearchResults,
                "next-match" => Action::FindAgain(true),
                "previous-match" => Action::FindAgain(false),
                "bookmark" => Action::Mark,
                "bookmarks" => Action::Bookmarks,
                "single" => Action::Single,
                "spread" => Action::Spread,
                "scroll" => Action::Scroll,
                "page-layout" => Action::Layout,
                "wider-gap" => Action::Gap(1),
                "narrower-gap" => Action::Gap(-1),
                "wider-margins" | "narrower-page" => Action::Margin(1),
                "narrower-margins" | "wider-page" => Action::Margin(-1),
                "close" => Action::Close,
                _ => return Err(io::Error::other(format!("Unknown command {name}"))),
            };
            Ok((key, action))
        })
        .collect()
}

pub fn read_server(
    term: &mut AppTerminal,
    path: &Path,
    doc: Document,
    notices: &mut Vec<String>,
    at: crate::ReadingStart<'_>,
    mut remote: Option<&mut bookmarks::ServerMarks>,
    mut live: Option<&mut crate::live_progress::Live>,
) -> io::Result<(
    Option<botui_bookorbit::progress::Move>,
    f64,
    crate::BookExit,
)> {
    let base = at.server;
    if !doc.image_pages.is_empty() {
        // Image-only EPUB uses a CFI remotely; comics use physical page numbers.
        let kind =
            botui_formats::FileKind::from_path(path).unwrap_or(botui_formats::FileKind::Epub);
        let comic = matches!(
            kind,
            botui_formats::FileKind::Cbz | botui_formats::FileKind::Cbr
        );
        let (change, percent, exit) = crate::scan::read_pages(
            term,
            path,
            kind,
            notices,
            crate::ReadingStart {
                server: if comic { base } else { None },
                ..at
            },
            crate::scan::Source::ImageBook(Box::new(doc)),
            if comic { live } else { None },
        )?;
        return Ok((change, percent.unwrap_or_default(), exit));
    }
    let mut viewer = crate::cover_viewer::Viewer::default();
    let before = notices.len();
    let (mut state, mut saved) = match LocalState::for_file(path).and_then(|store| {
        let saved = store.load()?;
        Ok((Some(store), saved))
    }) {
        Ok(pair) => pair,
        Err(err) => {
            notices.push(format!("Local reading state unavailable: {err}"));
            (None, None)
        }
    };
    let size = term.size()?;
    if base.is_some_and(|p| p.percentage == 0. && p.text.cfi.is_none() && p.text.page.is_none())
        && let Some(local) = &mut saved
    {
        local.anchor = botui_core::Anchor::default();
        local.illustration = None;
        local.finished = false;
    }
    let mut resume_anchor = None;
    let mut can_sync = base.is_some();
    if let Some(p) = base {
        if let Some(cfi) = p.text.cfi.as_deref() {
            if let Some(a) = doc.reading_anchor(cfi) {
                resume_anchor = Some(a);
            } else {
                can_sync = false;
                notices.push("BookOrbit position could not be located in this text.".into());
            }
        } else if p.text.page.is_some() || p.percentage > 0.0 {
            can_sync = false;
            notices.push("BookOrbit has no exact text location. Using the local position.".into());
        }
    }
    if let Some(mode) = crate::settings::book::BookSettings::load(state.as_ref())?.reading_layout
        && let Some(s) = &mut saved
    {
        s.spread = mode == "spread";
        s.scrolling = mode == "scroll";
    }
    let mut reader = Reader::new(
        doc,
        Rect::new(0, 0, size.width, size.height),
        saved,
        path,
        resume_anchor,
        &crate::settings::book::BookSettings::effective(state.as_ref())?,
    );
    let mut annotation_sync = live
        .as_ref()
        .and_then(|l| l.annotations.clone())
        .map(|context| crate::annotation_sync::Live::text(context, path, reader.doc.clone()));
    if let Some(store) = &state {
        match store.notes() {
            Ok(notes) => reader.study.notes = notes,
            Err(e) => notices.push(format!("Could not load annotations: {e}")),
        }
    }
    match bindings() {
        Ok(map) => reader.bindings = map,
        Err(e) => notices.push(format!("Could not read key bindings: {e}")),
    }
    reader.dirty |= resume_anchor.is_some();
    let opened = Instant::now();
    let start = reader.pager.anchor();
    let started_finished = reader.finished;
    if let Some(target) = at.annotation {
        let note = target.resolve(state.as_ref())?;
        reader.jump_to(note.anchor());
        reader.dirty = true;
    }
    let mut observed = (start, reader.finished);
    let mut last_save = Instant::now();
    let mut last_drag_scroll = Instant::now();
    let mut save_after_draw = None;
    let mut exit = crate::BookExit::Close;
    let result: io::Result<()> = (|| {
        loop {
            let _dialog_theme = crate::settings::reader_dialogs(reader.theme);
            if let Some(sync) = annotation_sync.as_mut()
                && sync.tick()
                && let Some(store) = &state
            {
                // Background updates must not open the text tools or replace their feedback.
                // Sync status is available explicitly through Space a s.
                reader.study.notes = store.notes()?;
            }

            if let Some(message) = viewer.collect() {
                crate::prompt::message(term, "View cover", &message)?;
            }
            let frame_result =
                reader.draw_latest(term, notices[before..].last().map(String::as_str));
            reader.pager.start_pagination();
            // Publish the new geometry before syncing its settings file to disk.
            if let Some(action) = save_after_draw.take() {
                reader.save_appearance(action, state.as_ref(), notices);
            }
            frame_result?;
            if reader.search.refresh_highlight(&reader.pager, &reader.doc) {
                continue;
            }
            if can_sync && let Some(sync) = live.as_deref_mut() {
                let anchor = reader.saved_position().anchor;
                if observed != (anchor, reader.finished) {
                    observed = (anchor, reader.finished);
                    sync.moved(
                        reader
                            .doc
                            .cfi(anchor)
                            .map(|cfi| botui_bookorbit::progress::Move {
                                target: botui_bookorbit::progress::Target::Text(cfi),
                                percentage: reader.reading_percent(),
                            }),
                    );
                }
                if sync.tick(term)? {
                    if sync.reopen {
                        break;
                    }
                    continue;
                }
            }
            let action = loop {
                let remaining = Duration::from_millis(600).saturating_sub(last_save.elapsed());
                if reader.dirty && remaining.is_zero() {
                    reader.flush(&mut state, notices);
                    last_save = Instant::now();
                }
                let images =
                    viewer.busy() || reader.illustrations.as_ref().is_some_and(|p| p.busy());
                let drag_direction = reader.study.drag_direction(reader.text_rect);
                let timeout = reader
                    .dirty
                    .then_some(remaining)
                    .into_iter()
                    .chain(live.as_ref().filter(|_| can_sync).map(|s| s.timeout()))
                    .chain(annotation_sync.as_ref().map(|s| s.timeout()))
                    .chain(
                        reader
                            .pager
                            .is_paginating()
                            .then_some(Duration::from_millis(55)),
                    )
                    .chain(reader.leader.timeout())
                    .chain(reader.word_flash.timeout())
                    .chain(reader.study.toolbar.timeout())
                    .chain(images.then_some(Duration::from_millis(70)))
                    .chain((drag_direction != 0).then_some(
                        Duration::from_millis(80).saturating_sub(last_drag_scroll.elapsed()),
                    ))
                    .min();
                let Some(e) = crate::settings::read_event_for(timeout)? else {
                    if reader.word_flash.expire() || reader.study.toolbar.expire() {
                        break Action::Resize;
                    }
                    if drag_direction != 0
                        && last_drag_scroll.elapsed() >= Duration::from_millis(80)
                    {
                        reader.pager.scroll(isize::from(drag_direction));
                        reader.dirty = true;
                        last_drag_scroll = Instant::now();
                        break Action::Resize;
                    }
                    if reader.leader.active()
                        || reader.pager.is_paginating()
                        || images
                        || (can_sync && live.is_some())
                        || annotation_sync.is_some()
                    {
                        break Action::Resize;
                    }
                    continue;
                };
                if matches!(e, Event::FocusGained)
                    && let Some(s) = live.as_mut()
                {
                    s.refresh();
                    break Action::Resize;
                }
                if reader.menu.is_none()
                    && reader.nav.is_none()
                    && reader.palette.is_none()
                    && reader.layout_panel.is_none()
                    && !reader.study.dragging()
                    && let Some(result) = reader.splitter.input(&e)
                {
                    if let Err(error) = result {
                        notices.push(error.to_string());
                    }
                    break Action::Resize;
                }
                if let Event::Paste(value) = &e
                    && let Some(nav) = reader.nav.as_mut()
                {
                    nav.paste(value);
                    break Action::Resize;
                }
                if reader.menu.is_some()
                    && matches!(&e, Event::Key(k) if k.code == KeyCode::Char(' '))
                {
                    reader.menu = None;
                }
                if reader.menu.is_none()
                    && reader.nav.is_none()
                    && reader.palette.is_none()
                    && reader.layout_panel.is_none()
                    && !reader.study.expects_literal()
                    && let Some(command) = reader.leader.input(&e, reader.leader_context())
                {
                    use crate::leader::Command as L;
                    let action = match command {
                        Some(L::Key(KeyCode::F(10))) => Some(Action::AnnotationSync),
                        Some(L::Key(KeyCode::Char('N'))) => Some(Action::Note),
                        Some(L::OpenTable) => Some(Action::OpenTable),
                        Some(L::Dictionary) => Some(Action::Study(study::Command::Lookup)),
                        Some(L::Translate) => Some(Action::Study(study::Command::Translate)),
                        Some(L::Key(KeyCode::F(7))) => Some(Action::Study(study::Command::Normal)),
                        Some(L::Key(KeyCode::F(8))) => {
                            reader.study.show_notes = !reader.study.show_notes;
                            Some(Action::Resize)
                        }
                        Some(L::Key(KeyCode::Char('H'))) => {
                            Some(Action::Study(study::Command::Highlight))
                        }
                        Some(L::Key(KeyCode::Char('Y'))) => {
                            Some(Action::Study(study::Command::Copy))
                        }
                        Some(c) => c.event().and_then(|e| reader.input_with_bindings(e, false)),
                        None => None,
                    };
                    break action.unwrap_or(Action::Resize);
                }
                use crate::keymap::Scope;
                let e = if reader.nav.as_ref().is_some_and(|nav| nav.editing()) {
                    e
                } else if reader.menu.is_some()
                    || reader.nav.is_some()
                    || reader.palette.is_some()
                    || reader.layout_panel.is_some()
                {
                    Scope::Dialogs.event(e)
                } else if reader.study.normal {
                    if reader.study.expects_literal() {
                        e
                    } else {
                        Scope::Vim.event(e)
                    }
                } else if matches!(&e, Event::Key(k) if k.modifiers.is_empty() && matches!(k.code, KeyCode::Char(c) if reader.bindings.contains_key(&c)))
                {
                    // A pre-existing [keys] action wins for its physical key.
                    e
                } else {
                    Scope::Reading.event(e)
                };
                if reader.menu.is_none()
                    && reader.nav.is_none()
                    && reader.palette.is_none()
                    && reader.layout_panel.is_none()
                    && !reader.study.normal
                    && !crate::leader::reading_event(&e)
                    && !matches!(e, Event::Key(k) if k.modifiers.is_empty() && matches!(k.code, KeyCode::Char(c) if reader.bindings.contains_key(&c)))
                {
                    continue;
                }
                if let Some(action) = reader.study_input(&e) {
                    break action;
                }
                let back =
                    matches!(&e, Event::Key(k) if matches!(k.code, KeyCode::Char('q' | 'Q')));
                if let Some(action) = reader.input(e) {
                    if back && action == Action::Close {
                        exit = crate::BookExit::Back;
                    }
                    break action;
                }
            };
            if action == Action::AnnotationSync {
                if let Some(sync) = annotation_sync.as_mut() {
                    sync.review(term)?;
                } else {
                    crate::prompt::message(
                        term,
                        "BookOrbit annotations",
                        "Open this edition from a connected BookOrbit shelf to synchronize its notes.",
                    )?;
                }
                continue;
            }
            let supplement = match action {
                Action::Sidebar(edit) => reader.sidebar.supplement(edit),
                Action::NavEdit(edit) => reader.nav.as_ref().and_then(|nav| nav.supplement(edit)),
                _ => None,
            };
            if let Some(entry) = supplement.cloned() {
                let doc = reader.doc.clone();
                match resources::open_target(term, path, &doc, &entry.title, &entry.target, |f| {
                    reader.draw_resource_background(f)
                }) {
                    Ok(Some(at)) => {
                        reader.nav = None;
                        reader.sidebar_focus = false;
                        reader.jump_to(at);
                        reader.dirty = true;
                    }
                    Ok(None) => (),
                    Err(e) => {
                        notices.push(e.to_string());
                    }
                }
                continue;
            }
            if matches!(
                action,
                Action::Note | Action::Export | Action::Navigate(Kind::Notes) | Action::Study(_)
            ) {
                reader.menu = None;
                let action_result = (|| -> io::Result<()> {
                    let command = match action {
                        Action::Note => study::Command::Comment,
                        Action::Export => study::Command::Export,
                        Action::Study(command) => command,
                        _ => study::Command::List,
                    };
                    annotations::run(term, &mut reader, state.as_ref(), command)?;
                    Ok(())
                })();
                if let Err(e) = action_result {
                    notices.push(e.to_string());
                    reader.study.message = Some(e.to_string());
                }
                continue;
            }
            if action == Action::Cover {
                if let Some(kind) = botui_formats::FileKind::from_path(path) {
                    viewer.document(path, kind);
                }
                continue;
            }
            if action == Action::Hyphenation {
                use crate::settings::book::BookSettings;
                let mut prefs = BookSettings::effective(state.as_ref())?;
                prefs.hyphenation = !prefs.hyphenation;
                if let Some(store) = state.as_ref() {
                    store.update_book_preferences(|p: &mut BookSettings| {
                        p.hyphenation = Some(prefs.hyphenation)
                    })?;
                } else {
                    crate::settings::Settings::update(|s| s.hyphenation = prefs.hyphenation)?;
                }
                reader.apply_typography(term, &prefs)?;
                continue;
            }
            if matches!(action, Action::Settings | Action::Help | Action::Alignment) {
                let old = crate::settings::book::BookSettings::effective(state.as_ref())?;
                let kind = if reader.doc.markdown.is_some() {
                    crate::preferences::BookKind::Markdown
                } else if botui_formats::FileKind::from_path(path)
                    == Some(botui_formats::FileKind::Epub)
                {
                    crate::preferences::BookKind::Epub
                } else {
                    crate::preferences::BookKind::Text
                };
                if action == Action::Alignment {
                    reader.menu = None;
                    reader.layout_panel = None;
                    layout::choose_alignment(term, &mut reader, state.as_ref())?;
                    continue;
                } else {
                    crate::preferences::show_book(
                        term,
                        if action == Action::Help {
                            crate::preferences::Open::Help
                        } else {
                            crate::preferences::Open::Settings
                        },
                        crate::preferences::Context::Text,
                        state.as_ref().map(|s| (s, kind)),
                    )?;
                }
                let new = crate::settings::book::BookSettings::effective(state.as_ref())?;
                if old.pagination_mode != new.pagination_mode {
                    reader.pager.set_pagination_mode(new.pagination_mode);
                    reader.flash_chapter_position();
                }
                reader.zen = new.zen;
                reader.scroll_lines = new.scroll_lines.unwrap_or(1).clamp(1, 12) as i8;
                reader.zen_progress = new.zen_progress;
                reader.splitter.preferred = new.contents_width;
                if new.reading_layout != old.reading_layout {
                    reader.switch_mode = true;
                    reader.mode = match new.reading_layout.as_deref() {
                        Some("scroll") => LayoutMode::Scroll,
                        Some("single") => LayoutMode::Single,
                        _ => LayoutMode::Spread,
                    };
                    reader.dirty = true;
                }

                reader.opaque = new.opaque_background;
                reader.menu = None;
                if new.theme != old.theme {
                    reader.theme = Theme::from_id(new.theme.as_deref().unwrap_or("terminal"));
                    reader.dirty = true;
                }
                if old.page_width != new.page_width {
                    reader.page_width = layout::width_setting(new.page_width.unwrap_or_default());
                    if new.page_width.is_none() {
                        reader.margin = 0;
                        reader.dirty = true;
                    }
                }
                if let Some(g) = new.column_gap {
                    reader.gap = g.min(32);
                }
                if old.preserve_printed_lines != new.preserve_printed_lines
                    || old.text_alignment != new.text_alignment
                    || old.hyphenation != new.hyphenation
                    || old.heading_rules != new.heading_rules
                    || old.hide_inline_images != new.hide_inline_images
                    || old.paragraph_indent != new.paragraph_indent
                    || old.paragraph_spacing != new.paragraph_spacing
                    || old.compact_spacing() != new.compact_spacing()
                {
                    reader.apply_typography(term, &new)?;
                }
                continue;
            }
            if matches!(
                action,
                Action::Resources | Action::VisibleLinks | Action::Follow(_)
            ) {
                reader.menu = None;
                let visible = reader.links.visible();
                let selected = match action {
                    Action::Follow(i) => Some(i),
                    Action::VisibleLinks => {
                        resources::choose(term, &reader.doc, reader.pager.anchor(), Some(&visible))?
                    }
                    _ => resources::choose(term, &reader.doc, reader.pager.anchor(), None)?,
                };
                if let Some(i) = selected {
                    let reference = reader.doc.references[i].clone();
                    let doc = reader.doc.clone();
                    match resources::open(term, path, &doc, &reference, |f| {
                        reader.draw_resource_background(f)
                    }) {
                        Ok(Some(a)) => {
                            reader.word_flash.clear();
                            if let Some(previous) =
                                reader
                                    .links
                                    .follow(reader.pager.history_position(), &reference, a)
                            {
                                reader.pager.restore_history(&doc, previous);
                            } else {
                                reader.pager.seek(a);
                            }
                        }
                        Ok(None) => (),
                        Err(e) => {
                            notices.push(e.to_string());
                            reader.menu = Some(TableState::default().with_selected(Some(0)));
                        }
                    }
                }
                continue;
            }
            if action == Action::OpenTable {
                reader.menu = None;
                table_view::open(term, &reader)?;
                continue;
            }
            if action == Action::Statistics {
                reader.menu = None;
                if let Err(e) =
                    crate::stats::show(term, state.as_ref(), opened, reader.reading_percent())
                {
                    notices.push(e.to_string());
                }
                continue;
            }
            if action == Action::Close {
                break;
            }
            if action == Action::Bookmarks {
                reader.menu = None;
                reader.manage_marks(term, remote.as_deref_mut(), state.as_ref())?;
                continue;
            }
            if action == Action::Mark {
                let at = reader.pager.anchor();
                let add = !reader.marks.contains(&at);
                let result = if add && let Some(m) = remote.as_deref_mut() {
                    m.toggle(&reader.doc, at).map_err(io::Error::other)
                } else {
                    reader.set_local_bookmark(state.as_ref(), at, add)
                };
                reader.menu = None;
                if let Err(error) = result {
                    crate::prompt::message(term, "Could not update bookmark", &error.to_string())?;
                }
                continue;
            }
            if action == Action::Menu {
                reader.remote_marked = remote.as_ref().is_some_and(|m| {
                    m.entries(&reader.doc)
                        .iter()
                        .any(|(_, a, _)| *a == Some(reader.pager.anchor()))
                });
            }
            if action == Action::Borders
                && let Err(e) = crate::appearance::cycle()
            {
                notices.push(e.to_string());
            }
            reader.apply(action);
            match action {
                Action::Single
                | Action::Spread
                | Action::Scroll
                | Action::Zen
                | Action::ZenProgress
                | Action::Gap(_)
                | Action::Margin(_)
                | Action::FullWidth => save_after_draw = Some(action),
                Action::Background | Action::Pick(theme_picker::Edit::Accept(_)) => {
                    reader.save_appearance(action, state.as_ref(), notices)
                }
                _ => (),
            }
        }
        Ok(())
    })();
    reader.pager.finish_resize();
    if let Some(origin) = reader.links.origin() {
        reader.pager.restore_history(&reader.doc, origin);
    }
    reader.flush(&mut state, notices);
    crate::stats::finish(state.as_ref(), opened, notices);
    if let Some(sync) = annotation_sync.as_mut() {
        sync.finish(notices);
    }
    result?;
    let a = reader.pager.anchor();
    let percent = reader.reading_percent();
    if can_sync && (a != start || reader.finished != started_finished) {
        if let Some(cfi) = reader.doc.cfi(a) {
            return Ok((
                Some(botui_bookorbit::progress::Move {
                    target: botui_bookorbit::progress::Target::Text(cfi),
                    percentage: percent,
                }),
                percent,
                exit,
            ));
        }
        notices.push("Could not map this passage to BookOrbit. Saved locally.".into());
    }
    let shelf_percent = if base.is_none()
        && let Some(saved) = state.as_ref().and_then(|s| s.load().ok()).flatten()
    {
        if saved.finished {
            100.
        } else {
            reader.doc.text_percentage(saved.anchor).min(99.)
        }
    } else {
        percent
    };
    Ok((None, shelf_percent, exit))
}
