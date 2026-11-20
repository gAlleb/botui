use super::{Anchor, Block, CELL, Chapter, PageLine, ROW, canvas};
use blitz_dom::{BaseDocument, Node, UserAgentStylesheet};
use blitz_traits::node_id::NodeId;
use blitz_traits::shell::{ColorScheme, Viewport};
use std::ops::Range;

/// A scrollable block table (as emitted by Pandoc) needs a single shared
/// column grid in a terminal. Inspect the publisher cascade before Blitz
/// constructs layout boxes, preserving hidden tables and media queries.
pub(super) fn normalize_scroll_tables(doc: &mut BaseDocument) {
    doc.handle_messages();
    doc.resolve_stylist(0.);
    let markers: Vec<_> = doc
        .tree()
        .iter()
        .filter_map(|(id, node)| {
            super::marker(node.element_data()?, "data-botui-data-table")?;
            (node.primary_styles()?.clone_display()
                == style::values::specified::box_::Display::Block)
                .then_some((id, "data-botui-scroll-table"))
        })
        .collect();
    set_markers(doc, &markers, true);
}

/// Preserve publisher-drawn borders on any part of the table.
pub(super) fn needs_rules(doc: &BaseDocument, node: &Node) -> bool {
    use blitz_dom::node::SpecialElementData;
    use style::values::computed::BorderStyle;
    let Some(SpecialElementData::TableRoot(table)) = node.element_data().map(|e| &e.special_data)
    else {
        return false;
    };
    let has_border = |id| {
        doc.get_node(id)
            .and_then(|n| n.primary_styles())
            .is_some_and(|s| {
                let b = s.get_border();
                [
                    b.border_left_style,
                    b.border_right_style,
                    b.border_top_style,
                    b.border_bottom_style,
                ]
                .iter()
                .any(|side| *side != BorderStyle::None)
            })
    };
    !has_border(node.id)
        && !table.cells.iter().any(|c| has_border(c.node_id))
        && !table
            .rows
            .iter()
            .any(|r| has_border(r.node_id) || r.group.is_some_and(has_border))
        && !table
            .columns
            .iter()
            .any(|c| has_border(c.node_id) || c.group.is_some_and(has_border))
}

// The default border spacing already supplies one terminal cell on each side.
pub(super) fn missing_gutters(
    doc: &BaseDocument,
    chapter: &Chapter,
) -> Vec<(NodeId, &'static str)> {
    let mut gutters = Vec::new();
    if chapter.tables.is_empty() {
        return gutters;
    }
    for (id, node) in doc.tree().iter() {
        if node
            .element_data()
            .and_then(|e| super::marker(e, "data-botui-data-table"))
            .is_none()
        {
            continue;
        }
        let Some(style) = node.primary_styles() else {
            continue;
        };
        let gap = if style.clone_border_collapse()
            == style::computed_values::border_collapse::T::Separate
        {
            style.clone_border_spacing().0.width.px()
        } else {
            0.
        };
        let padding = node.final_layout().padding;
        if gap < CELL && needs_rules(doc, node) {
            if gap + padding.left < CELL {
                gutters.push((id, "data-botui-gutter-left"));
            }
            if gap + padding.right < CELL {
                gutters.push((id, "data-botui-gutter-right"));
            }
        }
    }
    gutters
}

pub(super) fn set_markers(
    doc: &mut BaseDocument,
    gutters: &[(NodeId, &'static str)],
    enabled: bool,
) {
    if gutters.is_empty() {
        return;
    }
    let mut dom = doc.mutate();
    for &(id, marker) in gutters {
        let mut name = blitz_dom::qual_name!("class");
        name.local = marker.into();
        if enabled {
            dom.set_attribute(id, name, "");
        } else {
            // The next viewport measures the original publisher box again.
            dom.clear_attribute(id, name);
        }
    }
}

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
        let rows = canvas::lines(&mut doc, width, self, blocks);
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
                let end = range.next_back().unwrap_or(first) + 1;
                let top_rule = first.checked_sub(1).filter(|&i| {
                    rows[i].surfaces.iter().any(|surface| {
                        matches!(surface, crate::Surface::TableEdge { top: true, .. })
                    })
                });
                let separator = rows.get(end).is_some_and(|line| {
                    line.surfaces
                        .iter()
                        .any(|s| matches!(s, crate::Surface::TableHeaderRule { .. }))
                });
                header = top_rule.unwrap_or(first)..end + usize::from(separator);
            }
        }
        Some(TableView {
            rows,
            width,
            header,
        })
    }
}
