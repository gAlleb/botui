use super::{Anchor, Block, CELL, Chapter, PageLine, ROW, canvas};
use blitz_dom::UserAgentStylesheet;
use blitz_traits::shell::{ColorScheme, Viewport};
use std::ops::Range;

/// Lightweight index into the already annotated spine HTML. The full table is
/// laid out independently only while the user has its viewer open.
#[derive(Clone, Debug)]
pub struct DataTable {
    pub source: Range<Anchor>,
    pub heading: Option<Range<Anchor>>,
    pub title: String,
    pub columns: u16,
}

pub struct TableView {
    pub rows: Vec<PageLine>,
    pub header: Range<usize>,
    pub width: u16,
}

impl Chapter {
    pub fn table_view(
        &self,
        index: usize,
        blocks: &[Block],
        width: u16,
        height: u16,
        options: crate::ReflowOptions,
    ) -> Option<TableView> {
        let table = self.tables.get(index)?;
        // Give squeezed columns room to wrap at words. Bound the virtual
        // canvas for pathological source spans; a wider terminal still fits.
        let width = width.max(table.columns.saturating_mul(24).min(1024)).max(1);
        let viewport = Viewport::new(
            u32::from(width) * CELL as u32,
            u32::from(height.max(1)) * ROW as u32,
            1.0,
            ColorScheme::Light,
        );
        let mut doc = self.build_document(viewport, options);
        let selector = format!("[data-botui-data-table='{index}']");
        let table_node = doc.query_selector(&selector).ok()??;
        let body = doc.query_selector("body").ok()??;
        // Retain the original parents and stylesheets. Hide unrelated siblings
        // without renumbering elements used by CSS selectors like :nth-child.
        let mut siblings = Vec::new();
        let mut child = table_node;
        while child != body {
            let parent = doc.get_node(doc.get_node(child)?.parent?)?;
            siblings.extend(parent.children.iter().copied().filter(|&id| id != child));
            child = parent.id;
        }
        {
            let mut dom = doc.mutate();
            for sibling in siblings {
                if dom
                    .doc
                    .get_node(sibling)
                    .is_some_and(|n| n.element_data().is_some())
                {
                    let mut name = blitz_dom::qual_name!("class");
                    name.local = "data-botui-table-hidden".into();
                    dom.set_attribute(sibling, name, "");
                } else {
                    dom.remove_node(sibling);
                }
            }
        }
        doc.add_user_agent_stylesheet(UserAgentStylesheet::parse(&format!(
            "[data-botui-table-hidden] {{ display: none !important }} \
            body, body *:has({selector}) {{ margin:0 !important; padding:0 !important; width:auto !important; max-width:none !important }} \
            {selector} {{ width:100% !important; max-width:none !important; margin:0 !important }}"
        )));
        super::spacing::resolve(&mut doc, options);
        let rows = canvas::lines(&mut doc, width, &self.sources, blocks, &self.images);
        let mut header = 0..0;
        if let Some(source) = &table.heading {
            let mut range = rows
                .iter()
                .enumerate()
                .filter(|(_, row)| {
                    row.spans
                        .iter()
                        .filter_map(|s| s.source)
                        .any(|a| source.contains(&a))
                })
                .map(|(i, _)| i);
            if let Some(first) = range.next() {
                header = first..range.next_back().unwrap_or(first) + 1;
            }
        }
        Some(TableView {
            rows,
            width,
            header,
        })
    }
}
