use std::f64::consts::LN_2;
use std::hash::Hash;

use crate::{hash, mix};

/// A Bloom filter: `k` bits set per item, out of `m`. It can't delete, since a bit may belong to
/// several items.
#[derive(Clone, Debug)]
pub struct Bloom {
    words: Vec<u64>,
    m: u64,
    k: u32,
    len: usize,
}

impl Bloom {
    /// Sized for `n` items at false positive rate `p`, with the optimal number of hashes:
    /// m = -n·ln(p) / ln(2)², k = (m/n)·ln(2).
    pub fn new(n: usize, p: f64) -> Bloom {
        assert!(
            p > 0.0 && p < 1.0,
            "the false positive rate must be between 0 and 1"
        );
        let n = n.max(1) as f64;
        let m = (-n * p.ln() / (LN_2 * LN_2)).ceil().max(64.0) as u64;
        let k = ((m as f64 / n) * LN_2).round().max(1.0) as u32;
        Bloom::with_size(m, k)
    }

    /// `m` bits and `k` hashes, chosen yourself.
    pub fn with_size(m: u64, k: u32) -> Bloom {
        assert!(m > 0 && k > 0);
        Bloom {
            words: vec![0; m.div_ceil(64) as usize],
            m,
            k,
            len: 0,
        }
    }

    /// Double hashing: probe `i` is `h1 + i·h2`, which behaves like `k` independent hashes
    /// (Kirsch and Mitzenmacher, 2006).
    fn probes<T: Hash + ?Sized>(&self, item: &T) -> impl Iterator<Item = u64> + use<T> {
        let h1 = hash(item);
        let h2 = mix(h1) | 1;
        let (m, k) = (self.m, self.k as u64);
        (0..k).map(move |i| h1.wrapping_add(i.wrapping_mul(h2)) % m)
    }

    pub fn insert<T: Hash + ?Sized>(&mut self, item: &T) {
        for bit in self.probes(item) {
            self.words[(bit / 64) as usize] |= 1 << (bit % 64);
        }
        self.len += 1;
    }

    pub fn contains<T: Hash + ?Sized>(&self, item: &T) -> bool {
        self.probes(item)
            .all(|bit| self.words[(bit / 64) as usize] & (1 << (bit % 64)) != 0)
    }

    /// How many items were inserted, counting repeats.
    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn bits(&self) -> u64 {
        self.m
    }

    pub fn hashes(&self) -> u32 {
        self.k
    }

    /// The rate the theory predicts at the current size: (1 - e^(-kn/m))^k.
    pub fn expected_fpr(&self) -> f64 {
        let (k, n, m) = (self.k as f64, self.len as f64, self.m as f64);
        (1.0 - (-k * n / m).exp()).powf(k)
    }
}
