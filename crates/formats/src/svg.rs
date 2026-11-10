//! SVG resources stay inside the EPUB. Rasterization belongs to the image worker,
//! not to chapter parsing or pagination.
use crate::{Result, decode_xml, epub, invalid};
use base64::{Engine, engine::general_purpose::STANDARD};
use std::io::{Read, Seek};
use zip::ZipArchive;

pub(super) const INLINE: &str = "botui-inline-svg:";

pub(super) fn locator(base: &str, href: &str) -> Result<String> {
    if let Some(index) = href.strip_prefix(INLINE) {
        let index: usize = index.parse()?;
        Ok(format!("{INLINE}{index}:{base}"))
    } else {
        epub::member_path(base, href)
    }
}

/// Dimensions only: no fonts, paths, decoded pixels or SVG renderer at book open.
pub(super) fn size<'a>(attr: impl Fn(&str) -> Option<&'a str>) -> Option<(usize, usize)> {
    let length = |s: &str| -> Option<f64> {
        let s = s.trim();
        let (s, scale) = [
            ("px", 1.),
            ("pt", 96. / 72.),
            ("in", 96.),
            ("cm", 96. / 2.54),
            ("mm", 96. / 25.4),
        ]
        .into_iter()
        .find_map(|(u, k)| s.strip_suffix(u).map(|s| (s, k)))
        .unwrap_or((s, 1.));
        s.parse::<f64>()
            .ok()
            .map(|v| v * scale)
            .filter(|v| v.is_finite() && *v > 0.)
    };
    let view = attr("viewBox").or_else(|| attr("viewbox")).and_then(|v| {
        let v: Vec<_> = v
            .split(|c: char| c.is_ascii_whitespace() || c == ',')
            .filter(|v| !v.is_empty())
            .filter_map(|v| v.parse::<f64>().ok())
            .collect();
        (v.len() == 4 && v[2] > 0. && v[3] > 0. && v[2].is_finite() && v[3].is_finite())
            .then(|| (v[2], v[3]))
    });
    let w = attr("width").and_then(length);
    let h = attr("height").and_then(length);
    let (w, h) = match (w, h, view) {
        (Some(w), Some(h), _) => (w, h),
        (Some(w), None, Some((vw, vh))) => (w, w * vh / vw),
        (None, Some(h), Some((vw, vh))) => (h * vw / vh, h),
        (None, None, Some(v)) => v,
        _ => return None,
    };
    Some((
        w.round().clamp(1., 65535.) as usize,
        h.round().clamp(1., 65535.) as usize,
    ))
}

pub(super) fn header_size(bytes: &[u8]) -> Option<(usize, usize)> {
    let xml = std::str::from_utf8(bytes).ok()?;
    let mut attrs = std::collections::HashMap::new();
    for token in xmlparser::Tokenizer::from(xml).map_while(std::result::Result::ok) {
        match token {
            xmlparser::Token::Attribute { local, value, .. } => {
                attrs.insert(local.as_str(), value.as_str());
            }
            xmlparser::Token::ElementEnd { .. } => break,
            _ => {}
        }
    }
    size(|key| attrs.get(key).copied())
}

pub(super) fn read<R: Read + Seek>(
    zip: &mut ZipArchive<R>,
    member: &str,
    budget: &mut u64,
) -> Result<Vec<u8>> {
    let (base, inline) = if let Some(rest) = member.strip_prefix(INLINE) {
        let (index, base) = rest
            .split_once(':')
            .ok_or_else(|| invalid("Invalid SVG locator"))?;
        (base, Some(index.parse::<usize>()?))
    } else {
        (member, None)
    };
    let bytes = bytes(zip, base, budget)?;
    let svg = if let Some(index) = inline {
        let xml = decode_xml(&bytes)?;
        let dom = roxmltree::Document::parse_with_options(
            &xml,
            roxmltree::ParsingOptions {
                allow_dtd: true,
                nodes_limit: 100_000,
                ..Default::default()
            },
        )?;
        let node = dom
            .descendants()
            .filter(|n| {
                n.has_tag_name("svg")
                    && n.attribute("hidden").is_none()
                    && n.ancestors().skip(1).any(|a| a.has_tag_name("body"))
                    && !n
                        .ancestors()
                        .skip(1)
                        .any(|a| a.has_tag_name("svg") || a.attribute("hidden").is_some())
            })
            .nth(index)
            .ok_or_else(|| invalid("Inline SVG not found"))?;
        let mut out = String::new();
        serialize(node, zip, base, budget, 0, &mut out)?;
        out.into_bytes()
    } else if base.to_ascii_lowercase().ends_with(".svg")
        || header_size(&bytes).is_some() && bytes.starts_with(b"<svg")
    {
        bundle(&bytes, zip, base, budget, 0)?
    } else {
        bytes
    };
    Ok(svg)
}

