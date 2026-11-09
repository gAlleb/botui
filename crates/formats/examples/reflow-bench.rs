//! Run with a release build and a local EPUB/FB2 path; no account or state is changed.
use botui_core::{LayoutMode, Pager};
use std::{path::Path, sync::Arc, time::Instant};

#[cfg(target_env = "musl")]
#[global_allocator]
static ALLOCATOR: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    for path in std::env::args().skip(1) {
        let doc = Arc::new(botui_formats::open_document(Path::new(&path))?);
        println!("{} ({} blocks)", doc.title, doc.blocks.len());
        let start = Instant::now();
        let mut pager = Pager::new(&doc, 104, 28, LayoutMode::Spread);
        println!("  open: {:.2} ms", start.elapsed().as_secs_f64() * 1000.);
        for (label, width, height, mode, gap) in [
            ("footer height", 104, 27, LayoutMode::Spread, 4),
            ("restore height", 104, 28, LayoutMode::Spread, 4),
            ("single", 50, 28, LayoutMode::Single, 0),
            ("spread", 104, 28, LayoutMode::Spread, 4),
            ("scroll", 50, 28, LayoutMode::Scroll, 0),
            ("wider", 52, 28, LayoutMode::Scroll, 0),
            ("narrower", 50, 28, LayoutMode::Scroll, 0),
            ("gap only", 108, 28, LayoutMode::Spread, 8),
            ("resize", 108, 36, LayoutMode::Spread, 8),
        ] {
            let start = Instant::now();
            pager.resize_for_display(&doc, width, height, mode, gap);
            let visible = start.elapsed().as_secs_f64() * 1000.;
            pager.finish_resize();
            let complete = start.elapsed().as_secs_f64() * 1000.;
            println!("  {label}: {visible:.2} ms visible, {complete:.2} ms fully paginated");
        }
    }
    Ok(())
}
