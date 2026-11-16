//! Editor integration for a particular open local PDF. No commands run while reading normally.
use botui_store::PdfRegion;
use sha2::{Digest, Sha256};
use std::{
    env, fs, io,
    os::unix::{
        fs::{DirBuilderExt, PermissionsExt},
        net::UnixDatagram,
    },
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::AtomicBool,
};

fn directory() -> io::Result<PathBuf> {
    let path = env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .filter(|d| d.is_absolute())
        .map_or_else(botui_store::state_dir, Ok)?
        .join("botui/synctex");
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&path)?;
    let meta = path.symlink_metadata()?;
    if !meta.is_dir() || meta.permissions().mode() & 0o077 != 0 {
        return Err(io::Error::other(
            "SyncTeX needs a private runtime directory",
        ));
    }
    Ok(path)
}

fn prefix(pdf: &Path) -> String {
    let hash = format!("{:x}", Sha256::digest(pdf.as_os_str().as_encoded_bytes()));
    format!("{}-", &hash[..24])
}

pub(crate) struct Inbox {
    socket: UnixDatagram,
    path: PathBuf,
    pub pending: Option<PdfRegion>,
}

impl Inbox {
    pub fn open(pdf: &Path) -> io::Result<Self> {
        let path = directory()?.join(format!("{}{}", prefix(pdf), std::process::id()));
        // Another live process cannot have the same PID. A crash may leave its socket on disk.
        if path.try_exists()? {
            fs::remove_file(&path)?;
        }
        let socket = UnixDatagram::bind(&path)?;
        socket.set_nonblocking(true)?;
        Ok(Self {
            socket,
            path,
            pending: None,
        })
    }

    pub fn poll(&mut self) {
        let mut bytes = [0; 512];
        while let Ok(n) = self.socket.recv(&mut bytes) {
            if let Ok(r) = serde_json::from_slice::<PdfRegion>(&bytes[..n])
                && r.page > 0
                && r.bounds.iter().all(|n| (0.0..=1.0).contains(n))
                && r.bounds[0] < r.bounds[2]
                && r.bounds[1] < r.bounds[3]
            {
                self.pending = Some(r);
            }
        }
    }
}

impl Drop for Inbox {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

pub(crate) fn forward(position: &str, pdf: &Path) -> io::Result<()> {
    let mut spec = position.splitn(3, ':');
    let line = spec
        .next()
        .and_then(|s| s.parse::<u32>().ok())
        .filter(|n| *n > 0);
    let col = spec.next().and_then(|s| s.parse::<u32>().ok());
    let (Some(line), Some(col), Some(source)) = (line, col, spec.next().filter(|s| !s.is_empty()))
    else {
        return Err(io::Error::other(
            "Expected LINE:COLUMN:SOURCE (line starts at 1)",
        ));
    };
    let pdf = pdf.canonicalize()?;
    let source = Path::new(source).canonicalize()?;
    let name = prefix(&pdf);
    let mut windows = fs::read_dir(directory()?)?
        .filter_map(Result::ok)
        .filter(|e| e.file_name().to_string_lossy().starts_with(&name))
        .collect::<Vec<_>>();
    windows.sort_by_key(|e| e.metadata().and_then(|meta| meta.modified()).ok());
    if !windows.is_empty() {
        let position = format!("{line}:{col}:{}", botui_fixed::synctex::as_str(&source)?);
        let (page, bounds) =
            botui_fixed::synctex::forward(&pdf, &position, &AtomicBool::new(false))?;
        let request = serde_json::to_vec(&PdfRegion { page, bounds })?;
        let sender = UnixDatagram::unbound()?;
        sender.set_write_timeout(Some(std::time::Duration::from_millis(500)))?;
        for window in windows.iter().rev() {
            match sender.send_to(&request, window.path()) {
                Ok(_) => return Ok(()),
                Err(e)
                    if matches!(
                        e.kind(),
                        io::ErrorKind::ConnectionRefused | io::ErrorKind::NotFound
                    ) => {}
                Err(e) => return Err(e),
            }
        }
    }
    Err(io::Error::other(
        "Open this local PDF in botui before sending a SyncTeX jump",
    ))
}

pub(crate) fn editor_command(args: &[String]) -> String {
    // NUL cannot occur in a process argument. Make an invalid hand-edited TOML
    // value visible and editable in the same field.
    shlex::try_join(args.iter().map(String::as_str))
        .unwrap_or_else(|_| args.join(" ").replace('\0', "�"))
}

pub(crate) fn parse_editor_command(input: &str) -> io::Result<Vec<String>> {
    let args = shlex::split(input).ok_or_else(|| {
        io::Error::other("Unclosed quote or trailing backslash in the editor command.")
    })?;
    if !input.trim().is_empty() && args.first().is_none_or(String::is_empty) {
        return Err(io::Error::other(
            "Enter the editor executable or clear the field.",
        ));
    }
    Ok(args)
}

pub(crate) fn open_editor(
    editor: &[String],
    source: botui_fixed::synctex::Source,
) -> io::Result<String> {
    let file = botui_fixed::synctex::as_str(&source.path)?;
    let location = format!("{}:{}:{}", file, source.line, source.column);
    let Some((program, args)) = editor.split_first() else {
        return Ok(format!("{location} · choose your editor in F2 → Reader"));
    };
    let values = [
        ("{line}", source.line.to_string()),
        ("{column}", source.column.to_string()),
    ];
    let args = args.iter().map(|arg| {
        let arg = values
            .iter()
            .fold(arg.clone(), |s, (name, value)| s.replace(name, value));
        arg.replace("{file}", file)
    });
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(location)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editable_command_preserves_configured_arguments() {
        let args = [
            "/my applications/редактор",
            "+{line}",
            "{file}:{line}:{column}",
            "quote \" and '",
            r#"C:\path with space\file"#,
            "$(literal);",
            "#argument",
            "",
        ]
        .map(String::from);
        assert_eq!(parse_editor_command(&editor_command(&args)).unwrap(), args);
        assert_eq!(
            parse_editor_command(r#"kitty nvim +{line} -- "{file}""#).unwrap(),
            ["kitty", "nvim", "+{line}", "--", "{file}"]
        );
        assert!(parse_editor_command("   ").unwrap().is_empty());
        for invalid in ["'unfinished", "editor \\", "'' --goto"] {
            assert!(parse_editor_command(invalid).is_err());
        }
    }
}
