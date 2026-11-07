//! Give the desktop a local image with its real MIME type, never a catalog URL.
use crate::shelf::background::{Ticket, Worker};
use botui_bookorbit::Result;
use image::ImageFormat;
use sha2::{Digest, Sha256};
use std::{
    io::{self, Cursor, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        atomic::AtomicBool,
        mpsc::{self, Receiver, TryRecvError},
    },
    time::{Duration, Instant},
};

pub(crate) fn png(image: image::DynamicImage) -> Result<Vec<u8>> {
    let mut out = Cursor::new(Vec::new());
    image.write_to(&mut out, ImageFormat::Png)?;
    Ok(out.into_inner())
}

#[derive(Default)]
pub(crate) struct Viewer {
    pending: Option<Ticket<Result<PathBuf>>>,
    handler: Option<(Instant, Receiver<io::Result<std::process::ExitStatus>>)>,
}
impl Viewer {
    pub fn busy(&self) -> bool {
        self.pending.is_some()
            || self
                .handler
                .as_ref()
                .is_some_and(|(at, _)| at.elapsed() < Duration::from_secs(1))
    }
    pub fn document(&mut self, path: &Path, kind: botui_formats::FileKind) {
        use botui_formats::FileKind::*;
        let path = path.to_owned();
        self.start(move |cancel| match kind {
            Pdf | Djvu => png(botui_fixed::page(&path, kind, 1, 1440, cancel)?),
            _ => {
                if let Some(bytes) = botui_formats::cover(&path).map_err(|e| e.to_string())? {
                    return Ok(bytes);
                }
                let meta = botui_formats::book_metadata(&path).map_err(|e| e.to_string())?;
                let (title, authors) = crate::catalog::local_credits(&path, Some(&meta));
                crate::shelf::gallery::binding::Binding::new(
                    &title,
                    &authors,
                    meta.series.first().map(String::as_str),
                    None,
                )
                .png()
            }
        });
    }
    pub fn start(&mut self, get: impl FnOnce(&AtomicBool) -> Result<Vec<u8>> + Send + 'static) {
        if self.pending.is_some() {
            return;
        }
        self.pending = Some(Worker::new().start(move |cancel| {
            let data = get(cancel)?;
            let ext = match image::guess_format(&data)? {
                ImageFormat::Jpeg => "jpg",
                ImageFormat::Png => "png",
                ImageFormat::WebP => "webp",
                ImageFormat::Gif => "gif",
                ImageFormat::Tiff => "tiff",
                ImageFormat::Bmp => "bmp",
                ImageFormat::Avif => "avif",
                _ => return Err("Unsupported cover image format.".into()),
            };
            let dir = crate::cache::cache_dir()?.join("covers");
            std::fs::create_dir_all(&dir)?;
            let path = dir.join(format!("{:x}.{ext}", Sha256::digest(&data)));
            if !path.exists() {
                let mut file = tempfile::NamedTempFile::new_in(&dir)?;
                file.write_all(&data)?;
                if let Err(e) = file.persist_noclobber(&path)
                    && e.error.kind() != io::ErrorKind::AlreadyExists
                {
                    return Err(e.error.into());
                }
            }
            Ok(path)
        }));
    }
    fn launch(&mut self, path: &Path) -> io::Result<()> {
        let mut child = Command::new("xdg-open")
            .arg(path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        // Some desktop handlers wait until the image window closes. Reap them without
        // tying the lifetime of the reader, or its redraw rate, to that window.
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(child.wait());
        });
        self.handler = Some((Instant::now(), rx));
        Ok(())
    }
    pub fn collect(&mut self) -> Option<String> {
        if let Some(ticket) = &self.pending {
            match ticket.receiver.try_recv() {
                Ok(result) => {
                    self.pending = None;
                    if let Err(e) = result.and_then(|p| self.launch(&p).map_err(Into::into)) {
                        return Some(format!("View cover: {e}"));
                    }
                }
                Err(TryRecvError::Disconnected) => {
                    self.pending = None;
                    return Some("Cover retrieval stopped unexpectedly.".into());
                }
                Err(TryRecvError::Empty) => {}
            }
        }
        let result = self.handler.as_ref()?.1.try_recv();
        if matches!(result, Err(TryRecvError::Empty)) {
            return None;
        }
        self.handler = None;
        if matches!(result, Ok(Ok(status)) if status.success()) {
            None
        } else {
            Some("The system image viewer could not open this cover.".into())
        }
    }
}
