#![allow(clippy::assertions_on_constants)]
pub struct Messaging;

impl Messaging {
    pub fn new() -> Self {
        Self
    }
}

impl Default for Messaging {
    fn default() -> Self {
        Self::new()
    }
}
