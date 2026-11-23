use super::{Anchor, Block, CELL, Face, PageLine, ROW, SourceText, TextSpan};
use blitz_dom::{BaseDocument, Node, node::TextBrush};
use blitz_traits::node_id::NodeId;
use parley::{Alignment, AlignmentOptions, PositionedLayoutItem};
use std::collections::{BTreeMap, HashMap};
use style::{
    computed_values::visibility::T as Visibility,
    values::{computed::font::FontStyle, specified::text::TextDecorationLine},
};
use unicode_width::UnicodeWidthStr;

#[derive(Clone, Copy, PartialEq, Eq)]
struct Paint {
    owner: Option<usize>,
    face: Face,
    equation: Option<usize>,
    visible: bool,
}
impl Paint {
    fn for_node(doc: &BaseDocument, id: NodeId) -> Self {
        let el = doc.get_node(id).and_then(|n| n.data.downcast_element());
        let owner = el.and_then(|e| super::marker(e, "data-botui-text"));
        let equation = if owner.is_some() {
            None
        } else {
            std::iter::successors(doc.get_node(id), |n| n.parent.and_then(|p| doc.get_node(p)))
                .filter_map(|n| n.element_data())
                .find_map(|e| super::marker(e, "data-botui-equation"))
        };
        let values = doc.get_node(id).and_then(|n| n.primary_styles());
        let face = values.as_ref().map_or(Face::default(), |s| {
            let decoration = s.clone_text_decoration_line();
            Face {
                syntax: el
                    .and_then(|e| super::marker(e, "data-botui-syntax"))
                    .unwrap_or(0) as u8,
                role: std::iter::successors(doc.get_node(id), |n| {
                    n.parent.and_then(|p| doc.get_node(p))
                })
                .filter_map(|n| n.element_data())
                .find_map(|e| match &*e.name.local {
                    "math" => Some(17),
                    "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                        Some(usize::from(e.name.local.as_bytes()[1] - b'0'))
                    }
                    _ => super::marker(e, "data-botui-role"),
                })
                .unwrap_or(0) as u8,
                bold: s.clone_font_weight().value() >= 600.,
                italic: s.clone_font_style() != FontStyle::NORMAL,
                underline: decoration.contains(TextDecorationLine::UNDERLINE),
                strike: decoration.contains(TextDecorationLine::LINE_THROUGH),
            }
        });
        Self {
            owner,
            equation,
            visible: values
                .as_ref()
                .is_none_or(|s| s.clone_visibility() == Visibility::Visible),
            face,
        }
    }
}

#[derive(Clone, Copy, Default)]
struct Cursor {
    part: usize,
    offset: usize,
}
impl Cursor {
    fn take(
        &mut self,
        spans: &[SourceText],
        blocks: &[Block],
        glyph: &str,
    ) -> Option<(Anchor, usize)> {
        while let Some(span) = spans.get(self.part) {
            if let Some(display) = &span.display {
                let rest = display.get(self.offset..).unwrap_or("");
                if rest.is_empty() {
                    self.part += 1;
                    self.offset = 0;
                    continue;
                }
                if rest.starts_with(glyph) {
                    self.offset += glyph.len();
                    return Some((span.start, span.end));
                }
                if let Some(c) = rest.chars().next().filter(|c| c.is_whitespace()) {
                    self.offset += c.len_utf8();
                    continue;
                }
                return None;
            }
            let text = &blocks.get(span.start.block)?.text;
            let begin = span.start.byte + self.offset;
            let end = span.end.min(text.len());
            let rest = text.get(begin..end).unwrap_or("");
            if rest.is_empty() {
                self.part += 1;
                self.offset = 0;
                continue;
            }
            let consumed = if rest.starts_with(glyph) {
                glyph.len()
            } else {
                let c = rest.chars().next()?;
                if c.is_whitespace() && !glyph.trim().is_empty() {
                    self.offset += c.len_utf8();
                    continue;
                }
                if glyph.trim().is_empty()
                    || c.to_lowercase()
                        .ne(glyph.chars().flat_map(char::to_lowercase))
                {
                    return None;
                }
                c.len_utf8()
            };
            self.offset += consumed;
            return Some((
                Anchor {
                    block: span.start.block,
                    byte: begin,
                },
                begin + consumed,
            ));
        }
        None
    }
}

struct Ink<'a> {
    x: usize,
    text: &'a str,
    source: Option<(Anchor, usize)>,
    face: Face,
    rule: bool,
}

