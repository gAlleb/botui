//! CSS layout for EPUB spine items. Blitz computes boxes in CSS pixels.
//! The font has no outlines: it supplies terminal cell advances to the shaper.
//! Actual glyphs are drawn by the user's terminal.
mod canvas;
mod spacing;
mod table;
use super::{Anchor, Block, PageLine};
use blitz_dom::{DocumentConfig, UserAgentStylesheet, build_single_font_ctx};
use blitz_html::HtmlDocument;
use blitz_traits::{
    net::{Bytes, NetHandler, NetProvider, Request, Url},
    shell::{ColorScheme, Viewport},
};
use std::{
    collections::{BTreeMap, HashMap},
    ops::Range,
    sync::{Arc, LazyLock},
};
pub use table::{DataTable, TableView};

const CELL: f32 = 8.0;
const ROW: f32 = 16.0;

/// Parsed CSS/DOM belongs to an open pager, not to the portable source document.
/// Resizing reuses the engine's incremental layout instead of parsing the book
/// again. Retain at most 512 KiB of markup and 4,000 source tags; larger books
/// still render normally without growing this cache without bound.
pub(super) struct Reflow {
    pool: Option<rayon::ThreadPool>,
    owners: BTreeMap<usize, usize>,
}

impl Reflow {
    pub fn new(doc: &super::Document) -> Self {
        let count = std::thread::available_parallelism()
            .map_or(1, |n| n.get())
            .min(8)
            .min(doc.styled.len());
        let pool = (count > 0)
            .then(|| {
                rayon::ThreadPoolBuilder::new()
                    .num_threads(count)
                    .thread_name(|i| format!("epub-layout-{i}"))
                    .build()
                    .ok()
            })
            .flatten();
        // A chapter keeps its cached DOM on the same worker across viewport changes.
        let mut weights: Vec<_> = doc
            .styled
            .iter()
            .map(|(&start, c)| (start, c.html.len()))
            .collect();
        weights.sort_unstable_by_key(|&(start, size)| (std::cmp::Reverse(size), start));
        let mut loads = vec![0; count.max(1)];
        let mut owners = BTreeMap::new();
        for (start, size) in weights {
            let (owner, load) = loads
                .iter_mut()
                .enumerate()
                .min_by_key(|(_, load)| **load)
                .unwrap();
            owners.insert(start, owner);
            *load += size;
        }
        Self { pool, owners }
    }

    pub fn layout(
        &self,
        doc: &super::Document,
        layout: super::Layout,
        text: &super::BookLayout,
        include: impl Fn(usize) -> bool + Sync,
    ) -> BTreeMap<usize, Vec<PageLine>> {
        let render = |worker: &mut Worker, owner: Option<usize>| {
            doc.styled
                .iter()
                .filter_map(|(&start, chapter)| {
                    if owner.is_some_and(|i| self.owners[&start] != i)
                        || !include(start)
                        || (!chapter.preserve_structure
                            && text.printed[start..chapter.end].contains(&true))
                    {
                        return None;
                    }
                    let at = text
                        .resume
                        .filter(|a| (start..chapter.end).contains(&a.block));
                    Some((
                        start,
                        worker.layout(start, chapter, layout, text.options, &doc.blocks, at),
                    ))
                })
                .collect::<Vec<_>>()
        };
        let Some(pool) = &self.pool else {
            return render(&mut Worker::default(), None).into_iter().collect();
        };
        thread_local! {
            static CACHE: std::cell::RefCell<Worker> = std::cell::RefCell::new(Worker::default());
        }
        pool.broadcast(|context| {
            CACHE.with(|cache| {
                let mut worker = cache.borrow_mut();
                worker.partitions = pool.current_num_threads();
                render(&mut worker, Some(context.index()))
            })
        })
        .into_iter()
        .flatten()
        .collect()
    }
}

