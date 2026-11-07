//! Print a few candidate locations, or resolve a JSON array of CFIs from stdin.
use botui_core::Anchor;
use std::{env, path::Path};
fn main() {
    let mut args = env::args().skip(1);
    let file = args.next().unwrap();
    let doc = botui_formats::open_document(Path::new(&file)).unwrap();
    eprintln!(
        "Карта: {} из {} блоков",
        doc.locations.0.len(),
        doc.blocks.len()
    );
    if args.next().as_deref() == Some("--resolve") {
        let addresses: Vec<String> = serde_json::from_reader(std::io::stdin()).unwrap();
        let positions: Vec<_> = addresses
            .iter()
            .map(|c| doc.resolve_cfi(c).map(|a| (a.block, a.byte)))
            .collect();
        println!("{}", serde_json::json!(positions));
        return;
    }
    let mut out = vec![];
    let step = (doc.blocks.len() / 10).max(1);
    for i in (0..doc.blocks.len()).step_by(step) {
        let Some((block, b)) = doc
            .blocks
            .iter()
            .enumerate()
            .skip(i)
            .find(|(j, b)| b.text.chars().count() > 32 && doc.locations.0.contains_key(j))
        else {
            continue;
        };
        let byte = b.text.char_indices().nth(7).unwrap().0;
        let a = Anchor { block, byte };
        if let Some(cfi) = doc.cfi(a) {
            assert_eq!(doc.resolve_cfi(&cfi), Some(a));
            out.push(serde_json::json!({"block":block,"byte":byte,"cfi":cfi,"fragment":b.text[byte..].chars().take(8).collect::<String>()}));
        }
    }
    println!("{}", serde_json::json!(out));
}
