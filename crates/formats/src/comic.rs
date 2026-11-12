//! Comic archives reuse the image-page reader and existing ZIP decoder.
//! RAR decoding is built in; no external program or full archive extraction.
use super::{Result, invalid};
use botui_core::{Anchor, ContentsEntry, Document};
use std::{
    cmp::Ordering,
    fs::File,
    io::{BufReader, Read},
    path::Path,
};
const IMAGE_LIMIT: u64 = 16 << 20;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ArchiveKind {
    Zip,
    Rar,
}
impl ArchiveKind {
    // Catalogs and local collections sometimes name RAR files .cbz (or ZIP
    // files .cbr). Use the same signature check for the index, cover and pages.
    fn detect(path: &Path) -> Result<Self> {
        let mut header = Vec::with_capacity(8);
        File::open(path)?.take(8).read_to_end(&mut header)?;
        if header.starts_with(b"Rar!\x1a\x07\x00") || header.starts_with(b"Rar!\x1a\x07\x01\x00") {
            Ok(Self::Rar)
        } else if [b"PK\x03\x04", b"PK\x05\x06", b"PK\x07\x08"]
            .iter()
            .any(|magic| header.starts_with(*magic))
        {
            Ok(Self::Zip)
        } else {
            Err(invalid("Comic archive is not a supported ZIP or RAR file").into())
        }
    }
}

