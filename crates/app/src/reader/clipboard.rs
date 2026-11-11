use base64::Engine;
use std::{
    io::{self, Write},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

pub fn copy(text: &str) -> io::Result<&'static str> {
    if text.is_empty() {
        return Err(io::Error::other("Select some text first"));
    }
    let candidates: &[(&str, &[&str])] = if cfg!(target_os = "macos") {
        &[("pbcopy", &[])]
    } else if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        &[
            ("wl-copy", &["--type", "text/plain;charset=utf-8"]),
            ("xclip", &["-selection", "clipboard"]),
        ]
    } else {
        &[
            ("xclip", &["-selection", "clipboard"]),
            ("xsel", &["--clipboard", "--input"]),
        ]
    };
    for (program, args) in candidates {
        let Ok(mut child) = Command::new(program)
            .args(*args)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        else {
            continue;
        };
        // Write and wait away from the event loop; the bounded wait also handles
        // a clipboard helper whose display server has disappeared.
        let input = text.as_bytes().to_vec();
        let mut stdin = child.stdin.take().unwrap();
        let writer = std::thread::spawn(move || stdin.write_all(&input));
        let start = Instant::now();
        loop {
            if let Some(status) = child.try_wait()? {
                if status.success() && writer.join().is_ok_and(|r| r.is_ok()) {
                    return Ok("Copied to clipboard");
                }
                break;
            }
            if start.elapsed() > Duration::from_secs(2) {
                let _ = child.kill();
                let _ = child.wait();
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    if text.len() > 100_000 {
        return Err(io::Error::other(
            "Selection is too large for terminal clipboard; install wl-copy or xclip",
        ));
    }
    let encoded = base64::engine::general_purpose::STANDARD.encode(text);
    let seq = format!("\x1b]52;c;{encoded}\x07");
    let mut out = io::stdout().lock();
    if std::env::var_os("TMUX").is_some() {
        write!(out, "\x1bPtmux;{}\x1b\\", seq.replace('\x1b', "\x1b\x1b"))?;
    } else {
        out.write_all(seq.as_bytes())?;
    }
    out.flush()?;
    // OSC 52 has no success acknowledgement. Be explicit instead of claiming
    // a clipboard update that a terminal can refuse by configuration.
    Ok("Sent to terminal clipboard (OSC 52)")
}
