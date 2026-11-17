//! Editor integration for a particular open local PDF. No commands run while reading normally.
use botui_store::PdfRegion;
use serde::{Deserialize, Serialize};
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

mod editor;
mod launch;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Mode {
    #[default]
    Custom,
    Neovim,
}

#[derive(Deserialize, Serialize)]
struct Request {
    #[serde(flatten)]
    region: PdfRegion,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    editor: Option<PathBuf>,
}

#[derive(Clone)]
pub(crate) struct EditorTarget {
    peer: Option<PathBuf>,
    owned: PathBuf,
}

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
    editor: EditorTarget,
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
        let owned = {
            let unique = tempfile::Builder::new()
                .prefix("n-")
                .tempfile_in(directory()?)?;
            unique.path().to_owned()
        };
        Ok(Self {
            socket,
            path,
            pending: None,
            editor: EditorTarget { peer: None, owned },
        })
    }

    pub fn poll(&mut self) {
        let mut bytes = [0; 8192];
        while let Ok(n) = self.socket.recv(&mut bytes) {
            if let Ok(Request { region: r, editor }) =
                serde_json::from_slice::<Request>(&bytes[..n])
                && r.page > 0
                && r.bounds.iter().all(|n| (0.0..=1.0).contains(n))
                && r.bounds[0] < r.bounds[2]
                && r.bounds[1] < r.bounds[3]
            {
                self.pending = Some(r);
                if let Some(peer) = editor.filter(|p| p.is_absolute()) {
                    self.editor.peer = Some(peer);
                }
            }
        }
    }

    pub fn editor(&self) -> EditorTarget {
        self.editor.clone()
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
    let cancel = AtomicBool::new(false);
    let position = format!("{line}:{col}:{}", botui_fixed::synctex::as_str(&source)?);
    let (page, bounds) = botui_fixed::synctex::forward(&pdf, &position, &cancel)?;
    let request = serde_json::to_vec(&Request {
        region: PdfRegion { page, bounds },
        editor: env::var_os("NVIM")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute()),
    })?;
    if send(&pdf, &request)? {
        return Ok(());
    }
    let settings = crate::settings::Settings::load()?;
    if settings.synctex_mode == Mode::Custom {
        return Err(io::Error::other(
            "Open this local PDF in botui before sending a SyncTeX jump",
        ));
    }
    // Serialize first launches, then recheck: fast repeated VimTeX requests
    // must not each open a terminal while the reader is starting.
    let _lock = launch::lock(&cancel)?;
    if send(&pdf, &request)? {
        return Ok(());
    }
    let executable = env::current_exe()?;
    let mut child = launch::terminal(
        &settings.synctex_botui_terminal,
        &executable,
        &["--".into(), pdf.as_os_str().to_owned()],
    )?;
    launch::until_ready(&mut child, &cancel, "BOTUI", || send(&pdf, &request))
}

fn send(pdf: &Path, request: &[u8]) -> io::Result<bool> {
    let name = prefix(pdf);
    let mut windows = fs::read_dir(directory()?)?
        .filter_map(Result::ok)
        .filter(|e| e.file_name().to_string_lossy().starts_with(&name))
        .collect::<Vec<_>>();
    windows.sort_by_key(|e| e.metadata().and_then(|meta| meta.modified()).ok());
    if !windows.is_empty() {
        let sender = UnixDatagram::unbound()?;
        sender.set_write_timeout(Some(std::time::Duration::from_millis(500)))?;
        for window in windows.iter().rev() {
            match sender.send_to(request, window.path()) {
                Ok(_) => return Ok(true),
                Err(e)
                    if matches!(
                        e.kind(),
                        io::ErrorKind::ConnectionRefused | io::ErrorKind::NotFound
                    ) => {}
                Err(e) => return Err(e),
            }
        }
    }
    Ok(false)
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
    settings: &crate::settings::Settings,
    source: botui_fixed::synctex::Source,
    target: Option<EditorTarget>,
    cancel: &AtomicBool,
) -> io::Result<String> {
    let file = botui_fixed::synctex::as_str(&source.path)?;
    let location = format!("{}:{}:{}", file, source.line, source.column);
    if settings.synctex_mode == Mode::Neovim {
        let target = target.ok_or_else(|| io::Error::other("No local PDF session"))?;
        editor::open(settings, &source, &target, cancel)?;
        return Ok(location);
    }
    let editor = &settings.synctex_editor;
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
    launch::cancelled(cancel)?;
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

pub(crate) fn inverse(
    pdf: &Path,
    page: u32,
    point: [f32; 2],
    target: Option<EditorTarget>,
    cancel: &AtomicBool,
) -> io::Result<String> {
    let source = botui_fixed::synctex::inverse(pdf, page, point, cancel)?;
    let settings = crate::settings::Settings::load()?;
    open_editor(&settings, source, target, cancel)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inbox_keeps_editor_identity_when_an_older_client_sends_a_jump() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("reader");
        let socket = UnixDatagram::bind(&path).unwrap();
        socket.set_nonblocking(true).unwrap();
        let mut inbox = Inbox {
            socket,
            path: path.clone(),
            pending: None,
            editor: EditorTarget {
                peer: None,
                owned: dir.path().join("owned"),
            },
        };
        // A valid protocol payload can exceed the former 512-byte buffer.
        let peer = PathBuf::from(format!("/tmp/{}/editor", "кириллица".repeat(50)));
        let sender = UnixDatagram::unbound().unwrap();
        let request = Request {
            region: PdfRegion {
                page: 2,
                bounds: [0.1, 0.2, 0.3, 0.4],
            },
            editor: Some(peer.clone()),
        };
        sender
            .send_to(&serde_json::to_vec(&request).unwrap(), &path)
            .unwrap();
        inbox.poll();
        assert_eq!(inbox.pending.as_ref().unwrap().page, 2);
        assert_eq!(inbox.editor.peer.as_ref(), Some(&peer));
        let old_request = PdfRegion {
            page: 3,
            bounds: [0.2, 0.3, 0.4, 0.5],
        };
        sender
            .send_to(&serde_json::to_vec(&old_request).unwrap(), &path)
            .unwrap();
        inbox.poll();
        assert_eq!(inbox.pending.as_ref().unwrap().page, 3);
        assert_eq!(inbox.editor.peer.as_ref(), Some(&peer));
        sender
            .send_to(br#"{"page":0,"bounds":[0,0,1,1],"editor":"/wrong"}"#, &path)
            .unwrap();
        inbox.poll();
        assert_eq!(inbox.pending.as_ref().unwrap().page, 3);
        assert_eq!(inbox.editor.peer.as_ref(), Some(&peer));
    }

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
