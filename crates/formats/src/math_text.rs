//! Adapt mathematical typography to a fixed terminal cell. Keep token owners
//! in the layout DOM so transformed digits still refer to their EPUB source.
use botui_core::publisher::SourceText;
use scraper::{ElementRef, Html, Node, node::Text};

use crate::publisher::set_attr;

fn text(s: &str) -> Node {
    Node::Text(Text { text: s.into() })
}

fn script(s: &str, upper: bool) -> Option<String> {
    let (plain, raised) = if upper {
        ("0123456789+-−=()in", "⁰¹²³⁴⁵⁶⁷⁸⁹⁺⁻⁻⁼⁽⁾ⁱⁿ")
    } else {
        (
            "0123456789+-−=()aehijklmnoprstuvx",
            "₀₁₂₃₄₅₆₇₈₉₊₋₋₌₍₎ₐₑₕᵢⱼₖₗₘₙₒₚᵣₛₜᵤᵥₓ",
        )
    };
    s.chars()
        .map(|c| raised.chars().nth(plain.chars().position(|p| p == c)?))
        .collect()
}

fn decorate(dom: &mut Html, id: ego_tree::NodeId, left: &str, right: &str) {
    let mut n = dom.tree.get_mut(id).unwrap();
    if !left.is_empty() {
        n.insert_before(text(left));
    }
    if !right.is_empty() {
        n.insert_after(text(right));
    }
}

fn format_script(
    dom: &mut Html,
    id: ego_tree::NodeId,
    upper: bool,
    sources: &mut [Vec<SourceText>],
) {
    let el = ElementRef::wrap(dom.tree.get(id).unwrap()).unwrap();
    if let Some(node) = el.first_child().filter(|n| n.next_sibling().is_none())
        && let Node::Text(t) = node.value()
        && let Some(value) = script(t, upper)
    {
        let child = node.id();
        if let Some(owner) = el
            .attr("data-botui-text")
            .and_then(|s| s.parse::<usize>().ok())
            && let Some(span) = sources.get_mut(owner).and_then(|v| v.first_mut())
        {
            span.display = Some(value.clone());
        }
        *dom.tree.get_mut(child).unwrap().value() = text(&value);
    } else {
        decorate(dom, id, if upper { "^(" } else { "_(" }, ")");
    }
}

// Stretch ordinary paired fences to the table's rows using terminal glyphs.
fn matrix_fences(dom: &mut Html, table: ego_tree::NodeId) {
    let table = ElementRef::wrap(dom.tree.get(table).unwrap()).unwrap();
    let Some(left) = table.prev_siblings().find_map(ElementRef::wrap) else {
        return;
    };
    let Some(right) = table.next_siblings().find_map(ElementRef::wrap) else {
        return;
    };
    if left.value().name() != "mo" || right.value().name() != "mo" {
        return;
    }
    let fences = match (
        left.text().collect::<String>().as_str(),
        right.text().collect::<String>().as_str(),
    ) {
        ("(", ")") => [("⎛", "⎞"), ("⎜", "⎟"), ("⎝", "⎠")],
        ("[", "]") => [("⎡", "⎤"), ("⎢", "⎥"), ("⎣", "⎦")],
        _ => return,
    };
    let cells: Option<Vec<_>> = table
        .child_elements()
        .map(|row| {
            if row.value().name() != "mtr" {
                return None;
            }
            let mut cells = row.child_elements();
            let first = cells.next()?;
            let last = cells.last().unwrap_or(first);
            Some((first.id(), last.id()))
        })
        .collect();
    let Some(cells) = cells.filter(|c| c.len() >= 2) else {
        return;
    };
    let ends = [left.id(), right.id()];
    let last_row = cells.len() - 1;
    for (index, (first, last)) in cells.into_iter().enumerate() {
        let (l, r) = fences[if index == 0 {
            0
        } else if index == last_row {
            2
        } else {
            1
        }];
        dom.tree.get_mut(first).unwrap().prepend(text(l));
        dom.tree.get_mut(last).unwrap().append(text(r));
    }
    for id in ends {
        dom.tree.get_mut(id).unwrap().detach();
    }
}

