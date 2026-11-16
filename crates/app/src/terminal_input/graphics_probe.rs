//! Let the image library query the terminal without consuming the reader's input.
use super::graphics_replies::GraphicsReplies;
use nix::{
    errno::Errno,
    poll::{PollFd, PollFlags, PollTimeout, poll},
    unistd::read,
};
use ratatui_image::picker::{Picker, cap_parser::QueryStdioOptions};
use std::{
    fs::File,
    io::{self, IsTerminal, Write},
    os::fd::{AsFd, BorrowedFd, OwnedFd},
    thread,
    time::{Duration, Instant},
};

pub(super) fn query(
    tty: BorrowedFd<'_>,
    mut options: QueryStdioOptions,
    filter: &mut GraphicsReplies,
    mut on_input: impl FnMut(&[u8]),
) -> Option<Picker> {
    if !io::stdout().is_terminal() {
        return None;
    }
    let timeout = Duration::from_millis(options.timeout_ms.try_into().ok()?);
    let (mut master, slave) = private_pty().ok()?;
    options.stdio = Some((slave.try_clone().ok()?, slave));
    let mut output = File::from(io::stdout().as_fd().try_clone_to_owned().ok()?);
    thread::scope(move |scope| {
        let picker = thread::Builder::new()
            .name("terminal-graphics-query".into())
            .spawn_scoped(scope, move || {
                Picker::from_query_stdio_with_options(options)
            })
            .ok()?;
        let complete = relay(
            tty,
            &mut master,
            &mut output,
            timeout,
            filter,
            &mut on_input,
        );
        if !complete {
            // Hang up the private terminal to wake the library on errors or timeout.
            drop(master);
        }
        let picker = picker.join().ok()?.ok()?;
        complete.then_some(picker)
    })
}

fn relay(
    tty: BorrowedFd<'_>,
    master: &mut File,
    output: &mut File,
    timeout: Duration,
    filter: &mut GraphicsReplies,
    on_input: &mut impl FnMut(&[u8]),
) -> bool {
    let mut started = false;
    let mut block = [0; 4096];
    let mut user = Vec::new();
    let mut replies = Vec::new();
    let mut deadline = Instant::now() + timeout;
    while let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
        // Leave typeahead in the real terminal until the library issues its query.
        let mut fds = [
            PollFd::new(master.as_fd(), PollFlags::POLLIN),
            PollFd::new(
                tty,
                if started {
                    PollFlags::POLLIN
                } else {
                    PollFlags::empty()
                },
            ),
        ];
        let wait = PollTimeout::try_from(remaining).unwrap_or(PollTimeout::MAX);
        match poll(&mut fds, wait) {
            Err(Errno::EINTR) => continue,
            Err(_) | Ok(0) => break,
            Ok(_) => {}
        }
        let outbound = fds[0].revents().unwrap_or(PollFlags::POLLERR);
        let inbound = fds[1].revents().unwrap_or(PollFlags::POLLERR);
        if outbound.contains(PollFlags::POLLIN) {
            let size = match read(&*master, &mut block) {
                Ok(0) | Err(_) => break,
                Ok(n) => n,
            };
            if output.write_all(&block[..size]).is_err() {
                break;
            }
            if !started {
                // In tmux the library enables passthrough before writing the query.
                // Give the terminal its reply timeout starting at the actual query.
                started = true;
                deadline = Instant::now() + timeout;
            }
        } else if !outbound.is_empty() {
            break;
        }
        if inbound.contains(PollFlags::POLLIN) {
            let size = match read(tty, &mut block) {
                Err(Errno::EINTR) => continue,
                Ok(0) | Err(_) => break,
                Ok(n) => n,
            };
            let mut complete = false;
            user.clear();
            replies.clear();
            for &byte in &block[..size] {
                complete |= filter.push(byte, !complete, &mut user, &mut replies);
            }
            if !user.is_empty() {
                on_input(&user);
            }
            if master.write_all(&replies).is_err() {
                break;
            }
            if complete {
                return true;
            }
        } else if !inbound.is_empty() {
            break;
        }
    }
    false
}

#[cfg(any(target_os = "linux", target_os = "android"))]
fn private_pty() -> nix::Result<(File, OwnedFd)> {
    use nix::{
        fcntl::{OFlag, open},
        pty::{grantpt, posix_openpt, ptsname_r, unlockpt},
        sys::stat::Mode,
    };
    let flags = OFlag::O_RDWR | OFlag::O_NOCTTY | OFlag::O_CLOEXEC;
    let master = posix_openpt(flags | OFlag::O_NONBLOCK)?;
    grantpt(&master)?;
    unlockpt(&master)?;
    let slave = open(ptsname_r(&master)?.as_str(), flags, Mode::empty())?;
    Ok((File::from(OwnedFd::from(master)), slave))
}

#[cfg(not(any(target_os = "linux", target_os = "android")))]
fn private_pty() -> nix::Result<(File, OwnedFd)> {
    use nix::{
        fcntl::{FcntlArg, FdFlag, OFlag, fcntl},
        pty::openpty,
    };
    let pair = openpty(None, None)?;
    for descriptor in [&pair.master, &pair.slave] {
        fcntl(descriptor, FcntlArg::F_SETFD(FdFlag::FD_CLOEXEC))?;
    }
    fcntl(&pair.master, FcntlArg::F_SETFL(OFlag::O_NONBLOCK))?;
    Ok((File::from(pair.master), pair.slave))
}