fn bytes<R: Read + Seek>(
    zip: &mut ZipArchive<R>,
    member: &str,
    budget: &mut u64,
) -> Result<Vec<u8>> {
    let file = zip.by_name(member)?;
    if file.size() > *budget {
        return Err(invalid("Illustration exceeds size limit").into());
    }
    let mut out = Vec::new();
    file.take(*budget + 1).read_to_end(&mut out)?;
    if out.len() as u64 > *budget {
        return Err(invalid("Illustration exceeds size limit").into());
    }
    *budget -= out.len() as u64;
    Ok(out)
}

fn bundle<R: Read + Seek>(
    data: &[u8],
    zip: &mut ZipArchive<R>,
    base: &str,
    budget: &mut u64,
    depth: usize,
) -> Result<Vec<u8>> {
    if depth > 4 {
        return Err(invalid("SVG nesting exceeds limit").into());
    }
    let xml = decode_xml(data)?;
    let doc = roxmltree::Document::parse_with_options(
        &xml,
        roxmltree::ParsingOptions {
            allow_dtd: true,
            nodes_limit: 100_000,
            ..Default::default()
        },
    )?;
    if !doc.root_element().has_tag_name("svg") {
        return Err(invalid("Not an SVG image").into());
    }
    let mut out = String::new();
    serialize(doc.root_element(), zip, base, budget, depth, &mut out)?;
    Ok(out.into_bytes())
}
fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('"', "&quot;")
}
fn serialize<R: Read + Seek>(
    node: roxmltree::Node<'_, '_>,
    zip: &mut ZipArchive<R>,
    base: &str,
    budget: &mut u64,
    depth: usize,
    out: &mut String,
) -> Result<()> {
    if node.is_text() {
        out.push_str(&escape(node.text().unwrap_or_default()));
        return Ok(());
    }
    if !node.is_element() {
        return Ok(());
    }
    let name = node.tag_name().name();
    if matches!(name, "script" | "foreignObject") {
        return Ok(());
    }
    out.push('<');
    out.push_str(name);
    if name == "svg" {
        out.push_str(
            " xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\"",
        );
    }
    for a in node.attributes() {
        if a.namespace().is_some_and(|ns| {
            !matches!(
                ns,
                "http://www.w3.org/1999/xlink" | "http://www.w3.org/XML/1998/namespace"
            )
        }) {
            continue;
        }
        if a.name().starts_with("on")
            || (a.name() == "href"
                && a.namespace().is_some()
                && node
                    .attributes()
                    .any(|other| other.name() == "href" && other.namespace().is_none()))
        {
            continue;
        }
        let mut value = a.value().to_owned();
        let href = a.name() == "href";
        if name == "image" && href && !value.starts_with("data:") && !value.starts_with('#') {
            value = match epub::member_path(base, &value).ok().and_then(|member| {
                let data = bytes(zip, &member, budget).ok()?;
                let svg = member.to_ascii_lowercase().ends_with(".svg");
                let data = if svg {
                    bundle(&data, zip, &member, budget, depth + 1).ok()?
                } else {
                    data
                };
                Some(format!(
                    "data:{};base64,{}",
                    if svg { "image/svg+xml" } else { "text/plain" },
                    STANDARD.encode(data)
                ))
            }) {
                Some(data) => data,
                None => continue,
            };
        }
        out.push(' ');
        if href {
            out.push_str("xlink:");
        } else if a.namespace() == Some("http://www.w3.org/XML/1998/namespace") {
            out.push_str("xml:");
        }
        out.push_str(a.name());
        out.push_str("=\"");
        out.push_str(&escape(&value));
        out.push('"');
    }
    out.push('>');
    for child in node.children() {
        serialize(child, zip, base, budget, depth, out)?;
    }
    out.push_str("</");
    out.push_str(name);
    out.push('>');
    Ok(())
}
