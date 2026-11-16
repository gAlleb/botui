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

pub(crate) fn open_editor(
    editor: &[String],
    source: botui_fixed::synctex::Source,
) -> io::Result<String> {
    let file = botui_fixed::synctex::as_str(&source.path)?;
    let location = format!("{}:{}:{}", file, source.line, source.column);
    let Some((program, args)) = editor.split_first() else {
        return Ok(format!(
            "{location} · set synctex_editor to open your editor"
        ));
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
