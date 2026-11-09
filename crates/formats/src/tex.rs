//! TeX explicitly marked by EPUB producers such as Pandoc or MathJax.
//! Convert only formula markup. Dollar amounts and code examples stay literal.
use math_core::{LatexToMathML, MathDisplay};
use scraper::{ElementRef, Html, Node};

pub(super) fn script(el: &scraper::node::Element) -> bool {
    el.name() == "script"
        && el.attr("type").is_some_and(|mime| {
            matches!(
                mime.split(';').next().unwrap_or("").trim(),
                "math/tex" | "application/x-tex"
            )
        })
}
pub(super) fn marked(el: &scraper::node::Element) -> bool {
    script(el)
        || matches!(el.name(), "span" | "div")
            && el.has_class("math", scraper::CaseSensitivity::CaseSensitive)
}

pub(super) fn convert(dom: &mut Html) {
    let mut replacements = Vec::new();
    for el in dom.tree.nodes().filter_map(ElementRef::wrap) {
        let tag = el.value();
        let fallback = tag.name() == "annotation"
            && tag
                .attr("encoding")
                .is_some_and(|v| v.eq_ignore_ascii_case("application/x-tex"))
            && el.parent().and_then(ElementRef::wrap).is_some_and(|p| {
                p.value().name() == "semantics"
                    && p.child_elements()
                        .all(|c| matches!(c.value().name(), "annotation" | "annotation-xml"))
            });
        if !(marked(tag) || fallback) || el.child_elements().next().is_some() {
            continue;
        }
        let raw = el.text().collect::<String>();
        let raw = raw.trim();
        if raw.is_empty() {
            continue;
        }
        let block = tag.has_class("display", scraper::CaseSensitivity::CaseSensitive)
            || tag.attr("type").is_some_and(|v| v.contains("mode=display"))
            || raw.starts_with("\\[")
            || raw.starts_with("$$");
        let formula = [("\\(", "\\)"), ("\\[", "\\]"), ("$$", "$$"), ("$", "$")]
            .into_iter()
            .find_map(|(start, end)| raw.strip_prefix(start)?.strip_suffix(end))
            .unwrap_or(raw);
        let display = if block {
            MathDisplay::Block
        } else {
            MathDisplay::Inline
        };
        replacements.push((el.id(), formula.to_owned(), display));
    }
    if replacements.is_empty() {
        return;
    }
    let converter = LatexToMathML::new(Default::default(), Vec::new()).expect("no custom macros");
    for (id, tex, display) in replacements {
        let children: Vec<_> = dom
            .tree
            .get(id)
            .unwrap()
            .children()
            .map(|n| n.id())
            .collect();
        let mut host = dom.tree.get_mut(id).unwrap();
        let Node::Element(el) = host.value() else {
            continue;
        };
        // Make an invalid expression readable as TeX as well.
        el.name.local = "span".into();
        el.name.ns = "http://www.w3.org/1999/xhtml".into();
        let Ok(output) = converter.convert_with_local_state(&tex, display) else {
            continue;
        };
        let fragment = Html::parse_fragment(&output.mathml);
        let Some(math) = fragment
            .root_element()
            .child_elements()
            .find(|e| e.value().name() == "math")
        else {
            continue;
        };
        if let Some(owner) = el.attr("data-botui-text").map(str::to_owned) {
            super::publisher::set_attr(el, "data-botui-equation", &owner);
        }
        for child in children {
            dom.tree.get_mut(child).unwrap().detach();
        }
        let mut stack = vec![(id, *math)];
        while let Some((parent, src)) = stack.pop() {
            let dest = dom
                .tree
                .get_mut(parent)
                .unwrap()
                .append(src.value().clone())
                .id();
            let children: Vec<_> = src.children().collect();
            stack.extend(children.into_iter().rev().map(|c| (dest, c)));
        }
    }
}
