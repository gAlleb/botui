use super::{EditorTarget, launch};
use botui_fixed::synctex::Source;
use std::{io, os::unix::net::UnixStream, path::Path, process::Command, sync::atomic::AtomicBool};

fn connected(path: &Path) -> io::Result<bool> {
    match UnixStream::connect(path) {
        Ok(_) => Ok(true),
        Err(e)
            if matches!(
                e.kind(),
                io::ErrorKind::NotFound | io::ErrorKind::ConnectionRefused
            ) =>
        {
            Ok(false)
        }
        Err(e) => Err(e),
    }
}

fn expression(source: &Source) -> io::Result<String> {
    let args = serde_json::to_string(&(
        botui_fixed::synctex::as_str(&source.path)?,
        source.line.max(1),
        source.column.max(1),
    ))?;
    // JSON is data inside a Vim single-quoted string. No filename is ever
    // parsed as Ex commands, shell syntax, or --remote-send key notation.
    let lua = "(function(a) vim.api.nvim_cmd({cmd='edit',args={a[1]},magic={file=false,bar=false}},{}); local l=math.min(a[2],vim.api.nvim_buf_line_count(0)); local s=vim.api.nvim_buf_get_lines(0,l-1,l,false)[1]; vim.api.nvim_win_set_cursor(0,{l,math.min(a[3]-1,#s)}); return true end)(_A)";
    Ok(format!(
        "luaeval('{}', json_decode('{}'))",
        lua.replace('\'', "''"),
        args.replace('\'', "''")
    ))
}

fn reveal(socket: &Path, source: &Source, cancel: &AtomicBool) -> io::Result<()> {
    launch::cancelled(cancel)?;
    let mut cmd = Command::new("nvim");
    cmd.arg("--server")
        .arg(socket)
        .arg("--remote-expr")
        .arg(expression(source)?);
    launch::Process::spawn(&mut cmd, false)?.wait(cancel)
}

pub(super) fn open(
    settings: &crate::settings::Settings,
    source: &Source,
    target: &EditorTarget,
    cancel: &AtomicBool,
) -> io::Result<()> {
    launch::cancelled(cancel)?;
    for socket in target.peer.iter().chain(std::iter::once(&target.owned)) {
        if connected(socket)? {
            // A live editor's refusal (e.g. an unsaved buffer) is an error,
            // not permission to create a second editor or discard changes.
            return reveal(socket, source, cancel);
        }
    }
    let _lock = launch::lock(cancel)?;
    if connected(&target.owned)? {
        return reveal(&target.owned, source, cancel);
    }
    let mut child = launch::terminal(
        &settings.synctex_nvim_terminal,
        Path::new("nvim"),
        &[
            "--listen".into(),
            target.owned.as_os_str().to_owned(),
            format!("+{}", source.line.max(1)).into(),
            "--".into(),
            source.path.as_os_str().to_owned(),
        ],
    )?;
    // Once launched, finish the bounded startup even if a newer click cancels
    // this jump. The next queued job can then reuse its socket, not launch a
    // second window while the first editor is still loading its configuration.
    launch::until_ready(&mut child, &AtomicBool::new(false), "Neovim", || {
        connected(&target.owned)
    })?;
    reveal(&target.owned, source, cancel)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_name_remains_data_not_editor_commands() {
        let source = Source {
            path: "/tmp/кириллица ' \" <CR> | {line} \\.tex".into(),
            line: 12,
            column: 3,
        };
        let expr = expression(&source).unwrap();
        let json = expr
            .split("json_decode('")
            .nth(1)
            .unwrap()
            .strip_suffix("'))")
            .unwrap()
            .replace("''", "'");
        let (file, line, column): (String, u32, u32) = serde_json::from_str(&json).unwrap();
        assert_eq!(file, source.path.to_str().unwrap());
        assert_eq!((line, column), (12, 3));
    }
}