pub(super) fn lines(
    doc: &mut BaseDocument,
    width: u16,
    chapter: &super::Chapter,
    blocks: &[Block],
) -> Vec<PageLine> {
    use style::values::computed::text::TextAlignLast;
    // Parley has already broken the lines. On a cell grid, stretching those
    // lines to fill the column leaves whole blank cells between words.
    let gutters = super::table::missing_gutters(doc, chapter);
    if !gutters.is_empty() {
        super::table::set_markers(doc, &gutters, true);
        doc.resolve(0.);
    }
    let justified: Vec<_> = doc
        .tree()
        .iter()
        .filter_map(|(id, node)| {
            let layout = &node.element_data()?.inline_layout_data.as_ref()?.layout;
            if node
                .element_data()?
                .attrs()
                .iter()
                .any(|a| &*a.name.local == "data-botui-prose-align")
            {
                return None;
            }
            let align = layout.alignment()?;
            let last = node.primary_styles()?.clone_text_align_last();
            if last != TextAlignLast::Justify && (align != Alignment::Justify || layout.len() <= 1)
            {
                return None;
            }
            let last = match last {
                TextAlignLast::Auto => None,
                TextAlignLast::Start => Some(Alignment::Start),
                TextAlignLast::End => Some(Alignment::End),
                TextAlignLast::Left => Some(Alignment::Left),
                TextAlignLast::Right => Some(Alignment::Right),
                TextAlignLast::Center => Some(Alignment::Center),
                TextAlignLast::Justify => Some(Alignment::Justify),
            };
            Some((id, align, last))
        })
        .collect();
    let natural = |align| {
        if align == Alignment::Justify {
            Alignment::Start
        } else {
            align
        }
    };
    let mut original_boxes = Vec::new();
    let mut boxes = Vec::new();
    for &(id, align, last) in &justified {
        let layout = inline_layout(doc, id);
        boxes.clear();
        if layout.inline_boxes().len() > 0 {
            boxes.extend(inline_box_positions(layout));
        }
        layout.align(
            natural(align),
            AlignmentOptions {
                last_line_alignment: last.map(natural),
                ..Default::default()
            },
        );
        if boxes.is_empty() {
            continue;
        }
        for ((id, x), (new_id, new_x)) in boxes.iter_mut().zip(inline_box_positions(layout)) {
            debug_assert_eq!(*id, new_id);
            *x = new_x - *x;
        }
        for &(child, shift) in &boxes {
            if shift == 0. {
                continue;
            }
            // Blitz rounds both absolute box edges to pixels. Adding a
            // fractional shift to the rounded origin can widen a 5-cell image to 6.
            let Some(rect) = doc.get_client_bounding_rect(child) else {
                continue;
            };
            let new_left = (rect.x + f64::from(shift)).round();
            let new_right = (rect.x + f64::from(shift) + rect.width).round();
            let area = doc.get_node_mut(child).unwrap().final_layout_mut();
            original_boxes.push((child, area.location.x, area.size.width));
            area.location.x += (new_left - rect.x.round()) as f32;
            area.size.width = (new_right - new_left) as f32;
        }
    }
    let rows = paint_lines::<true>(
        doc,
        width,
        &chapter.sources,
        blocks,
        &chapter.images,
        &chapter.tables,
    );
    // Blitz reuses the document on resize. Keep its cached geometry intact.
    for (child, x, width) in original_boxes {
        let area = doc.get_node_mut(child).unwrap().final_layout_mut();
        area.location.x = x;
        area.size.width = width;
    }
    for (id, align, last_line_alignment) in justified {
        inline_layout(doc, id).align(
            align,
            AlignmentOptions {
                last_line_alignment,
                ..Default::default()
            },
        );
    }
    super::table::set_markers(doc, &gutters, false);
    rows
}

fn inline_layout(doc: &mut BaseDocument, id: NodeId) -> &mut parley::Layout<TextBrush> {
    &mut doc
        .get_node_mut(id)
        .unwrap()
        .element_data_mut()
        .unwrap()
        .inline_layout_data
        .as_mut()
        .unwrap()
        .layout
}

fn inline_box_positions(
    layout: &parley::Layout<TextBrush>,
) -> impl Iterator<Item = (NodeId, f32)> + '_ {
    layout
        .lines()
        .flat_map(|line| line.items())
        .filter_map(|item| match item {
            PositionedLayoutItem::InlineBox(b) if b.kind == parley::InlineBoxKind::InFlow => {
                Some((NodeId::from_u64(b.id), b.x))
            }
            _ => None,
        })
}

