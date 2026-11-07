//! Download a complete edition from a reader into a caller-owned staging file.
use std::{
    io::{self, Read, Write},
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
};

pub const MIB: u64 = 1 << 20;

/// Zero is an explicit request for no size limit. Catalog and HTTP sizes are
/// early hints. The same check is made on the actual bytes before writing them.
pub fn check_size(size: u64, limit: u64) -> io::Result<()> {
    if limit != 0 && size > limit {
        let amount = if limit.is_multiple_of(MIB) {
            format!("{} MiB", limit / MIB)
        } else {
            format!("{limit} bytes")
        };
        return Err(io::Error::other(format!(
            "Book exceeds the {amount} download limit. Change max_download_mib in Settings → Library."
        )));
    }
    Ok(())
}

pub fn check_cancel(cancel: &AtomicBool) -> io::Result<()> {
    if cancel.load(Ordering::Acquire) {
        return Err(io::Error::other("Download cancelled"));
    }
    Ok(())
}

/// A single download's worker publishes counters that a UI can poll without blocking.
#[derive(Default)]
pub struct Progress {
    received: AtomicU64,
    total: AtomicU64,
    complete: AtomicBool,
}

impl Progress {
    pub fn new(advertised: Option<u64>) -> Self {
        Self {
            total: AtomicU64::new(advertised.unwrap_or(0)),
            ..Self::default()
        }
    }

    pub fn snapshot(&self) -> (u64, Option<u64>, bool) {
        let complete = self.complete.load(Ordering::Acquire);
        let total = self.total.load(Ordering::Relaxed);
        (
            self.received.load(Ordering::Relaxed),
            (total > 0).then_some(total),
            complete,
        )
    }
}

pub fn stream(
    mut source: impl Read,
    mut destination: impl Write,
    expected: Option<u64>,
    limit: u64,
    cancel: &AtomicBool,
    progress: &Progress,
) -> io::Result<u64> {
    if let Some(size) = expected {
        check_size(size, limit)?;
        progress.total.store(size, Ordering::Relaxed);
    }
    let advertised = progress.total.load(Ordering::Relaxed);
    let mut count = 0u64;
    let mut buf = [0; 64 * 1024];
    loop {
        check_cancel(cancel)?;
        let n = match source.read(&mut buf) {
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            result => result?,
        };
        check_cancel(cancel)?;
        if n == 0 {
            break;
        }
        count = count
            .checked_add(n as u64)
            .ok_or_else(|| io::Error::other("File length overflow"))?;
        check_size(count, limit)?;
        destination.write_all(&buf[..n]).map_err(write_error)?;
        // An edition may have changed since the catalog was indexed.
        if expected.is_none() && advertised > 0 && count > advertised {
            progress.total.store(0, Ordering::Relaxed);
        }
        progress.received.store(count, Ordering::Relaxed);
    }
    if count == 0 || expected.is_some_and(|n| n != count) {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "Incomplete book download",
        ));
    }
    destination.flush().map_err(write_error)?;
    check_cancel(cancel)?;
    if advertised > 0 {
        progress.total.store(count, Ordering::Relaxed);
    }
    progress.complete.store(true, Ordering::Release);
    Ok(count)
}

fn write_error(err: io::Error) -> io::Error {
    io::Error::new(
        err.kind(),
        format!("Could not write the downloaded book to disk: {err}"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fits_exactly_without_a_length_header_but_rejects_one_extra_byte() {
        let cancel = AtomicBool::new(false);
        let mut file = Vec::new();
        assert_eq!(
            stream(
                &b"12345"[..],
                &mut file,
                None,
                5,
                &cancel,
                &Progress::default()
            )
            .unwrap(),
            5
        );
        file.clear();
        assert!(
            stream(
                &b"123456"[..],
                &mut file,
                None,
                5,
                &cancel,
                &Progress::default()
            )
            .is_err()
        );
        assert!(file.len() <= 5);
        assert!(stream(&b""[..], io::sink(), None, 0, &cancel, &Progress::default()).is_err());
        assert!(
            stream(
                &b"123"[..],
                io::sink(),
                Some(5),
                0,
                &cancel,
                &Progress::default()
            )
            .is_err()
        );
    }

    #[test]
    fn response_size_replaces_the_catalog_hint_and_reports_only_written_bytes() {
        let cancel = AtomicBool::new(false);
        let progress = Progress::new(Some(1024));
        stream(&b"123"[..], io::sink(), Some(3), 5, &cancel, &progress).unwrap();
        assert_eq!(progress.snapshot(), (3, Some(3), true));

        let partial = Progress::default();
        assert!(stream(&b"123"[..], io::sink(), Some(4), 5, &cancel, &partial).is_err());
        assert_eq!(partial.snapshot(), (3, Some(4), false));

        let unknown = Progress::default();
        stream(&b"123"[..], io::sink(), None, 5, &cancel, &unknown).unwrap();
        assert_eq!(unknown.snapshot(), (3, None, true));
    }

    #[test]
    fn cancellation_during_a_chunk_leaves_staging_unpublished() {
        let cancel = AtomicBool::new(false);
        let mut count = 0;
        struct Pause<'a>(&'a AtomicBool, &'a mut usize);
        impl Write for Pause<'_> {
            fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
                *self.1 += buf.len();
                self.0.store(true, Ordering::Release);
                Ok(buf.len())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let result = stream(
            io::repeat(0).take(1 << 20),
            Pause(&cancel, &mut count),
            None,
            0,
            &cancel,
            &Progress::default(),
        );
        assert!(result.unwrap_err().to_string().contains("cancelled"));
        assert!(count > 0 && count < 1 << 20);
    }

    #[test]
    fn disk_failures_are_reported_as_writes_without_claiming_success() {
        struct Full;
        impl Write for Full {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                Err(io::Error::from(io::ErrorKind::StorageFull))
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let err = stream(
            &b"zip"[..],
            Full,
            None,
            0,
            &AtomicBool::new(false),
            &Progress::default(),
        )
        .unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::StorageFull);
        assert!(err.to_string().contains("disk"));
    }
}
