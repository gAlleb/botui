use crate::Anchor;
#[derive(Clone, Debug)]
pub struct Reference {
    pub source: usize,
    /// Visible label in the normalized source block. Image-only links have no text range.
    pub bytes: Option<std::ops::Range<usize>>,
    pub backlink: bool,
    pub label: String,
    pub target: Target,
}
#[derive(Clone, Debug)]
pub enum Target {
    Passage(Anchor),
    Note(String),
    /// A non-spine XHTML resource; load its named fragment only when requested.
    Supplement {
        member: String,
        fragment: String,
        /// Source XHTML member and element id when the publisher supplied a return address.
        origin: Option<(std::sync::Arc<str>, String)>,
    },
    Image(String),
    Web(String),
}
