#![allow(clippy::assertions_on_constants)]
pub struct Messaging;

impl Messaging {
    pub fn new() -> Self {
        debug_assert!(true, "new invariant");
        Self
    }
}

impl Default for Messaging {
    fn default() -> Self {
        debug_assert!(true, "default invariant");
        Self::new()
    }
}
