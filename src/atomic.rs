use std::sync::atomic::{AtomicU32, Ordering};

/// A simple wrapper around AtomicU32 that stores f32 values.
pub struct AtomicF32 {
    atomic: AtomicU32,
}

impl AtomicF32 {
    pub fn new(value: f32) -> Self {
        Self {
            atomic: AtomicU32::new(value.to_bits()),
        }
    }

    pub fn load(&self, ordering: Ordering) -> f32 {
        f32::from_bits(self.atomic.load(ordering))
    }

    pub fn store(&self, value: f32, ordering: Ordering) {
        self.atomic.store(value.to_bits(), ordering);
    }

    pub fn swap(&self, value: f32, ordering: Ordering) -> f32 {
        f32::from_bits(self.atomic.swap(value.to_bits(), ordering))
    }
}