fn paint_lines<const GROUPED: bool>(
    doc: &BaseDocument,
    width: u16,
    sources: &[Vec<SourceText>],
    blocks: &[Block],
    images: &[Option<usize>],
    tables: &[super::DataTable],
) -> Vec<PageLine> {
    let mut figures = BTreeMap::<usize, Vec<crate::Figure>>::new();
    let mut surfaces = BTreeMap::<usize, Vec<crate::Surface>>::new();
    let mut rows = Rows::new();
    let mut cursors: Vec<Cursor> = (0..sources.len()).map(|_| Cursor::default()).collect();
    let mut styles = HashMap::new();
    let mut data_tables = Vec::new();
    let mut equations = Vec::new();
    let mut stack = vec![(doc.root_element().id, 0.)];
    while let Some((id, mut shift_x)) = stack.pop() {
        let Some(node) = doc.get_node(id) else {
            continue;
        };
        if let Some(children) = node.layout_children.borrow().as_ref() {
            if node.element_data().is_some_and(|e| {
                &*e.name.local == "math"
                    && e.attrs()
                        .iter()
                        .any(|a| &*a.name.local == "display" && a.value == "block")
            }) {
                let top = node.absolute_position(0., 0.).y;
                let bottom = top + node.final_layout().size.height;
                let band = (top / ROW).round() as usize..(bottom / ROW).round() as usize;
                if band.len() > 1 {
                    equations.push(band);
                }
                // Quantize the origin of the equation once. Otherwise moving a
                // centered flex group half a cell rounds its operands independently.
                let mut first = children.first().and_then(|&id| doc.get_node(id));
                while let Some(wrapper) = first.filter(|n| {
                    n.element_data()
                        .is_some_and(|e| matches!(&*e.name.local, "mrow" | "semantics"))
                }) {
                    first = wrapper
                        .layout_children
                        .borrow()
                        .as_ref()
                        .and_then(|c| c.first())
                        .and_then(|&id| doc.get_node(id));
                }
                if let Some(first) = first {
                    let x = first.absolute_position(0., 0.).x;
                    shift_x = (x / CELL).round() * CELL - x;
                }
            }
            stack.extend(children.iter().rev().map(|&child| (child, shift_x)));
        }
        let area = node.final_layout();
        let pos = node.absolute_position(0., 0.);
        let visible = || {
            node.primary_styles()
                .is_none_or(|s| s.clone_visibility() == Visibility::Visible)
        };
        if node.element_data().is_some_and(|e| {
            &*e.name.local == "table" && super::marker(e, "data-botui-data-table").is_some()
        }) && visible()
            && super::table::needs_rules(doc, node)
        {
            data_tables.push(node);
        }
        if let Some(reference) = node
            .element_data()
            .and_then(|e| super::marker(e, "data-botui-image"))
            .and_then(|i| images.get(i))
            .copied()
            .flatten()
            && area.size.width > 0.
            && area.size.height > 0.
            && visible()
        {
            let x = (pos.x / CELL).round().max(0.) as u16;
            let right = (((pos.x + area.size.width) / CELL).ceil() as u16).min(width);
            if x < right {
                let y = (pos.y / ROW).round().max(0.) as usize;
                let height = ((area.size.height / ROW).ceil() as u16).clamp(1, 2048);
                for row in 0..height {
                    let y = y.saturating_add(usize::from(row));
                    rows.entry(y).or_default();
                    figures.entry(y).or_default().push(crate::Figure {
                        reference,
                        x,
                        width: right - x,
                        row,
                        height,
                    });
                }
            }
            // The terminal image painter owns the contents of this box.
            continue;
        }
        if let Some(role) = node
            .element_data()
            .and_then(|e| super::marker(e, "data-botui-surface"))
            && visible()
        {
            let x = (pos.x / CELL).round().max(0.) as u16;
            let right = (((pos.x + area.size.width) / CELL).round().max(0.) as u16).min(width);
            let top = (pos.y / ROW).round().max(0.) as usize;
            let bottom = ((pos.y + area.size.height) / ROW).round().max(0.) as usize;
            if right > x {
                for y in top..bottom.min(top.saturating_add(8192)) {
                    rows.entry(y).or_default();
                    surfaces
                        .entry(y)
                        .or_default()
                        .push(crate::Surface::Background {
                            x,
                            width: right - x,
                            role: role as u8,
                        });
                }
            }
        }
        if (area.border.top > 0.
            || area.border.bottom > 0.
            || area.border.left > 0.
            || area.border.right > 0.)
            && visible()
        {
            draw_border(
                node,
                shift_x,
                width,
                &mut rows,
                Paint::for_node(doc, id).face,
            );
        }
        draw_text::<GROUPED>(
            doc,
            (node, shift_x),
            &mut rows,
            sources,
            blocks,
            &mut cursors,
            &mut styles,
        );
    }
    let mut edges = TableEdges::default();
    if !data_tables.is_empty() {
        let mut previous = None;
        let positions: Vec<_> = rows
            .keys()
            .flat_map(|&y| {
                let blank = blank_rows(previous, y);
                previous = Some(y);
                blank.chain(std::iter::once(y))
            })
            .collect();
        for table in data_tables {
            if let Some((range, x, end)) =
                draw_table_dividers(doc, table, width, &positions, &rows, &mut surfaces)
            {
                let heading = table
                    .element_data()
                    .and_then(|e| super::marker(e, "data-botui-data-table"))
                    .and_then(|id| tables.get(id))
                    .and_then(|t| t.heading.as_ref());
                if let Some(heading) = heading {
                    let mut last_header = None;
                    for (&y, glyphs) in rows.range_mut(range.clone()) {
                        let mut header = false;
                        for ink in glyphs
                            .iter_mut()
                            .filter(|g| (usize::from(x)..usize::from(end)).contains(&g.x))
                        {
                            if ink.source.is_some_and(|(at, _)| heading.contains(&at)) {
                                header = true;
                                if ink.face.role == 0 || ink.face.role == 9 {
                                    ink.face.role = 18;
                                }
                            }
                        }
                        if header {
                            last_header = Some(y);
                        }
                    }
                    if let Some(y) = last_header
                        && rows.range(y + 1..range.end).next().is_some()
                    {
                        edges
                            .header
                            .insert(y, crate::Surface::TableHeaderRule { x, width: end - x });
                    }
                }
                let mut occupied = rows.range(range);
                if let Some((&first, _)) = occupied.next() {
                    let last = occupied.next_back().map_or(first, |(&row, _)| row);
                    // Keep the side rules on every emitted row inside the frame,
                    // including wrapped cells and the publisher's blank rows.
                    let start = positions.partition_point(|y| *y < first);
                    let end_row = positions.partition_point(|y| *y <= last);
                    for &y in &positions[start..end_row] {
                        surfaces.entry(y).or_default().extend([
                            crate::Surface::TableDivider { x },
                            crate::Surface::TableDivider { x: end - 1 },
                        ]);
                    }
                    edges
                        .above
                        .entry(first)
                        .or_default()
                        .push(crate::Surface::TableEdge {
                            x,
                            width: end - x,
                            top: true,
                        });
                    edges
                        .below
                        .entry(last)
                        .or_default()
                        .push(crate::Surface::TableEdge {
                            x,
                            width: end - x,
                            top: false,
                        });
                }
            }
        }
    }
    let anchor = sources
        .iter()
        .flatten()
        .next()
        .map(|s| s.start)
        .unwrap_or_default();
    compose_rows(rows, figures, surfaces, edges, equations, width, anchor)
}

