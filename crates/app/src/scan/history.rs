//! Explicit jumps can be undone without recording every turn of the mouse wheel.
use crate::cover::Crop;
use std::collections::VecDeque;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct Location {
    pub page: u32,
    pub offset: u16,
    pub crop: Crop,
}

#[derive(Default)]
pub(super) struct History {
    back: VecDeque<Location>,
    forward: Vec<Location>,
}

impl History {
    pub fn record(&mut self, from: Location, to: Location) {
        if from == to {
            return;
        }
        self.forward.clear();
        if self.back.back() != Some(&from) {
            if self.back.len() == 100 {
                self.back.pop_front();
            }
            self.back.push_back(from);
        }
    }

    pub fn back(&mut self, from: Location) -> Option<Location> {
        let target = self.back.pop_back()?;
        self.forward.push(from);
        Some(target)
    }

    pub fn forward(&mut self, from: Location) -> Option<Location> {
        let target = self.forward.pop()?;
        self.back.push_back(from);
        Some(target)
    }
}