struct Worker {
    documents: HashMap<usize, HtmlDocument>,
    markup: usize,
    tags: usize,
    partitions: usize,
}
impl Default for Worker {
    fn default() -> Self {
        Self {
            documents: HashMap::new(),
            markup: 0,
            tags: 0,
            partitions: 1,
        }
    }
}
impl Worker {
    fn layout(
        &mut self,
        start: usize,
        chapter: &Chapter,
        layout: super::Layout,
        options: super::ReflowOptions,
        blocks: &[Block],
        resume: Option<Anchor>,
    ) -> Vec<PageLine> {
        let viewport = Viewport::new(
            u32::from(layout.width) * CELL as u32,
            layout.height.max(1) as u32 * ROW as u32,
            1.0,
            ColorScheme::Light,
        );
        if let Some(doc) = self.documents.get_mut(&start)
            && ((chapter.tables.is_empty()
                && options.typography.alignment == crate::TextAlignment::Book)
                || doc.viewport().window_size == viewport.window_size)
        {
            if doc.viewport().window_size != viewport.window_size {
                doc.set_viewport(viewport);
                spacing::resolve(doc, options);
            }
            return canvas::lines(doc, layout.width, chapter, blocks);
        }
        let mut doc = chapter.document(viewport, options, blocks, resume);
        let lines = canvas::lines(&mut doc, layout.width, chapter, blocks);
        if let Some(cached) = self.documents.get_mut(&start) {
            *cached = doc;
            return lines;
        }
        let tags = chapter.html.bytes().filter(|b| *b == b'<').count();
        if self.markup + chapter.html.len() <= 512 * 1024 / self.partitions
            && self.tags + tags <= 4_000 / self.partitions
        {
            self.markup += chapter.html.len();
            self.tags += tags;
            self.documents.insert(start, doc);
        }
        lines
    }
}

fn marker(el: &blitz_dom::ElementData, name: &str) -> Option<usize> {
    el.attrs()
        .iter()
        .find(|attr| &*attr.name.local == name)?
        .value
        .parse()
        .ok()
}

#[derive(Debug, Default)]
pub struct Stylesheets(HashMap<String, Bytes>);

fn member_url(member: &str) -> Url {
    let mut url = Url::parse("https://epub.invalid/").unwrap();
    url.path_segments_mut()
        .unwrap()
        .clear()
        .extend(member.split('/'));
    url
}
impl Stylesheets {
    pub fn insert(&mut self, member: &str, css: String) {
        self.0.insert(member_url(member).into(), css.into());
    }
}
impl NetProvider for Stylesheets {
    fn fetch(&self, _: usize, req: Request, handler: Box<dyn NetHandler>) {
        // Only CSS members loaded from this archive can be resolved, including @imports.
        // Even a missing stylesheet must complete its request: the layout engine
        // waits for every render-blocking link before laying out the chapter.
        let mut member = req.url.clone();
        member.set_fragment(None);
        member.set_query(None);
        let resolved = req.url.into();
        let Some(bytes) = self.0.get(member.as_str()) else {
            handler.bytes(resolved, Bytes::new());
            return;
        };
        // Parsing an @import calls this provider synchronously. A cycle has no
        // effect on the cascade but expanding a branching cycle is expensive.
        thread_local! {
            static IMPORTS: std::cell::RefCell<Vec<Url>> = const { std::cell::RefCell::new(Vec::new()) };
        }
        IMPORTS.with(|imports| {
            if imports.borrow().contains(&member) {
                handler.bytes(resolved, Bytes::new());
            } else {
                imports.borrow_mut().push(member);
                handler.bytes(resolved, bytes.clone());
                imports.borrow_mut().pop();
            }
        });
    }
}

#[derive(Clone, Debug)]
pub struct SourceText {
    /// Decoded Markdown entity or normalized code text, if unlike the source slice.
    pub display: Option<String>,
    pub start: Anchor,
    pub end: usize,
    pub child: usize,
}
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct Face {
    /// Lightweight code token: keyword, string, comment, number.
    pub syntax: u8,
    /// Markdown presentation: headings 1–6, link 7, quote 8, table head 9, code 10, alerts 12–16, math 17, EPUB column heading 18.
    pub role: u8,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
}
#[derive(Clone, Debug)]
pub struct TextSpan {
    pub bytes: Range<usize>,
    pub source: Option<Anchor>,
    pub source_end: usize,
    pub face: Face,
}