type Rows<'a> = BTreeMap<usize, Vec<Ink<'a>>>;

#[derive(Default)]
struct TableEdges {
    header: BTreeMap<usize, crate::Surface>,
    above: BTreeMap<usize, Vec<crate::Surface>>,
    below: BTreeMap<usize, Vec<crate::Surface>>,
}

/// Use spare terminal cells between the actual grid tracks. A merged cell
/// contributes just its outer edge; a short continuation row needs no rule below it.
fn draw_table_dividers(
    doc: &BaseDocument,
    node: &Node,
    width: u16,
    positions: &[usize],
    painted: &Rows,
    surfaces: &mut BTreeMap<usize, Vec<crate::Surface>>,
) -> Option<(std::ops::Range<usize>, u16, u16)> {
    use blitz_dom::node::SpecialElementData;
    let Some(SpecialElementData::TableRoot(table)) = node.element_data().map(|e| &e.special_data)
    else {
        return None;
    };
    let grid = table.computed_grid_info.borrow();
    let grid = grid.as_ref()?;
    if grid.columns.positions.len() < 2 || grid.rows.positions.is_empty() {
        return None;
    }
    let pos = node.absolute_position(0., 0.);
    let y = |offset: f32| ((pos.y + offset) / ROW).round() as usize;
    let tracks = &grid.rows.positions;
    let track_range = y(tracks[0].start)..y(tracks.last()?.end);
    let mut content = painted.range(track_range);
    let (&top, _) = content.next()?;
    let bottom = content.next_back().map_or(top, |(&row, _)| row);
    let first = positions.partition_point(|&row| row < top);
    let last = positions.partition_point(|&row| row <= bottom);
    let rows = &positions[first..last];
    let merged: Vec<_> = grid
        .items
        .iter()
        .filter(|item| item.column_end - item.column_start > 1)
        .collect();
    // Ragged EPUB tables sometimes end with a stray empty td in a single
    // row. An unlabeled, entirely empty trailing track needs no divider.
    let mut last_column = 0;
    for cell in &table.cells {
        let Some(node) = doc.get_node(cell.node_id) else {
            continue;
        };
        let span = node
            .attr(blitz_dom::local_name!("colspan"))
            .and_then(|value| value.parse::<u16>().ok())
            .unwrap_or(1);
        let end_column = cell.column.saturating_add(span.saturating_sub(1));
        if end_column > last_column
            && node
                .children
                .iter()
                .filter_map(|id| doc.get_node(*id))
                .any(|child| {
                    child.element_data().is_some()
                        || child
                            .text_data()
                            .is_some_and(|t| !t.content.trim().is_empty())
                })
        {
            last_column = end_column;
        }
    }
    for (col, pair) in grid
        .columns
        .positions
        .windows(2)
        .take(usize::from(last_column))
        .enumerate()
    {
        let (a, b) = (&pair[0], &pair[1]);
        let (left, right) = if a.start <= b.start {
            (a.end, b.start)
        } else {
            (b.end, a.start)
        };
        let start = ((pos.x + left) / CELL).round() as usize;
        let end = ((pos.x + right) / CELL).round() as usize;
        if start >= end {
            continue;
        }
        let x = start + (end - start - 1) / 2;
        if x >= usize::from(width) {
            continue;
        }
        // Grid items use one-based line numbers; the shared line of columns
        // zero and one is line two. Only colspan cells covering it interrupt it.
        let boundary = col + 2;
        let mut interruptions: Vec<_> = merged
            .iter()
            .filter(|item| {
                usize::from(item.column_start) < boundary && boundary < usize::from(item.column_end)
            })
            .map(|item| {
                (
                    y(tracks[usize::from(item.row_start - 1)].start),
                    y(tracks[usize::from(item.row_end - 2)].end),
                )
            })
            .collect();
        interruptions.sort_unstable();
        let mut interruptions = interruptions.into_iter().peekable();
        let mut hidden_until = 0;
        for &row in rows {
            while let Some((_, end)) = interruptions.next_if(|&(start, _)| start <= row) {
                hidden_until = hidden_until.max(end);
            }
            if row >= hidden_until {
                surfaces
                    .entry(row)
                    .or_default()
                    .push(crate::Surface::TableDivider { x: x as u16 });
            }
        }
    }
    let columns = grid
        .columns
        .positions
        .iter()
        .take(usize::from(last_column) + 1);
    let x = |offset: f32| ((pos.x + offset) / CELL).round() as u16;
    // The first and last column contain source text all the way to their
    // track edges. Use the adjacent cell reserved by the table's padding.
    let left = columns.clone().map(|c| x(c.start)).min()?.saturating_sub(1);
    let right = columns.map(|c| x(c.end)).max()?.saturating_add(1);
    (right > left).then(|| {
        (
            y(tracks[0].start)..y(tracks.last().unwrap().end),
            left,
            right,
        )
    })
}

