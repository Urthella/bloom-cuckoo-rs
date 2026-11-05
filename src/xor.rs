use std::hash::Hash;

use crate::{hash, mix};

/// An xor filter (Graf and Lemire, 2019): built once from a fixed set, then read only. Each item
/// has three slots, one in each third of the table, and its fingerprint is the XOR of what's in
/// them. That takes about 1.23 · f bits per item, less than a Bloom or cuckoo filter at the same
/// rate, but nothing can be added or deleted afterwards.
#[derive(Clone, Debug)]
pub struct Xor {
    slots: Vec<u16>,
    block: usize,
    seed: u64,
    fp_bits: u32,
}

impl Xor {
    /// Builds a filter of `items` with `fp_bits`-bit fingerprints (1 to 16). Repeats are fine.
    pub fn build<'a, T: Hash + ?Sized + 'a>(
        items: impl IntoIterator<Item = &'a T>,
        fp_bits: u32,
    ) -> Xor {
        assert!((1..=16).contains(&fp_bits), "fingerprints are 1 to 16 bits");
        let mut hashes: Vec<u64> = items.into_iter().map(hash).collect();
        // Two copies of one item would never peel apart.
        hashes.sort_unstable();
        hashes.dedup();
        let block = ((1.23 * hashes.len() as f64).ceil() as usize + 32) / 3;
        let mut filter = Xor {
            slots: vec![0; block * 3],
            block,
            seed: 0,
            fp_bits,
        };
        // Peeling fails now and then, with a few percent chance: try again with another seed.
        for _ in 0..100 {
            filter.seed = mix(filter.seed.wrapping_add(0x9e3779b97f4a7c15));
            if let Some(order) = filter.peel(&hashes) {
                filter.assign(&order);
                return filter;
            }
        }
        panic!("peeling failed 100 times in a row, which shouldn't happen with distinct items");
    }

    fn slots_of(&self, h: u64) -> [usize; 3] {
        let h = mix(h ^ self.seed);
        let reduce = |x: u64| ((x as u32 as u64 * self.block as u64) >> 32) as usize;
        [
            reduce(h),
            self.block + reduce(h.rotate_left(21)),
            2 * self.block + reduce(h.rotate_left(42)),
        ]
    }

    fn fingerprint(&self, h: u64) -> u16 {
        let h = mix(h ^ self.seed);
        ((h ^ (h >> 32)) & ((1 << self.fp_bits) - 1)) as u16
    }

    /// Finds an order to fill the slots in: repeatedly take a slot that only one item uses, and
    /// remove that item from its other two. Returns each item with the slot it owns, or None if
    /// some slots are left that no item has to itself.
    fn peel(&self, hashes: &[u64]) -> Option<Vec<(u64, usize)>> {
        let mut count = vec![0u32; self.slots.len()];
        let mut xored = vec![0u64; self.slots.len()];
        for &h in hashes {
            for s in self.slots_of(h) {
                count[s] += 1;
                xored[s] ^= h;
            }
        }
        let mut alone: Vec<usize> = (0..self.slots.len()).filter(|&s| count[s] == 1).collect();
        let mut order = Vec::with_capacity(hashes.len());
        while let Some(s) = alone.pop() {
            if count[s] != 1 {
                continue;
            }
            // The only item left in this slot is the XOR of everything that was ever in it.
            let h = xored[s];
            order.push((h, s));
            for other in self.slots_of(h) {
                count[other] -= 1;
                xored[other] ^= h;
                if count[other] == 1 {
                    alone.push(other);
                }
            }
        }
        (order.len() == hashes.len()).then_some(order)
    }

    /// Fills slots in the reverse of the peeling order, so each item's owned slot is set last and
    /// can make the three XOR to its fingerprint.
    fn assign(&mut self, order: &[(u64, usize)]) {
        for &(h, owned) in order.iter().rev() {
            let [a, b, c] = self.slots_of(h);
            self.slots[owned] = 0;
            self.slots[owned] = self.fingerprint(h) ^ self.slots[a] ^ self.slots[b] ^ self.slots[c];
        }
    }

    pub fn contains<T: Hash + ?Sized>(&self, item: &T) -> bool {
        let h = hash(item);
        let [a, b, c] = self.slots_of(h);
        self.fingerprint(h) == self.slots[a] ^ self.slots[b] ^ self.slots[c]
    }

    /// The space the fingerprints take. Like the cuckoo filter, this stores each in 16 bits.
    pub fn bits(&self) -> u64 {
        self.slots.len() as u64 * self.fp_bits as u64
    }

    /// A non-member's three slots XOR to its fingerprint by chance: 1 in 2^f.
    pub fn expected_fpr(&self) -> f64 {
        1.0 / (1u64 << self.fp_bits) as f64
    }
}