impl TextSpan {
    fn can_merge(&self, other: &Self) -> bool {
        let (Some(a), Some(b)) = (self.source, other.source) else {
            return false;
        };
        self.face == other.face
            && self.bytes.end == other.bytes.start
            && a.block == b.block
            && self.source_end == b.byte
            && self.bytes.len() == self.source_end - a.byte
            && other.bytes.len() == other.source_end - b.byte
    }
}

pub type ClipboardReplacements = std::collections::BTreeMap<usize, Vec<(Range<usize>, String)>>;

#[derive(Clone, Debug)]
pub struct Chapter {
    pub(crate) end: usize,
    /// Raw reference indices are resolved after all spine items have been parsed.
    pub images: Vec<Option<usize>>,
    pub preserve_structure: bool,
    pub tables: Vec<DataTable>,
    /// Display substitutions indexed by canonical source byte range, used only when copying.
    pub clipboard_replacements: ClipboardReplacements,
    html: Arc<str>,
    url: String,
    sheets: Arc<Stylesheets>,
    sources: Vec<Vec<SourceText>>,
}
impl Chapter {
    pub fn new(
        end: usize,
        html: String,
        member: &str,
        sheets: Arc<Stylesheets>,
        sources: Vec<Vec<SourceText>>,
    ) -> Self {
        Self {
            end,
            html: html.into(),
            url: member_url(member).into(),
            sheets,
            sources,
            images: Vec::new(),
            tables: Vec::new(),
            clipboard_replacements: Default::default(),
            preserve_structure: false,
        }
    }
    fn document(
        &self,
        viewport: Viewport,
        options: super::ReflowOptions,
        blocks: &[Block],
        resume: Option<Anchor>,
    ) -> HtmlDocument {
        let mut doc = self.build_document(viewport, options);
        if let Some(at) = resume {
            self.break_at(&mut doc, blocks, at);
        }
        spacing::resolve(&mut doc, options);
        doc
    }

    fn build_document(&self, viewport: Viewport, options: super::ReflowOptions) -> HtmlDocument {
        thread_local! {
            static FONT: parley::FontContext = build_single_font_ctx(include_bytes!("cells.ttf"));
        }
        static SHEET: LazyLock<UserAgentStylesheet> =
            LazyLock::new(|| UserAgentStylesheet::parse(include_str!("reader.css")));
        let figures = if options.inline_images {
            ""
        } else {
            "[data-botui-image] {display:none!important}"
        };
        let mut settings = format!(
            ":root {{ --botui-indent: {}px; }} {figures}",
            u16::from(options.paragraph_indent) * CELL as u16
        );
        if options.typography.alignment == crate::TextAlignment::Left {
            settings.push_str("blockquote{margin-inline-start:0!important} blockquote p,td p,th p,td,th{text-indent:0!important} table[data-botui-data-table] :is(td,th){padding-inline-start:0!important}");
        }
        if options.typography.heading_rules {
            settings.push_str(
                "h1,h2{border-bottom:16px solid!important}h1{border-bottom-style:double!important}",
            );
        }
        if options.typography.alignment != crate::TextAlignment::Book {
            let align = match options.typography.alignment {
                crate::TextAlignment::Left => "left",
                crate::TextAlignment::Right => "right",
                crate::TextAlignment::Center => "center",
                crate::TextAlignment::Justify => "justify",
                crate::TextAlignment::Book => unreachable!(),
            };
            // Place the equation as one flex group. Its grid children retain their
            // own alignment for fraction bars, limits and matrix columns.
            if options.typography.alignment != crate::TextAlignment::Justify {
                let packing = match options.typography.alignment {
                    crate::TextAlignment::Right => "flex-end",
                    crate::TextAlignment::Center => "center",
                    _ => "flex-start",
                };
                settings.push_str(&format!(
                    "body,section,div,p,h1,h2,h3,h4,h5,h6,address,blockquote,figcaption,li,dt,dd,caption,th,td
                     {{text-align:{align}!important;text-align-last:auto!important}}
                     *|math[display='block'],*|math[display='block'] > *|mrow,
                     *|math[display='block'] > *|semantics > *|mrow
                     {{justify-content:{packing}!important}}"
                ));
            }
            if matches!(
                options.typography.alignment,
                crate::TextAlignment::Right | crate::TextAlignment::Center
            ) {
                settings.push_str("p[data-botui-prose-align]{text-indent:0!important}");
            }
            settings.push_str(&format!("p[data-botui-prose-align]{{text-align:{align}!important;text-align-last:auto!important}}"));
        }
        if let Some(rows) = options.paragraph_spacing {
            let gap = u16::from(rows) * ROW as u16;
            settings.push_str(&format!(
                "p[data-botui-prose-spacing] \
                 {{ margin-block: 0 {gap}px !important; }}"
            ));
        }
        let config = DocumentConfig {
            viewport: Some(viewport),
            base_url: Some(self.url.clone()),
            net_provider: Some(self.sheets.clone()),
            font_ctx: Some(FONT.with(Clone::clone)),
            ..Default::default()
        };
        let mut doc = HtmlDocument::from_html(&self.html, config);
        // Blitz enables Stylo's Grid and other feature flags during document
        // construction. Parsing the reusable sheet earlier drops those rules.
        doc.add_user_agent_stylesheet(SHEET.clone());
        doc.add_user_agent_stylesheet(UserAgentStylesheet::parse(&settings));
        if !self.tables.is_empty() {
            table::normalize_scroll_tables(&mut doc);
        }
        doc
    }
}