fn draw_border(node: &Node, shift_x: f32, width: u16, rows: &mut Rows<'_>, face: Face) {
    let area = node.final_layout();
    let pos = node.absolute_position(shift_x, 0.);
    let left = (pos.x / CELL).round().max(0.) as usize;
    let right = ((pos.x + area.size.width) / CELL).round().max(0.) as usize;
    let top = (pos.y / ROW).round().max(0.) as usize;
    let bottom = ((pos.y + area.size.height) / ROW).round().max(0.) as usize;
    if right > left && bottom > top {
        let mut rule = |x, y, glyph| {
            rows.entry(y).or_default().push(Ink {
                x,
                text: glyph,
                source: None,
                face,
                rule: true,
            })
        };
        let double = node.primary_styles().is_some_and(|s| {
            s.get_border().border_bottom_style == style::values::computed::BorderStyle::Double
        });
        for x in left..right.min(usize::from(width)) {
            if area.border.top > 0. {
                rule(x, top, "─");
            }
            if area.border.bottom > 0. {
                rule(x, bottom - 1, if double { "═" } else { "─" });
            }
        }
        for y in top..bottom.min(top.saturating_add(2048)) {
            if area.border.left > 0. {
                rule(left, y, "│");
            }
            if area.border.right > 0. && right <= usize::from(width) {
                rule(right - 1, y, "│");
            }
        }
    }
}

