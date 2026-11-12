//! Keep the Poppler GI document open across page turns on desktops without pdftoppm.
use super::*;
use std::path::PathBuf;
pub struct PageReader {
    path: PathBuf,
    kind: FileKind,
    plates: Option<std::sync::Arc<[String]>>,
    #[cfg(unix)]
    streaming: bool,
    #[cfg(unix)]
    session: Option<unix::Session>,
}
impl PageReader {
    pub fn new(path: &Path, kind: FileKind) -> Self {
        Self {
            path: path.to_owned(),
            kind,
            plates: None,
            #[cfg(unix)]
            streaming: kind == FileKind::Pdf
                && !std::env::var_os("PATH").is_some_and(|p| {
                    std::env::split_paths(&p).any(|d| d.join("pdftoppm").is_file())
                }),
            #[cfg(unix)]
            session: None,
        }
    }
    pub fn images(path: &Path, members: std::sync::Arc<[String]>) -> Self {
        Self {
            plates: Some(members),
            ..Self::new(path, FileKind::Epub)
        }
    }
    pub fn render(&mut self, n: u32, side: u16, cancel: &AtomicBool) -> io::Result<DynamicImage> {
        if let Some(plates) = &self.plates {
            let member = n
                .checked_sub(1)
                .and_then(|index| plates.get(index as usize))
                .ok_or_else(|| io::Error::other("Page outside image reading order"))?;
            let bytes = botui_formats::illustration(&self.path, member)
                .map_err(|e| io::Error::other(e.to_string()))?;
            let mut decoder =
                image::ImageReader::new(io::Cursor::new(bytes)).with_guessed_format()?;
            if decoder.format().is_none() {
                let side = u32::from(side.clamp(200, 1920));
                return svg::render(decoder.into_inner().get_ref(), side, side)
                    .ok_or_else(|| io::Error::other("Cannot render EPUB SVG page"));
            }
            let mut limits = image::Limits::default();
            limits.max_alloc = Some(64 << 20);
            limits.max_image_width = Some(8192);
            limits.max_image_height = Some(8192);
            decoder.limits(limits);
            let page = decoder.decode().map_err(io::Error::other)?;
            let side = u32::from(side.clamp(200, 1920));
            return Ok(if page.width().max(page.height()) > side {
                page.thumbnail(side, side)
            } else {
                page
            });
        }
        #[cfg(unix)]
        if self.streaming {
            if self.session.is_none() {
                self.session = Some(unix::Session::open(&self.path)?);
            }
            let result = self
                .session
                .as_mut()
                .unwrap()
                .render(n, side.clamp(200, 1920), cancel);
            if result.is_err() {
                self.session = None;
            } // A cancelled response must not reach a later request.
            return result;
        }
        page(&self.path, self.kind, n, side, cancel)
    }
}
#[cfg(unix)]
mod unix {
    use super::*;
    use std::{
        io::Write,
        os::{fd::OwnedFd, unix::net::UnixStream},
        process::Child,
    };
    pub(super) struct Session {
        child: Child,
        wire: UnixStream,
    }
    impl Session {
        pub fn open(path: &Path) -> io::Result<Self> {
            let (wire, peer) = UnixStream::pair()?;
            wire.set_read_timeout(Some(Duration::from_millis(25)))?;
            wire.set_write_timeout(Some(Duration::from_secs(1)))?;
            let child = Command::new("python3")
                .args(["-I", "-c", include_str!("poppler.py")])
                .arg(path.canonicalize()?)
                .arg("stream")
                .stdin(OwnedFd::from(peer.try_clone()?))
                .stdout(OwnedFd::from(peer))
                .stderr(Stdio::null())
                .spawn()?;
            Ok(Self { child, wire })
        }
        fn receive(
            &mut self,
            mut bytes: &mut [u8],
            cancel: &AtomicBool,
            deadline: Instant,
        ) -> io::Result<()> {
            while !bytes.is_empty() {
                if cancel.load(Ordering::Acquire) || Instant::now() >= deadline {
                    return Err(io::Error::other("Page rendering interrupted"));
                }
                match self.wire.read(bytes) {
                    Ok(0) => {
                        return Err(io::Error::other(
                            "Poppler closed the document. Check the PDF or install poppler-utils.",
                        ));
                    }
                    Ok(n) => bytes = &mut bytes[n..],
                    Err(e)
                        if matches!(
                            e.kind(),
                            io::ErrorKind::TimedOut
                                | io::ErrorKind::WouldBlock
                                | io::ErrorKind::Interrupted
                        ) => {}
                    Err(e) => return Err(e),
                }
            }
            Ok(())
        }
        pub fn render(
            &mut self,
            n: u32,
            side: u16,
            cancel: &AtomicBool,
        ) -> io::Result<DynamicImage> {
            writeln!(self.wire, "{n} {side}")?;
            let deadline = Instant::now() + Duration::from_secs(25);
            let mut header = [0; 8];
            self.receive(&mut header, cancel, deadline)?;
            let w = u32::from_be_bytes(header[..4].try_into().unwrap());
            let h = u32::from_be_bytes(header[4..].try_into().unwrap());
            if w.min(h) == 0 || w.max(h) > u32::from(side) {
                return Err(io::Error::other("Invalid Poppler page dimensions"));
            }
            let mut data = vec![0; (w * h * 4) as usize];
            self.receive(&mut data, cancel, deadline)?;
            // Cairo RGB24 is a native-endian 0x00RRGGBB word.
            for p in data.as_chunks_mut::<4>().0 {
                let [_, r, g, b] = u32::from_ne_bytes(*p).to_be_bytes();
                p.copy_from_slice(&[r, g, b, 255]);
            }
            Ok(DynamicImage::ImageRgba8(
                image::RgbaImage::from_raw(w, h, data).unwrap(),
            ))
        }
    }
    impl Drop for Session {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}
