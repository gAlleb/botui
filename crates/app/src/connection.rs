use serde::{Deserialize, Serialize};
use std::{
    io,
    process::{Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub(crate) struct Connection {
    pub server: String,
    pub username: String,
    pub list_view: bool,
    pub libraries_collapsed: bool,
    pub collections_collapsed: bool,
    pub password: Option<String>,
    pub password_command: Option<PasswordCommand>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub(crate) enum PasswordCommand {
    Argv(Vec<String>),
    Shell(String),
}

impl Connection {
    pub fn has_credentials(&self) -> bool {
        self.password_command.is_some() || self.password.as_ref().is_some_and(|p| !p.is_empty())
    }
    pub fn matches(&self, server: &str, username: &str) -> bool {
        !self.server.trim().is_empty()
            && !self.username.trim().is_empty()
            && self.server.trim().trim_end_matches('/') == server.trim().trim_end_matches('/')
            && self.username.trim() == username.trim()
    }
    pub fn resolve(&self, cancel: &AtomicBool) -> io::Result<String> {
        match &self.password_command {
            Some(command) => command.run(cancel),
            None => self
                .password
                .clone()
                .filter(|v| !v.is_empty())
                .ok_or_else(|| io::Error::other("Enter your BookOrbit password.")),
        }
    }
}

impl PasswordCommand {
    fn run(&self, cancel: &AtomicBool) -> io::Result<String> {
        use std::io::Read;
        use std::os::{fd::OwnedFd, unix::net::UnixStream};
        let mut command = match self {
            Self::Argv(args) if args.first().is_some_and(|s| !s.is_empty()) => {
                let mut cmd = Command::new(&args[0]);
                cmd.args(&args[1..]);
                cmd
            }
            Self::Shell(script) if !script.trim().is_empty() => {
                let mut cmd = Command::new("sh");
                cmd.args(["-c", script]);
                cmd
            }
            _ => return Err(io::Error::other("bookorbit.password_command is empty")),
        };
        let (mut output, writer) = UnixStream::pair()?;
        output.set_nonblocking(true)?;
        let mut child = command
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .stdout(Stdio::from(OwnedFd::from(writer)))
            .spawn()
            .map_err(|_| io::Error::other("Could not start bookorbit.password_command"))?;
        drop(command);
        let start = Instant::now();
        let mut first = Vec::new();
        let mut complete = false;
        let mut buf = [0; 4096];
        let result = (|| {
            loop {
                if cancel.load(Ordering::Acquire) || start.elapsed() > Duration::from_secs(120) {
                    return Err(io::Error::other(
                        "BookOrbit password command cancelled or timed out",
                    ));
                }
                match output.read(&mut buf) {
                    Ok(0) => {
                        if let Some(status) = child.try_wait()? {
                            break status;
                        }
                    }
                    Ok(n) => {
                        if !complete {
                            let line = &buf[..n];
                            let end = line.iter().position(|b| *b == b'\n');
                            first.extend_from_slice(&line[..end.unwrap_or(n)]);
                            if first.len() > 16384 {
                                return Err(io::Error::other(
                                    "Password command returned an overlong first line",
                                ));
                            }
                            complete = end.is_some();
                        }
                        continue;
                    }
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => (),
                    Err(e) => return Err(e),
                }
                std::thread::sleep(Duration::from_millis(15));
            }
            .success()
            .then_some(())
            .ok_or_else(|| io::Error::other("BookOrbit password command failed"))?;
            if first.last() == Some(&b'\r') {
                first.pop();
            }
            String::from_utf8(first)
                .ok()
                .filter(|s| !s.is_empty() && !s.contains('\0'))
                .ok_or_else(|| {
                    io::Error::other(
                        "Password command must return a nonempty UTF-8 password on its first line",
                    )
                })
        })();
        if result.is_err() {
            let _ = child.kill();
            let _ = child.wait();
        }
        result
    }
}

pub(crate) fn remember(server: &str, username: &str, list_view: bool) -> io::Result<()> {
    crate::settings::Settings::update(|s| {
        let c = &mut s.bookorbit;
        // A login typed for another account must never rebind an existing password.
        if !c.has_credentials() || c.matches(server, username) {
            c.server = server.to_owned();
            c.username = username.to_owned();
        }
        c.list_view = list_view;
    })?;
    Ok(())
}
