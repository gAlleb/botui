// BookOrbit artwork adaptation: see licenses/NOTICE.md and licenses/bookorbit/.
//! Fit lettering with the same shaping engine used for the final raster.
use super::{Design, artwork::Palette};
use resvg::usvg::{self, fontdb};
use std::{
    fmt::Write,
    path::Path,
    process::Command,
    sync::{Arc, OnceLock},
};

pub(super) fn with_fonts(f: impl FnOnce(&Arc<fontdb::Database>)) {
    static FONTS: OnceLock<Arc<fontdb::Database>> = OnceLock::new();
    f(FONTS.get_or_init(|| {
        let mut db = fontdb::Database::new();
        // A fontconfig match avoids eagerly opening thousands of installed families.
        for face in ["sans-serif:weight=bold", "sans-serif:weight=regular"] {
            if let Ok(output) = Command::new("fc-match")
                .args(["-f", "%{file}", face])
                .output()
                && output.status.success()
                && let Ok(path) = std::str::from_utf8(&output.stdout)
                && Path::new(path).is_absolute()
            {
                let _ = db.load_font_file(path);
            }
        }
        if db.is_empty() {
            db.load_system_fonts();
        }
        let family = db
            .faces()
            .find_map(|face| face.families.first().map(|(name, _)| name.clone()));
        if let Some(name) = family {
            db.set_sans_serif_family(name);
        }
        Arc::new(db)
    }))
}
fn escape(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_control())
        .collect::<String>()
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
pub(super) fn typeset(d: &Design, opts: &usvg::Options, palette: &Palette) -> String {
    let words = d.title.split_whitespace().collect::<Vec<_>>();
    if words.is_empty() {
        return String::new();
    }
    let mut measure = String::from(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="240" height="360" font-family="sans-serif" font-size="100">"#,
    );
    for (i, word) in words.iter().enumerate() {
        let _ = write!(
            measure,
            r#"<text id="w{i}" font-weight="bold" y="100">{}</text>"#,
            escape(word)
        );
    }
    let _ = write!(
        measure,
        r#"<text id="credit" y="100">{}</text></svg>"#,
        escape(&d.author)
    );
    let Ok(probe) = usvg::Tree::from_str(&measure, opts) else {
        return String::new();
    };
    let width = |id: &str| {
        probe
            .node_by_id(id)
            .map_or(0., |n| n.abs_bounding_box().width() / 100.)
    };
    let widths: Vec<_> = (0..words.len()).map(|i| width(&format!("w{i}"))).collect();
    if widths.iter().all(|w| *w == 0.) {
        return String::new();
    }
    // Large lettering for short titles; keep the author and cover art clear on long titles.
    let mut size = 42.;
    let mut lines: Vec<String> = Vec::new();
    loop {
        let mut used = 0.;
        lines.clear();
        for (word, &w) in words.iter().zip(&widths) {
            if !lines.is_empty() && (used + 0.34 + w) * size <= 206. {
                let last = lines.last_mut().unwrap();
                last.push(' ');
                last.push_str(word);
                used += 0.34 + w;
            } else {
                lines.push(word.to_string());
                used = w;
            }
        }
        if size <= 10.
            || (lines.len() as f32 * size <= 113. && widths.iter().all(|w| w * size <= 206.))
        {
            break;
        }
        size -= 1.;
    }
    let max_rows = (113. / size).floor() as usize;
    if lines.len() > max_rows {
        lines.truncate(max_rows);
        if let Some(s) = lines.last_mut() {
            s.push('…');
        }
    }
    let ink = palette.text.hex();
    let mut body = format!(r#"<g font-family="sans-serif" fill="{ink}">"#);
    let mut baseline = 32. + size * 0.8;
    for line in lines {
        let _ = write!(
            body,
            r#"<text x="17" y="{baseline}" font-size="{size}" font-weight="bold">{}</text>"#,
            escape(&line)
        );
        baseline += size;
    }
    let credit_width = width("credit");
    if credit_width > 0. {
        let author_size = (206. / credit_width).min(13.);
        if author_size >= 6. {
            let y = baseline - size + author_size + 10.;
            let _ = write!(
                body,
                r#"<text x="17" y="{y}" font-size="{author_size}">{}</text>"#,
                escape(&d.author)
            );
        }
    }
    body.push_str("</g>");
    body
}