impl Chapter {
    /// A web location can be in the middle of a wrapped line. Introduce one
    /// layout-only break before that character, leaving the source DOM/CFI intact.
    fn break_at(&self, doc: &mut HtmlDocument, blocks: &[Block], at: Anchor) {
        let Some((owner, span)) = self.sources.iter().enumerate().find_map(|(i, spans)| {
            spans
                .iter()
                .find(|s| s.start.block == at.block && (s.start.byte..s.end).contains(&at.byte))
                .map(|s| (i, s))
        }) else {
            return;
        };
        if at.byte == 0 {
            return;
        }
        let Some(id) = doc
            .query_selector(&format!("[data-botui-text='{owner}']"))
            .ok()
            .flatten()
        else {
            return;
        };
        let Some(el) = doc.get_node(id) else { return };
        if std::iter::successors(Some(el), |n| n.parent.and_then(|p| doc.get_node(p)))
            .filter_map(|n| n.element_data())
            .any(|e| {
                matches!(&*e.name.local, "math" | "pre" | "a")
                    || &*e.name.local == "table"
                        && !blocks
                            .get(at.block)
                            .and_then(|b| b.text.get(..at.byte))
                            .and_then(|prefix| prefix.chars().next_back())
                            .is_some_and(crate::breaks_word)
                    || marker(e, "data-botui-equation").is_some()
            })
        {
            return;
        }
        let Some(id) = el.children.get(span.child).copied() else {
            return;
        };
        let Some(raw) = doc
            .get_node(id)
            .and_then(|n| n.text_data())
            .map(|t| t.content.as_str())
        else {
            return;
        };
        let Some(normal) = blocks
            .get(at.block)
            .and_then(|b| b.text.get(span.start.byte..span.end.min(b.text.len())))
        else {
            return;
        };
        let mut chars = normal.char_indices().peekable();
        let split = raw.char_indices().find_map(|(byte, c)| {
            let &(offset, next) = chars.peek()?;
            if next == c || next == ' ' && c.is_whitespace() {
                chars.next();
                (span.start.byte + offset == at.byte).then_some(byte)
            } else {
                None
            }
        });
        let Some(split) = split else { return };
        let (before, after) = raw.split_at(split);
        let (before, after) = (before.to_owned(), after.to_owned());
        let mut dom = doc.mutate();
        let br = dom.create_element(blitz_dom::qual_name!("br", html), vec![]);
        let suffix = dom.create_text_node(&after);
        dom.set_node_text(id, &before);
        dom.insert_nodes_after(id, &[br, suffix]);
    }
}
