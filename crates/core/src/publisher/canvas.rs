use super::{Anchor, Block, CELL, Face, PageLine, ROW, SourceText, TextSpan};
use blitz_dom::{BaseDocument, Node};
use blitz_traits::node_id::NodeId;
use std::collections::{BTreeMap, HashMap};
use unicode_width::UnicodeWidthStr;

#[derive(Clone, Copy)]
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
        let style = |name| doc.resolved_style_value(id, name);
        let decoration = style("text-decoration-line");
        Self {
            owner,
            equation,
            visible: !matches!(style("visibility").as_str(), "hidden" | "collapse"),
            face: Face {
                bold: style("font-weight").parse::<u16>().is_ok_and(|v| v >= 600),
                italic: matches!(style("font-style").as_str(), "italic" | "oblique"),
                underline: decoration.contains("underline"),
                strike: decoration.contains("line-through"),
            },
        }
    }
}

#[derive(Default)]
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
    doc: &BaseDocument,
    width: u16,
    sources: &[Vec<SourceText>],
    blocks: &[Block],
    images: &[Option<usize>],
) -> Vec<PageLine> {
    let mut figures = BTreeMap::<usize, Vec<crate::Figure>>::new();
    let mut rows = Rows::new();
    let mut cursors: Vec<Cursor> = (0..sources.len()).map(|_| Cursor::default()).collect();
    let mut styles = HashMap::new();
    let mut stack = vec![doc.root_element().id];
    while let Some(id) = stack.pop() {
        let Some(node) = doc.get_node(id) else {
            continue;
        };
        if let Some(children) = node.layout_children.borrow().as_ref() {
            stack.extend(children.iter().rev().copied());
        }
        let area = node.final_layout();
        let pos = node.absolute_position(0., 0.);
        let visible = || {
            !matches!(
                doc.resolved_style_value(id, "visibility").as_str(),
                "hidden" | "collapse"
            )
        };
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
                let count = ((area.size.height / ROW).ceil() as usize).clamp(1, 2048);
                for y in y..y.saturating_add(count) {
                    rows.entry(y).or_default();
                    figures.entry(y).or_default().push(crate::Figure {
                        reference,
                        x,
                        width: right - x,
                    });
                }
            }
            // The terminal image painter owns the contents of this box.
            continue;
        }
        if (area.border.top > 0.
            || area.border.bottom > 0.
            || area.border.left > 0.
            || area.border.right > 0.)
            && visible()
        {
            draw_border(node, width, &mut rows);
        }
        draw_text(
            doc,
            node,
            &mut rows,
            sources,
            blocks,
            &mut cursors,
            &mut styles,
        );
    }
    let anchor = sources
        .iter()
        .flatten()
        .next()
        .map(|s| s.start)
        .unwrap_or_default();
    compose_rows(rows, figures, width, anchor)
}

type Rows<'a> = BTreeMap<usize, Vec<Ink<'a>>>;

fn draw_border(node: &Node, width: u16, rows: &mut Rows<'_>) {
    let area = node.final_layout();
    let pos = node.absolute_position(0., 0.);
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
                face: Face::default(),
                rule: true,
            })
        };
        for x in left..right.min(usize::from(width)) {
            if area.border.top > 0. {
                rule(x, top, "─");
            }
            if area.border.bottom > 0. {
                rule(x, bottom - 1, "─");
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

fn draw_text<'a>(
    doc: &'a BaseDocument,
    node: &'a Node,
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
    let pos = node.absolute_position(0., 0.);
    let area = node.final_layout();
    let left = pos.x + area.border.left + area.padding.left;
    let top = pos.y + area.border.top + area.padding.top + inline.block_offset;
    for line in inline.layout.lines() {
        let y = ((top + line.metrics().block_min_coord) / ROW)
            .round()
            .max(0.) as usize;
        for run in line.runs() {
            let clusters = run.visual_clusters();
            // Justification yields fractional spaces. Keep the fraction while
            // adding whole-cell advances, including across powers of two.
            let mut x = f64::from(left)
                + f64::from(
                    clusters
                        .clone()
                        .next()
                        .and_then(|c| c.visual_offset())
                        .unwrap_or_default(),
                );
            // Map in logical order, including RTL runs; then place in visual order.
            let mut glyphs = Vec::new();
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
                let paint = *styles.entry(id).or_insert_with(|| Paint::for_node(doc, id));
                glyphs.push((at, text, paint));
            }
            if run.is_rtl() {
                glyphs.reverse();
            }
            for (x, text, paint) in glyphs {
                let source = paint
                    .owner
                    .and_then(|i| cursors.get_mut(i)?.take(&sources[i], blocks, text))
                    .or_else(|| {
                        let s = sources.get(paint.equation?)?.first()?;
                        Some((s.start, s.end.min(blocks.get(s.start.block)?.text.len())))
                    });
                if paint.visible && text != "\u{fffc}" {
                    rows.entry(y).or_default().push(Ink {
                        x,
                        text,
                        source,
                        face: paint.face,
                        rule: false,
                    });
                }
            }
        }
    }
}

fn compose_rows(
    rows: Rows<'_>,
    mut figures: BTreeMap<usize, Vec<crate::Figure>>,
    width: u16,
    mut anchor: Anchor,
) -> Vec<PageLine> {
    let mut result = Vec::new();
    let mut prev: Option<usize> = None;
    for (y, mut glyphs) in rows {
        if let Some(last) = prev {
            // A chapter may specify an arbitrarily large empty CSS area.
            for _ in 0..y.saturating_sub(last.saturating_add(1)).min(4) {
                result.push(PageLine {
                    start: anchor,
                    ..Default::default()
                });
            }
        }
        prev = Some(y);
        glyphs.sort_by_key(|g| (g.x, g.rule)); // text paints over a coincident thin rule
        // Justified prose can place its collapsible trailing space just past
        // the column edge. It must not create a whole extra line of book text.
        while glyphs.last().is_some_and(|g| g.text.trim().is_empty()) {
            glyphs.pop();
        }
        let mut line = PageLine {
            figures: figures.remove(&y).unwrap_or_default(),
            ..Default::default()
        };
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
        finish(&mut result, &mut line, &mut anchor);
    }
    result
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
