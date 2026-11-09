//! CSS layout for EPUB spine items. Blitz computes boxes in CSS pixels.
//! The font has no outlines: it supplies terminal cell advances to the shaper.
//! Actual glyphs are drawn by the user's terminal.
mod canvas;
use super::{Anchor, Block, PageLine};
use blitz_dom::{DocumentConfig, UserAgentStylesheet, build_single_font_ctx};
use blitz_html::HtmlDocument;
use blitz_traits::{
    net::{Bytes, NetHandler, NetProvider, Request, Url},
    shell::{ColorScheme, Viewport},
};
use std::{
    collections::HashMap,
    ops::Range,
    sync::{Arc, LazyLock},
};

const CELL: f32 = 8.0;
const ROW: f32 = 16.0;

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
    pub start: Anchor,
    pub end: usize,
    pub child: usize,
}
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct Face {
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

#[derive(Clone, Debug)]
pub struct Chapter {
    pub(crate) end: usize,
    /// Raw reference indices are resolved after all spine items have been parsed.
    pub images: Vec<Option<usize>>,
    pub preserve_structure: bool,
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
            preserve_structure: false,
        }
    }
    pub(super) fn layout(
        &self,
        width: u16,
        height: usize,
        options: super::ReflowOptions,
        blocks: &[Block],
        resume: Option<Anchor>,
    ) -> Vec<PageLine> {
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
        let settings = format!(
            ":root {{ --botui-indent: {}px; }} {figures}",
            u16::from(options.paragraph_indent) * CELL as u16
        );
        let options = DocumentConfig {
            viewport: Some(Viewport::new(
                u32::from(width) * CELL as u32,
                height.max(1) as u32 * ROW as u32,
                1.0,
                ColorScheme::Light,
            )),
            base_url: Some(self.url.clone()),
            net_provider: Some(self.sheets.clone()),
            font_ctx: Some(FONT.with(Clone::clone)),
            ..Default::default()
        };
        let mut doc = HtmlDocument::from_html(&self.html, options);
        // Blitz enables Stylo's Grid and other feature flags during document
        // construction. Parsing the reusable sheet earlier drops those rules.
        doc.add_user_agent_stylesheet(SHEET.clone());
        doc.add_user_agent_stylesheet(UserAgentStylesheet::parse(&settings));
        if let Some(at) = resume {
            self.break_at(&mut doc, blocks, at);
        }
        doc.resolve(0.0);
        canvas::lines(&doc, width, &self.sources, blocks, &self.images)
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
                matches!(&*e.name.local, "table" | "math" | "pre")
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
