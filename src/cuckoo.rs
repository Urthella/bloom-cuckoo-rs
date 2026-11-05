use std::fmt;
use std::hash::Hash;

use crate::{hash, mix};

const SLOTS: usize = 4;
const MAX_KICKS: usize = 500;

/// The filter has no room left. Delete something first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Full;

impl fmt::Display for Full {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("the cuckoo filter is full")
    }
}

impl std::error::Error for Full {}

/// A cuckoo filter (Fan, Andersen, Kaminsky and Mitzenmacher, 2014): a fingerprint of each item
/// in one of two buckets of four. Unlike a Bloom filter it can delete, and at low false positive
/// rates it takes less space, but it can fill up.
#[derive(Clone, Debug)]
pub struct Cuckoo {
    /// `SLOTS` fingerprints per bucket. 0 means empty, so fingerprints are never 0.
    slots: Vec<u16>,
    buckets: usize,
    fp_bits: u32,
    len: usize,
    /// An item that was kicked out with nowhere to go. It's kept here so it's never lost, and
    /// the filter counts as full until something is deleted.
    victim: Option<(usize, u16)>,
    rng: u64,
}

impl Cuckoo {
    /// Room for `capacity` items with `fp_bits`-bit fingerprints (1 to 16). The table gets a
    /// power-of-two number of buckets, with some slack, since inserts start failing at about
    /// 95% full.
    pub fn new(capacity: usize, fp_bits: u32) -> Cuckoo {
        assert!((1..=16).contains(&fp_bits), "fingerprints are 1 to 16 bits");
        let buckets = ((capacity as f64 / SLOTS as f64 / 0.95).ceil() as usize)
            .next_power_of_two()
            .max(1);
        Cuckoo {
            slots: vec![0; buckets * SLOTS],
            buckets,
            fp_bits,
            len: 0,
            victim: None,
            rng: 0x9e3779b97f4a7c15,
        }
    }

    /// The fingerprint and first bucket. They come from different bits of the hash.
    fn locate<T: Hash + ?Sized>(&self, item: &T) -> (u16, usize) {
        let h = hash(item);
        let fp = ((h >> 32) & ((1 << self.fp_bits) - 1)) as u16;
        (fp.max(1), h as usize & (self.buckets - 1))
    }

    /// Partial-key cuckoo hashing: the other bucket depends only on this one and the
    /// fingerprint, so an item can be moved without knowing what it was. XOR makes it symmetric.
    fn alternate(&self, bucket: usize, fp: u16) -> usize {
        (bucket ^ mix(fp as u64) as usize) & (self.buckets - 1)
    }

    fn bucket(&self, i: usize) -> &[u16] {
        &self.slots[i * SLOTS..(i + 1) * SLOTS]
    }

    fn put(&mut self, i: usize, fp: u16) -> bool {
        match self.slots[i * SLOTS..(i + 1) * SLOTS]
            .iter_mut()
            .find(|s| **s == 0)
        {
            Some(slot) => {
                *slot = fp;
                true
            }
            None => false,
        }
    }

    fn random(&mut self) -> u64 {
        self.rng = self.rng.wrapping_add(0x9e3779b97f4a7c15);
        mix(self.rng)
    }

    /// Adds an item. A full filter refuses it. Inserting the same item twice keeps two copies,
    /// so it can be deleted twice; at most eight copies fit.
    pub fn insert<T: Hash + ?Sized>(&mut self, item: &T) -> Result<(), Full> {
        if self.victim.is_some() {
            return Err(Full);
        }
        let (fp, i1) = self.locate(item);
        self.place(i1, fp);
        Ok(())
    }

    /// Puts a fingerprint in bucket `i` or its alternate, moving others around if both are full.
    fn place(&mut self, i: usize, fp: u16) {
        self.len += 1;
        let alt = self.alternate(i, fp);
        if self.put(i, fp) || self.put(alt, fp) {
            return;
        }
        let (mut i, mut fp) = (if self.random() & 1 == 0 { i } else { alt }, fp);
        for _ in 0..MAX_KICKS {
            let slot = i * SLOTS + (self.random() % SLOTS as u64) as usize;
            std::mem::swap(&mut fp, &mut self.slots[slot]);
            i = self.alternate(i, fp);
            if self.put(i, fp) {
                return;
            }
        }
        self.victim = Some((i, fp));
    }

    pub fn contains<T: Hash + ?Sized>(&self, item: &T) -> bool {
        let (fp, i1) = self.locate(item);
        let i2 = self.alternate(i1, fp);
        self.bucket(i1).contains(&fp)
            || self.bucket(i2).contains(&fp)
            || self
                .victim
                .is_some_and(|(i, v)| v == fp && (i == i1 || i == i2))
    }

    /// Deletes one copy of an item. Only delete what you inserted: deleting something else can
    /// remove another item with the same fingerprint, and then that item is lost.
    pub fn remove<T: Hash + ?Sized>(&mut self, item: &T) -> bool {
        let (fp, i1) = self.locate(item);
        let i2 = self.alternate(i1, fp);
        if let Some((i, v)) = self.victim
            && v == fp
            && (i == i1 || i == i2)
        {
            self.victim = None;
            self.len -= 1;
            return true;
        }
        for i in [i1, i2] {
            if let Some(slot) = self.slots[i * SLOTS..(i + 1) * SLOTS]
                .iter_mut()
                .find(|s| **s == fp)
            {
                *slot = 0;
                self.len -= 1;
                // There's room now for the item that had nowhere to go.
                if let Some((i, v)) = self.victim.take() {
                    self.len -= 1;
                    self.place(i, v);
                }
                return true;
            }
        }
        false
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// How full the table is, from 0 to 1.
    pub fn load(&self) -> f64 {
        self.len as f64 / self.slots.len() as f64
    }

    /// The space the fingerprints take, which is what the paper counts. This implementation
    /// stores each in 16 bits to keep it simple.
    pub fn bits(&self) -> u64 {
        (self.slots.len() as u64) * self.fp_bits as u64
    }

    /// A lookup compares against up to 8 fingerprints, each matching by chance with probability
    /// 1/(2^f - 1): about 8·load/(2^f - 1).
    pub fn expected_fpr(&self) -> f64 {
        let compared = 2.0 * SLOTS as f64 * self.load();
        1.0 - (1.0 - 1.0 / ((1u64 << self.fp_bits) - 1) as f64).powf(compared)
    }
}