pub(super) fn prepare(dom: &mut Html, sources: &mut [Vec<SourceText>]) {
    let roots: Vec<_> = dom
        .tree
        .nodes()
        .filter_map(ElementRef::wrap)
        .filter(|e| e.value().name() == "math" || super::tex::marked(e.value()))
        .map(|e| e.id())
        .collect();
    for root in roots {
        let el = ElementRef::wrap(dom.tree.get(root).unwrap()).unwrap();
        let native = el.value().name() == "math";
        let display = el.attr("display") == Some("block");
        let nodes: Vec<_> = el
            .descendants()
            .filter_map(ElementRef::wrap)
            .filter(|e| native || matches!(e.value().name(), "sup" | "sub"))
            .map(|e| e.id())
            .collect();
        for id in nodes {
            let e = ElementRef::wrap(dom.tree.get(id).unwrap()).unwrap();
            let name = e.value().name();
            let args: Vec<_> = e.child_elements().map(|c| c.id()).collect();
            let limits = display
                && matches!(name, "msubsup" | "munderover")
                && e.child_elements()
                    .next()
                    .is_some_and(|base| base.value().name() == "mo");
            if limits {
                if let Node::Element(tag) = dom.tree.get_mut(id).unwrap().value() {
                    tag.name.local = "munderover".into();
                }
                continue;
            }
            if display && name == "mtable" {
                matrix_fences(dom, id);
                continue;
            }
            let mut scripts = Vec::new();
            match (name, args.as_slice()) {
                ("sup" | "sub", _) => scripts.push((id, name == "sup")),
                ("msup" | "msub", [_, arg]) => {
                    let argument = ElementRef::wrap(dom.tree.get(*arg).unwrap()).unwrap();
                    if display
                        && (argument.child_elements().next().is_some()
                            || script(&argument.text().collect::<String>(), name == "msup")
                                .is_none())
                    {
                        continue;
                    }
                    scripts.push((*arg, name == "msup"));
                }
                ("msubsup", [_, low, high]) if !display => {
                    scripts.extend([(*low, false), (*high, true)]);
                }
                ("msqrt", [first, ..]) => {
                    decorate(dom, *first, "√(", "");
                    decorate(dom, *args.last().unwrap(), "", ")");
                }
                ("mfrac", [a, b]) if !display && e.attr("linethickness") != Some("0") => {
                    for (arg, prefix) in [(*a, ""), (*b, "/")] {
                        let complex = ElementRef::wrap(dom.tree.get(arg).unwrap())
                            .unwrap()
                            .child_elements()
                            .next()
                            .is_some();
                        decorate(
                            dom,
                            arg,
                            &format!("{prefix}{}", if complex { "(" } else { "" }),
                            if complex { ")" } else { "" },
                        );
                    }
                }
                ("mo", [])
                    if matches!(
                        e.text().collect::<String>().as_str(),
                        "=" | "+" | "−" | "-" | "±" | "→" | "<" | ">" | "≤" | "≥"
                    ) =>
                {
                    let unary = matches!(e.text().collect::<String>().as_str(), "-" | "−" | "+")
                        && e.prev_siblings().find_map(ElementRef::wrap).is_none();
                    if !unary {
                        if let Node::Element(tag) = dom.tree.get_mut(id).unwrap().value() {
                            set_attr(tag, "data-botui-operator", "");
                        }
                        decorate(dom, id, " ", " ");
                    }
                    continue;
                }
                _ => continue,
            }
            for (arg, upper) in scripts {
                format_script(dom, arg, upper, sources);
            }
            if let Node::Element(tag) = dom.tree.get_mut(id).unwrap().value() {
                set_attr(tag, "data-botui-linear", "");
            }
        }
        // Indentation between MathML elements is XML formatting, unlike
        // whitespace within an mtext token.
        if native {
            let blanks: Vec<_> = dom.tree.get(root).unwrap().descendants()
                .filter(|n| matches!(n.value(), Node::Text(t) if t.trim().is_empty() && t.contains('\n')))
                .filter(|n| n.parent().and_then(ElementRef::wrap).is_some_and(|p| p.value().name() != "mtext"))
                .map(|n| n.id()).collect();
            for id in blanks {
                dom.tree.get_mut(id).unwrap().detach();
            }
        }
    }
}
