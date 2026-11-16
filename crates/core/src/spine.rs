use std::{ops::Range, sync::Arc};

/// Exclusive block and reference ends in OPF spine order. Empty and non-linear
/// items keep their place even though they contribute no main-reader content.
#[derive(Clone, Debug, Default)]
pub struct EpubSpine {
    ends: Arc<[(usize, usize)]>,
}

pub struct SpinePosition {
    pub ordinal: usize,
    pub total: usize,
    pub blocks: Range<usize>,
}

impl EpubSpine {
    /// The parser supplies one (block count, reference count) pair per itemref.
    pub fn from_counts(mut counts: Vec<(usize, usize)>) -> Self {
        let (mut blocks, mut references) = (0, 0);
        for (n, r) in &mut counts {
            blocks += *n;
            references += *r;
            (*n, *r) = (blocks, references);
        }
        Self {
            ends: counts.into(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.ends.is_empty()
    }

    pub fn position_at_block(&self, block: usize) -> Option<SpinePosition> {
        self.position(self.ends.partition_point(|&(end, _)| end <= block))
    }

    pub(crate) fn position_at_reference(&self, reference: usize) -> Option<SpinePosition> {
        self.position(self.ends.partition_point(|&(_, end)| end <= reference))
    }

    fn position(&self, index: usize) -> Option<SpinePosition> {
        let &(end, _) = self.ends.get(index)?;
        let start = self.ends[..index].last().map_or(0, |item| item.0);
        Some(SpinePosition {
            ordinal: index + 1,
            total: self.ends.len(),
            blocks: start..end,
        })
    }
}