fn draw_text<'a, const GROUPED: bool>(
    doc: &'a BaseDocument,
    (node, shift_x): (&'a Node, f32),
    rows: &mut Rows<'a>,
    sources: &[Vec<SourceText>],
    blocks: &[Block],
    cursors: &mut [Cursor],
    styles: &mut HashMap<NodeId, Paint>,
) {
    let Some(inline) = node
        .data
        .downcast_element()
        .and_then(|e| e.inline_layout_data.as_ref())
    else {
        return;
    };
    let pos = node.absolute_position(shift_x, 0.);
    let area = node.final_layout();
    let left = pos.x + area.border.left + area.padding.left;
    let top = pos.y + area.border.top + area.padding.top + inline.block_offset;
    let columns = (doc.viewport().window_size.0 / CELL as u32).max(1) as usize;
    let mut glyphs = Vec::new();
    for line in inline.layout.lines() {
        let y = ((top + line.metrics().block_min_coord) / ROW)
            .round()
            .max(0.) as usize;
        // One style with simple one-glyph characters can use the shaper's
        // positioned glyphs directly.
        // Complex scripts, mixed styles and transformed/source-split text keep
        // the cluster path below. The CSS engine still owns every advance.
        let mut items = line.items();
        if GROUPED
            && let Some(parley::PositionedLayoutItem::GlyphRun(run)) = items.next()
            && items.next().is_none()
            && !run.run().is_rtl()
        {
            let text = &inline.text[line.text_range()];
            let simple = |c: char| {
                matches!(c, ' '..='~' | '—' | '–' | '«' | '»' | '…')
                    || matches!(c, '\u{400}'..='\u{52f}')
                        && unicode_width::UnicodeWidthChar::width(c) == Some(1)
            };
            if !text.is_empty()
                && text.chars().all(simple)
                && run.glyphs().count() == text.chars().count()
            {
                let id = run.style().brush.id;
                let paint = *styles.entry(id).or_insert_with(|| Paint::for_node(doc, id));
                if let Some(i) = paint.owner {
                    let mut cursor = cursors[i];
                    if let Some((source, end)) = cursor.take(&sources[i], blocks, text)
                        && end - source.byte == text.len()
                    {
                        cursors[i] = cursor;
                        if paint.visible {
                            let row = rows.entry(y).or_default();
                            let mut x = f64::from(left) + f64::from(run.offset());
                            let mut begin = 0;
                            let mut first = 0;
                            let mut previous = None;
                            for ((byte, _), glyph) in text.char_indices().zip(run.glyphs()) {
                                let at = ((x * 64.).round() / (64. * f64::from(CELL)))
                                    .round()
                                    .max(0.) as usize;
                                x += f64::from(glyph.advance);
                                if let Some(prev) = previous
                                    && (at != prev + 1 || at / columns != first / columns)
                                {
                                    row.push(Ink {
                                        x: first,
                                        text: &text[begin..byte],
                                        source: Some((
                                            Anchor {
                                                byte: source.byte + begin,
                                                ..source
                                            },
                                            source.byte + byte,
                                        )),
                                        face: paint.face,
                                        rule: false,
                                    });
                                    begin = byte;
                                }
                                if begin == byte {
                                    first = at;
                                }
                                previous = Some(at);
                            }
                            row.push(Ink {
                                x: first,
                                text: &text[begin..],
                                source: Some((
                                    Anchor {
                                        byte: source.byte + begin,
                                        ..source
                                    },
                                    end,
                                )),
                                face: paint.face,
                                rule: false,
                            });
                        }
                        continue;
                    }
                }
            }
        }
        for run in line.runs() {
            let clusters = run.visual_clusters();
            // Preserve fractional coordinates when summing glyph advances.
            let mut x = f64::from(left)
                + f64::from(
                    clusters
                        .clone()
                        .next()
                        .and_then(|c| c.visual_offset())
                        .unwrap_or_default(),
                );
            // Map in logical order, including RTL runs; then place in visual order.
            glyphs.clear();
            let mut last_paint = None;
            for c in clusters {
                // Round the shaper's f32 positions to subpixels first so two
                // runs meeting at a half cell choose the same boundary.
                let at = ((x * 64.).round() / (64. * f64::from(CELL)))
                    .round()
                    .max(0.) as usize;
                x += f64::from(c.advance());
                let range = c.text_range();
                let text = &inline.text[range.clone()];
                if text.is_empty() || text.chars().any(char::is_control) {
                    continue;
                }
                let id = c.style().brush.id;
                let paint = match last_paint {
                    Some((last, paint)) if last == id => paint,
                    _ => {
                        let paint = *styles.entry(id).or_insert_with(|| Paint::for_node(doc, id));
                        last_paint = Some((id, paint));
                        paint
                    }
                };
                glyphs.push((at, range, paint, text.width()));
            }
            if run.is_rtl() {
                glyphs.reverse();
            }
            // A word normally consists of many adjacent shaping clusters.
            // Keep it as one paint span rather than allocating/sorting a cell
            // for each character. Split at gaps, styles, source boundaries,
            // RTL changes and terminal-column edges.
            let mut begin = 0;
            while begin < glyphs.len() {
                let (x, range, paint, first_cells) = &glyphs[begin];
                let mut end = begin + 1;
                let mut cells = *first_cells;
                while GROUPED
                    && let Some((next_x, next_range, next_paint, next_cells)) = glyphs.get(end)
                {
                    if paint != next_paint
                        || glyphs[end - 1].1.end != next_range.start
                        || x + cells != *next_x
                        || x % columns + cells + next_cells > columns
                        || inline.text[next_range.clone()].contains('\u{fffc}')
                    {
                        break;
                    }
                    cells += next_cells;
                    end += 1;
                }
                let range = range.start..glyphs[end - 1].1.end;
                let text = &inline.text[range.clone()];
                let whole = paint.owner.and_then(|i| {
                    if !GROUPED || text.contains('\u{fffc}') {
                        return None;
                    }
                    let mut cursor = *cursors.get(i)?;
                    let source = cursor.take(&sources[i], blocks, text)?;
                    if source.1 - source.0.byte != text.len() {
                        return None;
                    }
                    cursors[i] = cursor;
                    Some(source)
                });
                if let Some(source) = whole {
                    if paint.visible {
                        rows.entry(y).or_default().push(Ink {
                            x: *x,
                            text,
                            source: Some(source),
                            face: paint.face,
                            rule: false,
                        });
                    }
                } else {
                    // A CSS run may cross source spans, transform characters,
                    // or contain an equation. Preserve cluster-level anchors.
                    for (x, range, paint, _) in &glyphs[begin..end] {
                        let text = &inline.text[range.clone()];
                        let source = paint
                            .owner
                            .and_then(|i| cursors.get_mut(i)?.take(&sources[i], blocks, text))
                            .or_else(|| {
                                let s = sources.get(paint.equation?)?.first()?;
                                Some((s.start, s.end.min(blocks.get(s.start.block)?.text.len())))
                            });
                        if paint.visible && text != "\u{fffc}" {
                            rows.entry(y).or_default().push(Ink {
                                x: *x,
                                text,
                                source,
                                face: paint.face,
                                rule: false,
                            });
                        }
                    }
                }
                begin = end;
            }
        }
    }
}

