//! Export EPUB SVGs through the same decoder as the reader, without opening a terminal.
//! cargo run -p botui --example inspect-svg -- BOOK.epub OUTPUT_DIRECTORY
use std::path::Path;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let path = Path::new(args.get(1).ok_or("Expected EPUB path")?);
    let out = Path::new(args.get(2).ok_or("Expected output directory")?);
    std::fs::create_dir_all(out)?;
    let doc = botui_formats::open_epub(path)?;
    let mut count = 0;
    for (i, reference) in doc.references.iter().enumerate() {
        if let botui_core::Target::Image(member) = &reference.target {
            let bytes = botui_formats::illustration(path, member)?;
            if let Some(image) = botui_fixed::svg::render(&bytes, 1200, 900) {
                image.save(out.join(format!("{i:03}.png")))?;
                println!("{i}: {member} {}×{}", image.width(), image.height());
                count += 1;
            }
        }
    }
    if count == 0 {
        return Err("No rendered SVG illustrations".into());
    }
    println!("{count} SVG illustrations rendered");
    Ok(())
}
