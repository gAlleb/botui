//! Compare the server's projected bookmarks with the exact bytes of one edition.
//! No progress writes, bookmark edits, local sessions or source URL in the report.
use crate::settings::Settings;
use botui_bookorbit::Result;
use botui_bookorbit::{BookOrbit, CatalogOrder, Session};
use std::sync::atomic::AtomicBool;
pub fn bookmarks(query: &str) -> Result<()> {
    if query.trim().is_empty() {
        return Err("Supply a title or a distinctive part of it.".into());
    }
    let cfg = Settings::load()?.bookorbit;
    if !cfg.has_credentials() {
        return Err("Configure an account in F2 → BookOrbit to run this check.".into());
    }
    let api = BookOrbit::new(&cfg.server)?;
    let session = api.sign_in(&cfg.username, &cfg.resolve(&AtomicBool::new(false))?)?;
    let result = check(&session, query.trim());
    let logout = session.logout();
    result?;
    logout?;
    Ok(())
}
fn check(session: &Session, query: &str) -> Result<()> {
    let page = session
        .with_token(|api, t| api.catalog_query(t, None, 0, 32, CatalogOrder::Title, Some(query)))?;
    let q = query.to_lowercase();
    let name = |b: &botui_bookorbit::Book| b.title.as_deref().unwrap_or("").to_lowercase();
    let matches: Vec<_> = page.items.iter().filter(|b| name(b).contains(&q)).collect();
    let exact = matches
        .iter()
        .filter(|b| name(b) == q)
        .copied()
        .collect::<Vec<_>>();
    let book = match (exact.as_slice(), matches.as_slice()) {
        ([b], _) if page.total <= 32 => *b,
        (_, [b]) if page.total <= 32 => *b,
        (_, []) => return Err("No matching title found.".into()),
        _ => {
            for b in matches.iter().take(12) {
                println!("  {}", clean(b.title.as_deref().unwrap_or("Untitled")));
            }
            return Err("Multiple results. Enter a more specific title.".into());
        }
    };
    println!("{}", clean(book.title.as_deref().unwrap_or("Untitled")));
    let mut n = 0;
    for f in &book.files {
        let kind = f.format.as_deref().unwrap_or("").to_ascii_lowercase();
        if !matches!(kind.as_str(), "epub" | "fb2") {
            continue;
        }
        n += 1;
        let marks = session.with_token(|api, t| api.bookmarks(t, book.id, f.id))?;
        let with_cfi = marks
            .iter()
            .filter_map(|m| m.cfi.as_deref())
            .collect::<Vec<_>>();
        let located = if with_cfi.is_empty() {
            0
        } else {
            let edition = crate::catalog::BookFile {
                id: f.id,
                format: f.format.clone(),
                size_bytes: f.size_bytes,
                role: f.role.clone(),
            };
            let file = session.with_token(|api, t| {
                crate::cache::fetch(
                    api,
                    t,
                    &edition,
                    &AtomicBool::new(false),
                    &botui_transfer::Progress::default(),
                )
            })?;
            let d = botui_formats::open_document(&file)
                .map_err(|e| std::io::Error::other(e.to_string()))?;
            with_cfi
                .iter()
                .filter(|c| d.bookmark_anchor(c).is_some())
                .count()
        };
        println!(
            "{} · file {}: {} total, {} located, {} without an address from the server, {} unrecognized.",
            kind.to_uppercase(),
            f.id,
            marks.len(),
            located,
            marks.len() - with_cfi.len(),
            with_cfi.len() - located
        );
    }
    if n == 0 {
        println!("No EPUB or FB2 file on this title.");
    }
    Ok(())
}
fn clean(s: &str) -> String {
    s.replace(char::is_control, " ")
}