/// Share the same bounded CSS whitespace rows between layout and decoration.
fn blank_rows(previous: Option<usize>, y: usize) -> std::ops::Range<usize> {
    let start = previous.map_or(y, |last| last.saturating_add(1));
    start..y.min(start.saturating_add(4))
}

fn compose_rows(
    rows: Rows<'_>,
    mut figures: BTreeMap<usize, Vec<crate::Figure>>,
    mut surfaces: BTreeMap<usize, Vec<crate::Surface>>,
    mut edges: TableEdges,
    mut equations: Vec<std::ops::Range<usize>>,
    width: u16,
    mut anchor: Anchor,
) -> Vec<PageLine> {
    let mut result = Vec::new();
    // Keep coordinates in emitted rows: a wide CSS row can wrap, and
    // table rules and collapsed vertical margins change the row count.
    equations.sort_unstable_by_key(|r| r.start);
    equations.dedup_by(|next, previous| {
        if next.start < previous.end {
            previous.end = previous.end.max(next.end);
            true
        } else {
            false
        }
    });
    let mut bands = equations.into_iter().peekable();
    let mut formula_start = None;
    let mut prev: Option<usize> = None;
    for (y, mut glyphs) in rows {
        while bands.next_if(|r| r.end <= y).is_some() {
            if let Some(start) = formula_start.take() {
                mark_equation(&mut result[start..]);
            }
        }
        for blank in blank_rows(prev, y) {
            result.push(PageLine {
                surfaces: surfaces.remove(&blank).unwrap_or_default(),
                start: anchor,
                ..Default::default()
            });
        }
        if bands.peek().is_some_and(|r| r.contains(&y)) {
            formula_start.get_or_insert(result.len());
        }
        prev = Some(y);
        glyphs.sort_by_key(|g| (g.x, g.rule)); // text paints over a coincident thin rule
        // Justified prose can place its collapsible trailing space just past
        // the column edge. It must not create a whole extra line of book text.
        while glyphs.last().is_some_and(|g| g.text.trim().is_empty()) {
            glyphs.pop();
        }
        if let Some(last) = glyphs.last_mut() {
            let trimmed = last.text.trim_end();
            if let Some((start, end)) = last.source.as_mut()
                && *end - start.byte == last.text.len()
            {
                *end -= last.text.len() - trimmed.len();
            }
            last.text = trimmed;
        }
        let mut line = PageLine {
            figures: figures.remove(&y).unwrap_or_default(),
            surfaces: surfaces.remove(&y).unwrap_or_default(),
            ..Default::default()
        };
        if let Some(mut rules) = edges.above.remove(&y) {
            // Publisher margins can already supply the gap under a caption.
            // Only ordinary empty rows may be collapsed here.
            let plain_gap = result
                .iter()
                .rev()
                .take_while(|line| line.is_spacer() && line.surfaces.is_empty())
                .count();
            result.truncate(result.len() - plain_gap.saturating_sub(1));
            if result.last().is_some_and(|previous| !previous.is_spacer()) {
                result.push(PageLine {
                    start: anchor,
                    ..Default::default()
                });
            }
            rules.extend(
                line.surfaces
                    .iter()
                    .copied()
                    .filter(|s| matches!(s, crate::Surface::TableDivider { .. })),
            );
            result.push(PageLine {
                surfaces: rules,
                start: anchor,
                ..Default::default()
            });
        }
        let lower_edge = edges.below.remove(&y).map(|mut rules| {
            rules.extend(
                line.surfaces
                    .iter()
                    .copied()
                    .filter(|s| matches!(s, crate::Surface::TableDivider { .. })),
            );
            rules
        });
        let mut band = 0;
        let mut col = 0;
        let mut previous_x = None;
        for ink in glyphs {
            let row_width = usize::from(width);
            let next_band = ink.x / row_width;
            let x = ink.x % row_width;
            if next_band != band {
                finish(&mut result, &mut line, &mut anchor);
                col = 0;
                previous_x = None;
                band = next_band;
            }
            if ink.rule && (previous_x == Some(x) || col > x) {
                continue;
            }
            previous_x = Some(x);
            if x > col {
                line.text.extend(std::iter::repeat_n(' ', x - col));
                col = x;
            }
            let cells = ink.text.width();
            if col + cells > row_width {
                if ink.rule || ink.text.trim().is_empty() {
                    continue;
                }
                finish(&mut result, &mut line, &mut anchor);
                col = 0;
            }
            let start = line.text.len();
            line.text.push_str(ink.text);
            col += cells;
            let span = TextSpan {
                bytes: start..line.text.len(),
                source: ink.source.map(|v| v.0),
                source_end: ink.source.map_or(0, |v| v.1),
                face: ink.face,
            };
            if let Some(last) = line.spans.last_mut()
                && last.can_merge(&span)
            {
                last.bytes.end = span.bytes.end;
                last.source_end = span.source_end;
            } else {
                line.spans.push(span);
            }
        }
        let header_rule = edges.header.remove(&y).map(|rule| {
            let dividers: Vec<_> = line
                .surfaces
                .iter()
                .copied()
                .filter(|s| matches!(s, crate::Surface::TableDivider { .. }))
                .collect();
            (rule, dividers)
        });
        finish(&mut result, &mut line, &mut anchor);
        if let Some((rule, dividers)) = header_rule {
            // The rule itself separates the header from the first content row.
            prev = None;
            let mut border = vec![rule];
            border.extend(dividers);
            result.push(PageLine {
                start: anchor,
                surfaces: border,
                ..Default::default()
            });
        }
        if let Some(surfaces) = lower_edge {
            result.push(PageLine {
                surfaces,
                start: anchor,
                ..Default::default()
            });
        }
    }
    if let Some(start) = formula_start {
        mark_equation(&mut result[start..]);
    }
    result
}

