//! A remote book is readable only once its private, format-suffixed temporary file is complete.
use botui_bookorbit::{BookFile, BookOrbit, Result};
use botui_formats::FileKind;
use std::{
    io::{self, Write},
    sync::atomic::{AtomicBool, Ordering},
};
use tempfile::{Builder, TempPath};

const MAX_FILE: u64 = 512 << 20;

pub(crate) fn fetch(
    api: &BookOrbit,
    token: &str,
    file: &BookFile,
    cancelled: &AtomicBool,
) -> Result<TempPath> {
    let kind = file
        .format
        .as_deref()
        .and_then(FileKind::from_extension)
        .ok_or("Для этого формата доступна карточка книги")?;
    let suffix = format!(".{}", kind.label().to_ascii_lowercase());
    if file.size_bytes.is_some_and(|size| size > MAX_FILE) {
        return Err("Размер книги превышает лимит загрузки 512 МиБ".into());
    }
    let mut staging = Builder::new().prefix("botui-").suffix(&suffix).tempfile()?;
    let mut writer = BookWriter {
        file: staging.as_file_mut(),
        cancelled,
    };
    writer.check()?;
    let count = api.download(token, file.id, &mut writer, MAX_FILE)?;
    writer.check()?;
    if count == 0 {
        return Err("Сервер вернул пустой файл книги".into());
    }
    Ok(staging.into_temp_path())
}

struct BookWriter<'a> {
    file: &'a mut std::fs::File,
    cancelled: &'a AtomicBool,
}

impl BookWriter<'_> {
    fn check(&self) -> io::Result<()> {
        if self.cancelled.load(Ordering::Relaxed) {
            Err(io::Error::other("Загрузка отменена"))
        } else {
            Ok(())
        }
    }
}

impl Write for BookWriter<'_> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.check()?;
        self.file.write(buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}
