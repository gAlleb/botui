//! Page rasterization without a terminal, account, or network dependency.
use botui_formats::FileKind;
use image::DynamicImage;
use std::{
    io::{self, Read, Seek},
    path::Path,
    process::{Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::{Duration, Instant},
};

fn output(mut cmd: Command, cancel: &AtomicBool, limit: u64) -> io::Result<Vec<u8>> {
    let program = cmd.get_program().to_string_lossy().into_owned();
    let mut file = tempfile::tempfile()?;
    let mut proc = cmd
        .env("LC_ALL", "C.UTF-8")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .stdout(file.try_clone()?)
        .spawn()
        .map_err(|e| io::Error::new(e.kind(), format!("Could not start {program}: {e}")))?;
    let started = Instant::now();
    loop {
        if let Some(status) = proc.try_wait()? {
            if !status.success() {
                return Err(io::Error::other(format!(
                    "{program} could not read this document"
                )));
            }
            break;
        }
        if cancel.load(Ordering::Acquire)
            || started.elapsed() > Duration::from_secs(25)
            || file.metadata()?.len() > limit
        {
            let _ = proc.kill();
            let _ = proc.wait();
            return Err(io::Error::other("Page rendering interrupted"));
        }
        thread::sleep(Duration::from_millis(15));
    }
    if file.metadata()?.len() > limit {
        return Err(io::Error::other("Page too large"));
    }
    file.rewind()?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    Ok(bytes)
}
fn render_command(
    cmd: Command,
    kind: FileKind,
    path: &Path,
    n: u32,
    side: u16,
    cancel: &AtomicBool,
    limit: u64,
) -> io::Result<Vec<u8>> {
    match output(cmd, cancel, limit) {
        Err(e) if kind == FileKind::Pdf && e.kind() == io::ErrorKind::NotFound => {
            let mut fallback = Command::new("python3");
            fallback
                .args(["-I", "-c", include_str!("poppler.py")])
                .arg(path)
                .arg(n.to_string())
                .arg(side.to_string());
            output(fallback,cancel,limit).map_err(|_|io::Error::other("Could not render PDF. Install Poppler utilities or Python with Poppler GI and Cairo, and check the document."))
        }
        out => out,
    }
}
pub fn page_count(path: &Path, kind: FileKind, cancel: &AtomicBool) -> io::Result<u32> {
    let path = path.canonicalize()?;
    let cmd = match kind {
        FileKind::Pdf => {
            let mut c = Command::new("pdfinfo");
            c.arg(&path);
            c
        }
        FileKind::Djvu => {
            let mut c = Command::new("djvused");
            c.arg(&path).args(["-e", "n"]);
            c
        }
        _ => return Err(io::Error::other("Expected PDF or DjVu")),
    };

    let v = render_command(cmd, kind, &path, 0, 0, cancel, 2 << 20)?;
    let s = String::from_utf8_lossy(&v);
    let count = if kind == FileKind::Pdf {
        s.lines().find_map(|l| l.strip_prefix("Pages:"))
    } else {
        Some(s.trim())
    };
    count
        .and_then(|v| v.trim().parse().ok())
        .filter(|v| *v > 0)
        .ok_or_else(|| io::Error::other("Could not read the page count"))
}
pub fn page(
    path: &Path,
    kind: FileKind,
    n: u32,
    largest: u16,
    cancel: &AtomicBool,
) -> io::Result<DynamicImage> {
    let path = path.canonicalize()?;
    let side = largest.clamp(200, 1920);
    let mut cmd;
    match kind {
        FileKind::Pdf => {
            cmd = Command::new("pdftoppm");
            cmd.args([
                "-png",
                "-singlefile",
                "-f",
                &n.to_string(),
                "-l",
                &n.to_string(),
                "-scale-to",
                &side.to_string(),
            ])
            .arg(&path);
        }
        FileKind::Djvu => {
            cmd = Command::new("ddjvu");
            cmd.arg("-format=ppm")
                .arg(format!("-size={side}x{side}"))
                .arg(format!("-page={n}"))
                .arg(&path)
                .arg("-");
        }
        _ => return Err(io::Error::other("Expected PDF or DjVu")),
    }
    let bytes = render_command(cmd, kind, &path, n, side, cancel, 24 << 20)?;
    image::load_from_memory(&bytes).map_err(io::Error::other)
}

/// Page indices in outlines are one-based, like the rasterizer. On installations
/// without the optional PDF outline backend the reader can still navigate by page.
pub fn outline(path: &Path, kind: FileKind, cancel: &AtomicBool) -> io::Result<Vec<(u32, String)>> {
    let path = path.canonicalize()?;
    match kind {
        FileKind::Pdf => {
            let mut cmd = Command::new("python3");
            cmd.args(["-I", "-c", include_str!("outline.py")]).arg(path);
            let bytes = output(cmd, cancel, 2 << 20)?;
            Ok(String::from_utf8_lossy(&bytes)
                .lines()
                .filter_map(|line| {
                    let (n, title) = line.split_once('\t')?;
                    Some((n.parse().ok()?, title.to_owned()))
                })
                .collect())
        }
        FileKind::Djvu => {
            let mut cmd = Command::new("djvused");
            cmd.arg("-u").arg(path).args(["-e", "print-outline"]);
            let bytes = output(cmd, cancel, 2 << 20)?;
            // Each bookmark begins with ( "label" "#page", followed by its children.
            let mut chars = String::from_utf8_lossy(&bytes)
                .chars()
                .collect::<Vec<_>>()
                .into_iter()
                .peekable();
            let mut entries = Vec::new();
            while let Some(c) = chars.next() {
                if c != '"' {
                    continue;
                }
                let mut label = String::new();
                while let Some(c) = chars.next() {
                    if c == '"' {
                        break;
                    }
                    if c == '\\' {
                        if let Some(next) = chars.next() {
                            label.push(next);
                        }
                    } else {
                        label.push(c);
                    }
                }
                while chars.peek().is_some_and(|c| c.is_whitespace()) {
                    chars.next();
                }
                if chars.next_if_eq(&'"').is_none() {
                    continue;
                }
                let target: String = chars.by_ref().take_while(|c| *c != '"').collect();
                if let Some(n) = target
                    .strip_prefix('#')
                    .and_then(|s| s.parse::<u32>().ok())
                    .filter(|n| *n > 0)
                {
                    entries.push((n, label.replace(char::is_control, " ")));
                }
            }
            Ok(entries)
        }
        _ => Ok(Vec::new()),
    }
}

/// Search the embedded text layer. Keep a physical page for empty scans as well.
pub fn search(
    path: &Path,
    kind: FileKind,
    term: &str,
    cancel: &AtomicBool,
) -> io::Result<Vec<u32>> {
    let path = path.canonicalize()?;
    let mut cmd = match kind {
        FileKind::Pdf => {
            let mut c = Command::new("pdftotext");
            c.args(["-enc", "UTF-8"]).arg(&path).arg("-");
            c
        }
        FileKind::Djvu => {
            let mut c = Command::new("djvutxt");
            c.arg("-detail=page").arg(&path);
            c
        }
        _ => return Err(io::ErrorKind::InvalidInput.into()),
    };
    let limit = 32 << 20;
    let bytes = match output(cmd, cancel, limit) {
        Err(e) if e.kind() == io::ErrorKind::NotFound && kind == FileKind::Pdf => {
            cmd = Command::new("python3");
            cmd.args(["-I", "-c", include_str!("poppler.py")])
                .arg(&path)
                .arg("text");
            output(cmd, cancel, limit)?
        }
        r => r?,
    };
    let text = String::from_utf8_lossy(&bytes);
    let term = term
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    if term.is_empty() {
        return Ok(vec![]);
    }
    let matches = |s: &str| {
        s.split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase()
            .contains(&term)
    };
    let mut pages = Vec::new();
    let mut any_text = false;
    if kind == FileKind::Pdf {
        for (i, page) in text.split('\x0c').enumerate() {
            any_text |= !page.trim().is_empty();
            if matches(page) {
                pages.push(i as u32 + 1);
            }
        }
    } else {
        // miniexp prints one root expression per physical page. Quoted strings
        // can span lines and contain escaped quotes and parentheses.
        let mut value = String::new();
        let mut quoted = false;
        let mut escape = false;
        let mut depth = 0u32;
        let mut page = 0;
        for c in text.chars() {
            if quoted {
                if escape {
                    value.push(match c {
                        'n' | 'r' | 't' => ' ',
                        _ => c,
                    });
                    escape = false;
                } else if c == '\\' {
                    escape = true;
                } else if c == '"' {
                    quoted = false;
                    value.push(' ');
                } else {
                    value.push(c);
                }
            } else {
                match c {
                    '"' => quoted = true,
                    '(' => depth += 1,
                    ')' => {
                        depth = depth.saturating_sub(1);
                        if depth == 0 {
                            page += 1;
                            any_text |= !value.trim().is_empty();
                            if matches(&value) {
                                pages.push(page);
                            }
                            value.clear();
                        }
                    }
                    _ => (),
                }
            }
        }
    }
    if !any_text {
        return Err(io::Error::other("This scan has no searchable text layer."));
    }
    Ok(pages)
}
