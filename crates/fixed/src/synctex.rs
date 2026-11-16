//! SyncTeX coordinates and PDF text geometry both use points from the top left of the page.
use std::{
    io,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::AtomicBool,
};

fn query(pdf: &Path, mode: &str, args: &[&str], cancel: &AtomicBool) -> io::Result<String> {
    let mut command = Command::new("synctex");
    command
        .current_dir(pdf.parent().ok_or(io::ErrorKind::InvalidInput)?)
        .env_remove("SYNCTEX_VIEWER")
        .env_remove("SYNCTEX_EDITOR")
        .arg(mode)
        .args(args);
    String::from_utf8(super::output(command, cancel, 1024 * 1024)?).map_err(io::Error::other)
}

/// Return the first enclosing line box. SyncTeX gives its bottom edge in v.
pub fn forward(pdf: &Path, position: &str, cancel: &AtomicBool) -> io::Result<(u32, [f32; 4])> {
    let report = query(pdf, "view", &["-i", position, "-o", as_str(pdf)?], cancel)?;
    let mut record = report.lines().skip_while(|l| !l.starts_with("Page:"));
    let page = record
        .next()
        .and_then(|l| l.strip_prefix("Page:"))
        .and_then(|s| s.parse::<u32>().ok())
        .filter(|p| *p > 0)
        .ok_or_else(|| io::Error::other("No SyncTeX match for this source line"))?;
    let fields = record
        .take_while(|s| !s.starts_with("Page:") && !s.starts_with("SyncTeX result end"))
        .collect::<Vec<_>>();
    let number = |name: &str| {
        fields
            .iter()
            .find_map(|l| l.strip_prefix(name))
            .and_then(|s| s.parse::<f32>().ok())
            .filter(|n| n.is_finite())
            .ok_or_else(|| io::Error::other("Invalid SyncTeX coordinates"))
    };
    let (x, bottom, w, h) = (number("h:")?, number("v:")?, number("W:")?, number("H:")?);
    let [width, height] = super::text::page_text(pdf, page, cancel)?.size;
    let bounds = [
        x / width,
        (bottom - h) / height,
        (x + w.max(1.0)) / width,
        bottom / height,
    ]
    .map(|v| v.clamp(0., 1.));
    if bounds[0] >= bounds[2] || bounds[1] >= bounds[3] {
        return Err(io::Error::other("SyncTeX match is outside this PDF page"));
    }
    Ok((page, bounds))
}

pub struct Source {
    pub path: PathBuf,
    pub line: u32,
    /// One-based for editors, including when TeX reports no source column.
    pub column: u32,
}

pub fn inverse(pdf: &Path, page: u32, point: [f32; 2], cancel: &AtomicBool) -> io::Result<Source> {
    let [width, height] = super::text::page_text(pdf, page, cancel)?.size;
    let (x, y) = (point[0] * width, point[1] * height);
    let pos = format!("{page}:{x}:{y}:{}", as_str(pdf)?);
    let report = query(pdf, "edit", &["-o", &pos], cancel)?;
    let mut lines = report.lines().skip_while(|l| !l.starts_with("Input:"));
    let source = lines
        .next()
        .and_then(|l| l.strip_prefix("Input:"))
        .filter(|path| !path.is_empty())
        .ok_or_else(|| io::Error::other("No SyncTeX source at this point"))?;
    let path = pdf
        .parent()
        .ok_or(io::ErrorKind::InvalidInput)?
        .join(source)
        .canonicalize()?;
    let mut record = lines.take_while(|l| !l.starts_with("Input:"));
    let line = record
        .find_map(|l| l.strip_prefix("Line:"))
        .and_then(|s| s.parse::<u32>().ok())
        .filter(|n| *n > 0)
        .ok_or_else(|| io::Error::other("Invalid SyncTeX source line"))?;
    let column = record
        .find_map(|l| l.strip_prefix("Column:"))
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(0)
        .saturating_add(1);
    Ok(Source { path, line, column })
}

pub fn as_str(path: &Path) -> io::Result<&str> {
    path.to_str()
        .ok_or_else(|| io::Error::other("SyncTeX needs UTF-8 paths"))
}