fn mark_equation(rows: &mut [PageLine]) {
    if let Ok(height) = u16::try_from(rows.len()) {
        for (index, row) in rows.iter_mut().enumerate() {
            row.equation = (index as u16, height);
        }
    }
}

fn finish(rows: &mut Vec<PageLine>, line: &mut PageLine, anchor: &mut Anchor) {
    if let Some(at) = line
        .spans
        .iter()
        .filter(|s| !line.text[s.bytes.clone()].trim().is_empty())
        .find_map(|s| s.source)
    {
        *anchor = at;
    }
    line.start = *anchor;
    rows.push(std::mem::take(line));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::publisher::{Chapter, Stylesheets};
    use blitz_traits::shell::{ColorScheme, Viewport};
    use std::sync::Arc;

    #[test]
    fn grouped_paint_keeps_cluster_text_styles_and_source_addresses() {
        for text in [
            "— Простая реплика. Several words in a longer paragraph, repeated words words.",
            "  Spaces   and\u{a0}nonbreaking\u{a0}spaces. а\u{301}б 👩‍💻 🌄 漢字.",
            "שלום עולם — English words — مرحبا بالعالم",
            "mixed Case and CAPS, a link and another word.",
            "Кириллица а\u{483} и\u{486} Ёжик, Ѱ, й; punctuation — «слово»…",
        ] {
            let blocks = [Block {
                text: text.into(),
                heading: false,
            }];
            // A run can cross original source spans even when its face is uniform.
            let split = text.char_indices().nth(7).map_or(text.len(), |(i, _)| i);
            let sources = vec![vec![
                SourceText {
                    display: None,
                    start: Anchor::default(),
                    end: split,
                    child: 0,
                },
                SourceText {
                    display: None,
                    start: Anchor {
                        block: 0,
                        byte: split,
                    },
                    end: text.len(),
                    child: 0,
                },
            ]];
            for css in [
                "",
                "text-align:justify",
                "text-transform:uppercase",
                "word-spacing:3px",
                "direction:rtl",
                "visibility:hidden",
            ] {
                let chapter = Chapter::new(
                    1,
                    format!(
                        "<html><body><p style='{css}' data-botui-text='0'>{text}</p></body></html>"
                    ),
                    "chapter.xhtml",
                    Arc::new(Stylesheets::default()),
                    sources.clone(),
                );
                for width in [18, 32, 47, 80] {
                    let doc = chapter.document(
                        Viewport::new(u32::from(width) * CELL as u32, 480, 1., ColorScheme::Light),
                        crate::ReflowOptions {
                            typography: Default::default(),
                            repair_printed: true,
                            inline_images: true,
                            paragraph_indent: 0,
                            compact_spacing: true,
                            paragraph_spacing: None,
                        },
                        &blocks,
                        None,
                    );
                    assert_eq!(
                        format!(
                            "{:?}",
                            paint_lines::<true>(&doc, width, &sources, &blocks, &[], &[])
                        ),
                        format!(
                            "{:?}",
                            paint_lines::<false>(&doc, width, &sources, &blocks, &[], &[])
                        ),
                        "{text:?}, {css}, width {width}",
                    );
                }
            }
        }
    }
}
