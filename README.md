# bloom-cuckoo-rs

[![ci](https://github.com/Urthella/bloom-cuckoo-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/Urthella/bloom-cuckoo-rs/actions/workflows/ci.yml)

A Bloom filter and a cuckoo filter in about 220 lines of Rust with no dependencies, measured
against each other and against the theory. Both answer "might this be in the set?" in far less
space than the set: "no" is always right, and "yes" is wrong at a rate you choose.

```
cargo run --release --example compare
```

985,000 items in each filter, then 2,000,000 lookups of items that aren't there:

| Filter | Bits per item | Expected false positives | Measured | Insert ns | Lookup ns | Deletes |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| Bloom, k=7 | 9.6 | 1.0039% | 1.0089% | 35 | 44 | no |
| Bloom, k=10 | 14.4 | 0.1000% | 0.1020% | 49 | 47 | no |
| Bloom, k=13 | 19.2 | 0.0100% | 0.0104% | 65 | 49 | no |
| Cuckoo, 8-bit fingerprints | 8.5 | 2.9097% | 2.9361% | 55 | 33 | yes |
| Cuckoo, 12-bit fingerprints | 12.8 | 0.1834% | 0.1873% | 56 | 33 | yes |
| Cuckoo, 16-bit fingerprints | 17.0 | 0.0115% | 0.0103% | 57 | 37 | yes |

"Expected" is what the formulas below predict for each filter's actual size, and every measured
rate is close to it. The cuckoo filters are 94% full, the load they really run at. Times are from
one run on a Windows laptop, so read them as rough.

## Which to use

- **A Bloom filter** never fills up and works at any rate, but can't delete. At around 1% it's
  the smaller of the two.
- **A cuckoo filter** can delete, and its lookups are faster, since they read two buckets instead
  of `k` scattered bits. Below about 0.1% it's smaller: at about 0.01%, 17.0 bits per item
  against Bloom's 19.2. But it fills up at about 95%, and deleting an item that was never inserted
  can remove a different one.

## The theory it's checked against

**Bloom:** for `n` items and target rate `p`, `m = -n·ln(p) / ln(2)²` bits and `k = (m/n)·ln(2)`
hashes, which gives a rate of `(1 - e^(-kn/m))^k`. The `k` hashes come from two by double hashing
(Kirsch and Mitzenmacher, 2006).

**Cuckoo** (Fan, Andersen, Kaminsky and Mitzenmacher, 2014): each item is an `f`-bit fingerprint
in one of two buckets of four. The second bucket is the first XOR a hash of the fingerprint, so
an item can be moved without knowing what it was. A lookup compares against up to eight
fingerprints, so the rate is about `8·load / (2^f - 1)`. When both buckets are full, an inserted
item kicks one out to its other bucket, up to 500 times. If that fails, the last one kicked out is
kept aside so nothing is lost, and the filter counts as full until something is deleted.

```rust
use bloom_cuckoo_rs::{Bloom, Cuckoo};

let mut bloom = Bloom::new(1_000_000, 0.01);
bloom.insert("alice");
assert!(bloom.contains("alice"));

let mut cuckoo = Cuckoo::new(1_000_000, 12);
cuckoo.insert("alice")?;
assert!(cuckoo.remove("alice"));
```

## How it's tested

The tests measure false positive rates over 500,000 lookups and require them to be within 15%
of the formulas for each filter's actual size, or 50% for 16-bit fingerprints, where there are
only a few dozen hits. They also check that there are no false negatives, that Bloom sizes itself
by the formulas, that a cuckoo filter fills past 90% before refusing, and that deleting keeps
every other item.

To check that the tests can fail, 13 bugs were planted one at a time, and every one was caught:
among them, an alternate bucket that isn't symmetric, a fingerprint allowed to be 0 (which means
empty), forgetting the item set aside when the table is full, and probes that all hit the same
bit. One survived the first time: after deletes, the item set aside should go back in the table,
but in the one scenario tested it happened to be among the deleted items. The test now runs five
scenarios.

## Limits

- **Not tuned for speed.** It hashes with std's SipHash, and has no SIMD or the cuckoo paper's
  semi-sorted buckets, which save another bit per item.
- **Cuckoo fingerprints are stored in 16 bits** whatever their size, to keep the code simple.
  `bits()` reports the space the paper counts, which a packed table would use.
- **Not concurrent.** Inserts take `&mut self`.

Claude, Anthropic's AI model, wrote this with Halil Utku Demirtaş directing the work.

## License

MIT
