//! Bloom and cuckoo filters, small enough to read, with the false positive rates the theory
//! predicts. Both answer "might this be in the set?" in far less space than the set itself: "no"
//! is always right, and "yes" is wrong at a rate you choose.

mod bloom;
mod cuckoo;

pub use bloom::Bloom;
pub use cuckoo::{Cuckoo, Full};

use std::hash::{DefaultHasher, Hash, Hasher};

/// One 64-bit hash per item. `DefaultHasher::new()` uses fixed keys, so it's the same in every run.
fn hash<T: Hash + ?Sized>(item: &T) -> u64 {
    let mut hasher = DefaultHasher::new();
    item.hash(&mut hasher);
    hasher.finish()
}

/// The splitmix64 finalizer, to get a second, unrelated value from a hash.
fn mix(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
    z ^ (z >> 31)
}
