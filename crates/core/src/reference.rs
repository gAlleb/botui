use crate::Anchor;
#[derive(Clone, Debug)]
pub struct Reference {
    pub source: usize,
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
    },
    Image(String),
    Web(String),
}
