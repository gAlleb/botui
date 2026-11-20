//! Apply reader paragraph settings after the book's alignment has been resolved.
use crate::ReflowOptions;
use blitz_dom::{BaseDocument, ElementData, Node, qual_name};
use style::computed_values::{text_align::T as TextAlign, white_space_collapse::T as WhiteSpace};

/// Reading settings follow inherited styles and viewport-dependent media queries.
/// Only the private layout DOM changes, never the source/CFI nodes.
pub(super) fn resolve(doc: &mut BaseDocument, options: ReflowOptions) {
    doc.resolve(0.0);
    let mut changes = Vec::new();
    for (id, node) in doc.tree().iter() {
        let Some(element) = node.element_data() else {
            continue;
        };
        if &*element.name.local != "p" {
            continue;
        }
        let mut mark = |name: &'static str, enabled| {
            if element.attrs().iter().any(|a| &*a.name.local == name) != enabled {
                changes.push((id, name, enabled));
            }
        };
        if options.typography.alignment != crate::TextAlignment::Book {
            let prose =
                !std::iter::successors(Some(node), |n| n.parent.and_then(|p| doc.get_node(p)))
                    .filter_map(Node::element_data)
                    .any(|e| {
                        structured(e, true)
                            || matches!(
                                &*e.name.local,
                                "blockquote" | "figure" | "figcaption" | "header" | "footer"
                            )
                    });
            mark("data-botui-prose-align", prose);
        }
        let centered = node.primary_styles().is_some_and(|s| {
            matches!(
                s.clone_text_align(),
                TextAlign::Center | TextAlign::MozCenter
            )
        });
        mark("data-botui-centered", centered);
        if options.paragraph_spacing.is_some() {
            mark(
                "data-botui-prose-spacing",
                uses_reader_spacing(doc, node, options.compact_spacing),
            );
        }
    }
    if changes.is_empty() {
        return;
    }
    {
        let mut dom = doc.mutate();
        for (id, marker, enabled) in changes {
            let mut name = qual_name!("class");
            name.local = marker.into();
            if enabled {
                dom.set_attribute(id, name, "");
            } else {
                dom.clear_attribute(id, name);
            }
        }
    }
    doc.resolve(0.0);
}

fn uses_reader_spacing(doc: &BaseDocument, node: &Node, compact: bool) -> bool {
    if std::iter::successors(Some(node), |n| n.parent.and_then(|p| doc.get_node(p)))
        .filter_map(Node::element_data)
        .any(|el| structured(el, compact))
    {
        return false;
    }
    // Disabling compact groups restores the ordinary paragraph override. It
    // never disables the independently selected reader gap.
    if !compact {
        return true;
    }
    let Some(style) = node.primary_styles() else {
        return false;
    };
    if !matches!(
        style.clone_text_align(),
        TextAlign::Start | TextAlign::Left | TextAlign::MozLeft | TextAlign::Justify
    ) || style.clone_white_space_collapse() != WhiteSpace::Collapse
    {
        return false;
    }
    // NBSP, BR-only and image-only paragraphs often supply explicit separation
    // between groups. A user prose gap must not expand or collapse those boxes.
    let mut children: Vec<_> = node.children.to_vec();
    while let Some(id) = children.pop() {
        let Some(child) = doc.get_node(id) else {
            continue;
        };
        if child
            .text_data()
            .is_some_and(|t| !t.content.trim().is_empty())
        {
            return true;
        }
        children.extend_from_slice(&child.children);
    }
    false
}

fn structured(element: &ElementData, compact: bool) -> bool {
    matches!(
        &*element.name.local,
        "table" | "li" | "dl" | "figure" | "figcaption" | "pre" | "math"
    ) || (compact && &*element.name.local == "address")
        || element.attrs().iter().any(|a| {
            let role = &*a.name.local == "role" && a.name.ns.is_empty();
            let epub_type = (&*a.name.local == "epub:type" && a.name.ns.is_empty())
                || (&*a.name.local == "type" && &*a.name.ns == "http://www.idpf.org/2007/ops");
            a.value.split_ascii_whitespace().any(|v| {
                (role && matches!(v, "doc-poem" | "doc-verse"))
                    || (epub_type && matches!(v, "z3998:poem" | "z3998:verse" | "poem" | "verse"))
            })
        })
}
