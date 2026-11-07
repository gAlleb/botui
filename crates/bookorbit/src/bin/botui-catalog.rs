//! Print collections from the user's BookOrbit installation for diagnostics.

use botui_bookorbit::{BookOrbit, Result};
use std::io::{self, IsTerminal, Read, Write};
use std::process::ExitCode;

const USAGE: &str =
    "botui-catalog <URL-сервера>\nПередайте временный Bearer-токен через канал стандартного ввода.";

fn run() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    let Some(server) = args.next() else {
        return Err(USAGE.into());
    };
    if args.next().is_some() {
        return Err(USAGE.into());
    }
    if server == "--help" || server == "-h" {
        println!("{USAGE}");
        return Ok(());
    }
    let adapter = BookOrbit::new(server.to_str().ok_or("Некорректный адрес BookOrbit.")?)?;
    if io::stdin().is_terminal() {
        return Err("Передайте Bearer-токен через канал или перенаправленный stdin.".into());
    }
    const MAX_TOKEN: usize = 16 * 1024;
    let mut token = String::new();
    io::stdin()
        .lock()
        .take((MAX_TOKEN + 1) as u64)
        .read_to_string(&mut token)?;
    if token.len() > MAX_TOKEN {
        return Err("Слишком длинный токен.".into());
    }
    let token = token
        .strip_suffix("\r\n")
        .or_else(|| token.strip_suffix('\n'))
        .unwrap_or(&token);
    let collections = adapter.collections(token)?;
    let mut output = io::stdout().lock();
    serde_json::to_writer_pretty(&mut output, &collections)?;
    writeln!(output)?;
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
