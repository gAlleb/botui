//! Bounded startup/RPC waits; terminal windows are independent of the reader.
use std::{
    env,
    ffi::OsString,
    fs::{self, File},
    io::{self, Read},
    path::Path,
    process::{Child, Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

const STARTUP: Duration = Duration::from_secs(10);
const TICK: Duration = Duration::from_millis(50);

pub(super) fn cancelled(cancel: &AtomicBool) -> io::Result<()> {
    if cancel.load(Ordering::Acquire) {
        Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "SyncTeX cancelled",
        ))
    } else {
        Ok(())
    }
}

pub(super) struct Process {
    child: Option<Child>,
    errors: Arc<Mutex<Vec<u8>>>,
    detach: bool,
}

impl Process {
    pub fn spawn(command: &mut Command, detach: bool) -> io::Result<Self> {
        let mut child = command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()?;
        let mut pipe = child.stderr.take().unwrap();
        let errors = Arc::new(Mutex::new(Vec::new()));
        let output = errors.clone();
        // Drain the pipe even after the diagnostic limit, so a noisy child can
        // neither block nor grow a capture file for the lifetime of a window.
        thread::spawn(move || {
            let mut buffer = [0; 2048];
            while let Ok(n) = pipe.read(&mut buffer) {
                if n == 0 {
                    break;
                }
                if let Ok(mut bytes) = output.lock() {
                    let keep = n.min(8192usize.saturating_sub(bytes.len()));
                    bytes.extend_from_slice(&buffer[..keep]);
                }
            }
        });
        Ok(Self {
            child: Some(child),
            errors,
            detach,
        })
    }

    pub fn failure(&self, description: &str) -> io::Error {
        let detail = self
            .errors
            .lock()
            .map(|b| String::from_utf8_lossy(&b).trim().to_owned())
            .unwrap_or_default();
        io::Error::other(if detail.is_empty() {
            description.into()
        } else {
            format!("{description}: {detail}")
        })
    }

    pub fn wait(&mut self, cancel: &AtomicBool) -> io::Result<()> {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            cancelled(cancel)?;
            if let Some(status) = self.child.as_mut().unwrap().try_wait()? {
                return if status.success() {
                    Ok(())
                } else {
                    Err(self.failure("Neovim could not open the source"))
                };
            }
            if Instant::now() >= deadline {
                return Err(self.failure("Neovim did not respond within 3 seconds"));
            }
            thread::sleep(TICK);
        }
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            if !self.detach {
                let _ = child.kill();
            }
            thread::spawn(move || {
                let _ = child.wait();
            });
        }
    }
}

fn available(program: &str) -> bool {
    use std::os::unix::fs::PermissionsExt;
    env::split_paths(&env::var_os("PATH").unwrap_or_default()).any(|p| {
        fs::metadata(p.join(program))
            .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
    })
}

fn auto_terminal() -> io::Result<Vec<String>> {
    let mut candidates = Vec::new();
    if env::var_os("KITTY_WINDOW_ID").is_some() {
        candidates.push("kitty");
    }
    if env::var_os("ALACRITTY_SOCKET").is_some() || env::var_os("ALACRITTY_WINDOW_ID").is_some() {
        candidates.push("alacritty");
    }
    if env::var("TERM").is_ok_and(|t| t.contains("foot")) {
        candidates.push("foot");
    }
    candidates.extend(["kitty", "alacritty", "foot", "x-terminal-emulator", "xterm"]);
    let name = candidates
        .into_iter()
        .find(|c| available(c))
        .ok_or_else(|| io::Error::other("Choose a terminal command in F2 → Reader → SyncTeX"))?;
    Ok(
        if matches!(name, "alacritty" | "x-terminal-emulator" | "xterm") {
            vec![name.into(), "-e".into()]
        } else {
            vec![name.into()]
        },
    )
}

pub(super) fn terminal(
    prefix: &[String],
    program: &Path,
    args: &[OsString],
) -> io::Result<Process> {
    let auto;
    let prefix = if prefix.is_empty() {
        auto = auto_terminal()?;
        &auto
    } else {
        prefix
    };
    let mut command = Command::new(&prefix[0]);
    command.args(&prefix[1..]).arg(program).args(args);
    // Do not let a new editor inherit the enclosing editor's RPC identity.
    // Forward sends the actual initiating identity separately in its request.
    command.env_remove("NVIM").env_remove("NVIM_LISTEN_ADDRESS");
    Process::spawn(&mut command, true)
}

pub(super) fn until_ready(
    child: &mut Process,
    cancel: &AtomicBool,
    name: &str,
    mut ready: impl FnMut() -> io::Result<bool>,
) -> io::Result<()> {
    let deadline = Instant::now() + STARTUP;
    loop {
        cancelled(cancel)?;
        if ready()? {
            return Ok(());
        }
        if let Some(status) = child.child.as_mut().unwrap().try_wait()?
            && !status.success()
        {
            return Err(child.failure(&format!("{name} terminal exited with {status}")));
        }
        if Instant::now() >= deadline {
            return Err(child.failure(&format!(
                "{name} did not become ready within 10 seconds; check its terminal command"
            )));
        }
        thread::sleep(TICK);
    }
}

pub(super) fn lock(cancel: &AtomicBool) -> io::Result<File> {
    let file = File::options()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(super::directory()?.join("launch.lock"))?;
    let deadline = Instant::now() + STARTUP;
    loop {
        cancelled(cancel)?;
        match file.try_lock() {
            Ok(()) => return Ok(file),
            Err(std::fs::TryLockError::WouldBlock) if Instant::now() < deadline => {
                thread::sleep(TICK)
            }
            Err(e) => {
                return Err(io::Error::other(format!(
                    "Another SyncTeX window is starting: {e}"
                )));
            }
        }
    }
}