fn page_name(name: &str) -> bool {
    !name.chars().any(char::is_control)
        && !name
            .split(['/', '\\'])
            .any(|p| p.starts_with('.') || p == "__MACOSX")
        && Path::new(name)
            .extension()
            .and_then(|s| s.to_str())
            .is_some_and(|ext| {
                ["jpg", "jpeg", "png", "webp"]
                    .iter()
                    .any(|v| ext.eq_ignore_ascii_case(v))
            })
}
fn natural(a: &str, b: &str) -> Ordering {
    let (mut x, mut y) = (a.as_bytes(), b.as_bytes());
    while !x.is_empty() && !y.is_empty() {
        let order;
        if x[0].is_ascii_digit() && y[0].is_ascii_digit() {
            let nx = x.iter().take_while(|c| c.is_ascii_digit()).count();
            let ny = y.iter().take_while(|c| c.is_ascii_digit()).count();
            let ax = x[..nx].iter().position(|c| *c != b'0').unwrap_or(nx);
            let by = y[..ny].iter().position(|c| *c != b'0').unwrap_or(ny);
            order = (nx - ax)
                .cmp(&(ny - by))
                .then_with(|| x[ax..nx].cmp(&y[by..ny]));
            x = &x[nx..];
            y = &y[ny..];
        } else {
            order = x[0].to_ascii_lowercase().cmp(&y[0].to_ascii_lowercase());
            x = &x[1..];
            y = &y[1..];
        }
        if order != Ordering::Equal {
            return order;
        }
    }
    x.len().cmp(&y.len()).then_with(|| a.cmp(b))
}
pub(super) fn open(path: &Path) -> Result<Document> {
    let mut pages: Vec<(String, String)> = if ArchiveKind::detect(path)? == ArchiveKind::Zip {
        let zip = zip::ZipArchive::new(BufReader::new(File::open(path)?))?;
        let mut names = Vec::new();
        for name in zip.file_names() {
            let name = name?;
            if page_name(&name) {
                let name = name.into_owned();
                names.push((name.clone(), name));
            }
        }
        names
    } else {
        let archive = rar_archive(path)?;
        let mut names = Vec::new();
        for (index, entry) in archive.members().enumerate() {
            let meta = entry.meta;
            let name = meta.name_lossy();
            if !meta.is_directory && !meta.is_redirection && page_name(&name) {
                if meta.is_encrypted {
                    return Err(invalid("Password-protected CBR pages are not supported").into());
                }
                if meta.is_split_before || meta.is_split_after {
                    return Err(invalid(
                        "Multi-volume CBR archives are not supported; use a single archive",
                    )
                    .into());
                }
                names.push((name, format!("rar:{index}")));
            }
        }
        names
    };
    pages.sort_by(|a, b| natural(&a.0, &b.0));
    pages.dedup();
    if pages.is_empty() {
        return Err(invalid("Comic archive contains no JPEG, PNG or WebP pages").into());
    }
    if pages.len() > 50_000 {
        return Err(invalid("Comic archive has too many pages").into());
    }
    Ok(Document {
        title: path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned(),
        contents: pages
            .iter()
            .enumerate()
            .map(|(i, (name, _))| {
                ContentsEntry::passage(Anchor { block: i, byte: 0 }, name.clone())
            })
            .collect(),
        image_pages: pages.into_iter().map(|(_, key)| key).collect(),
        ..Default::default()
    })
}
pub(super) fn image(path: &Path, member: &str) -> Result<Vec<u8>> {
    if ArchiveKind::detect(path)? == ArchiveKind::Rar {
        return rar_image(path, member);
    }
    let mut zip = zip::ZipArchive::new(BufReader::new(File::open(path)?))?;
    let file = zip.by_name(member)?;
    if file.size() > IMAGE_LIMIT {
        return Err(invalid("Comic page exceeds 16 MiB").into());
    }
    let mut bytes = Vec::new();
    file.take(IMAGE_LIMIT + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > IMAGE_LIMIT {
        return Err(invalid("Comic page exceeds 16 MiB").into());
    }
    Ok(bytes)
}
fn rar_options() -> rars::ArchiveReadOptions<'static> {
    let mut options = rars::ArchiveReadOptions::default();
    options.max_header_count = Some(50_000);
    options.max_header_bytes = Some(16 << 20);
    options.max_member_output_bytes = Some(IMAGE_LIMIT);
    options.rar50_dictionary_size_limit = Some(64 << 20);
    options.max_reader_workspace_bytes = Some(128 << 20);
    options
}
fn rar_archive(path: &Path) -> Result<rars::Archive> {
    Ok(rars::ArchiveReader::read_reader_with_options(
        File::open(path)?,
        rar_options(),
    )?)
}
fn rar_image(path: &Path, member: &str) -> Result<Vec<u8>> {
    let index = member
        .strip_prefix("rar:")
        .and_then(|s| s.parse::<usize>().ok())
        .ok_or_else(|| invalid("Invalid CBR page address"))?;
    rar_archive(path)?
        .read_member_at_with_options(index, rar_options())?
        .ok_or_else(|| invalid("Page not found in CBR archive").into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn page_order_is_natural_without_integer_overflow() {
        let mut pages = [
            "10.jpg",
            "2.jpg",
            "01.jpg",
            "01.png",
            "9/10.JPG",
            "9/2.JPG",
            "9999999999999999999999999.jpg",
        ];
        pages.sort_by(|a, b| natural(a, b));
        assert_eq!(
            pages,
            [
                "01.jpg",
                "01.png",
                "2.jpg",
                "9/2.JPG",
                "9/10.JPG",
                "10.jpg",
                "9999999999999999999999999.jpg"
            ]
        );
        assert!(!page_name("__MACOSX/._2.jpg"));
        assert!(!page_name("../2.jpg"));
        assert!(!page_name("ComicInfo.xml"));
        assert!(page_name("Chapter 1/page [02].PNG"));
    }
}

#[cfg(test)]
mod archive_tests {
    use super::*;
    use std::io::Write;
    #[test]
    fn zip_named_cbr_keeps_the_cover_and_every_page() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("actually-zip.CbR");
        let mut zip = zip::ZipWriter::new(File::create(&path).unwrap());
        for (name, bytes) in [
            ("10.png", "ten"),
            ("2.png", "two"),
            ("1.png", "one"),
            ("rar:3.png", "colon"),
        ] {
            zip.start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(bytes.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
        let doc = crate::open_document(&path).unwrap();
        assert_eq!(doc.image_pages, ["1.png", "2.png", "10.png", "rar:3.png"]);
        for (member, expected) in doc.image_pages.iter().zip(["one", "two", "ten", "colon"]) {
            assert_eq!(super::image(&path, member).unwrap(), expected.as_bytes());
        }
        assert_eq!(crate::cover(&path).unwrap().unwrap(), b"one");
    }

    #[test]
    fn rar4_and_rar5_named_cbz_keep_the_cover_and_every_page() {
        for name in ["rar4", "rar4-solid", "rar5", "rar5-solid"] {
            let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join(format!("../../scripts/fixtures/comics/{name}.cbr"));
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("actually-rar.CbZ");
            std::fs::copy(&fixture, &path).unwrap();
            let expected = crate::open_document(&fixture).unwrap();
            let doc = crate::open_document(&path).unwrap();
            assert_eq!(doc.image_pages, expected.image_pages, "{name}");
            assert_eq!(doc.contents.len(), 3);
            for member in &doc.image_pages {
                let image = super::image(&path, member).unwrap();
                assert!(image.starts_with(b"\x89PNG\r\n\x1a\n"));
                assert_eq!(
                    image,
                    super::image(&fixture, member).unwrap(),
                    "{name}: {member}"
                );
            }
            assert_eq!(
                crate::cover(&path).unwrap(),
                crate::cover(&fixture).unwrap()
            );
        }
    }

    #[test]
    fn invalid_or_truncated_comics_fail_without_guessing_from_the_suffix() {
        let dir = tempfile::tempdir().unwrap();
        for suffix in ["cbz", "cbr"] {
            let path = dir.path().join(format!("invalid.{suffix}"));
            for bytes in [b"".as_slice(), b"PK", b"Rar!\x1a\x07", b"not an archive"] {
                std::fs::write(&path, bytes).unwrap();
                let error = crate::open_document(&path).unwrap_err().to_string();
                assert!(error.contains("not a supported ZIP or RAR"), "{error}");
                assert!(super::image(&path, "1.png").is_err());
            }
        }
    }

    #[test]
    fn cbz_pages_cover_and_empty_archive_are_checked() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("book.CBZ");
        let mut zip = zip::ZipWriter::new(File::create(&path).unwrap());
        for (name, bytes) in [
            ("10.png", "ten"),
            ("2.png", "two"),
            ("1.png", "one"),
            ("__MACOSX/._1.png", "junk"),
            ("ComicInfo.xml", "xml"),
        ] {
            zip.start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(bytes.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
        let doc = super::open(&path).unwrap();
        assert_eq!(doc.image_pages, ["1.png", "2.png", "10.png"]);
        assert_eq!(super::image(&path, "2.png").unwrap(), b"two");
        assert_eq!(crate::cover(&path).unwrap().unwrap(), b"one");
        assert!(crate::supports_path(&path));
        zip::ZipWriter::new(File::create(&path).unwrap())
            .finish()
            .unwrap();
        assert!(
            super::open(&path)
                .unwrap_err()
                .to_string()
                .contains("no JPEG")
        );
        std::fs::write(&path, b"not a zip").unwrap();
        assert!(super::open(&path).is_err());
    }
}
