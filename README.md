# bloom-cuckoo-rs

[![ci](https://github.com/Urthella/bloom-cuckoo-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/Urthella/bloom-cuckoo-rs/actions/workflows/ci.yml)

Bloom, cuckoo and xor filters in about 300 lines of Rust with no dependencies, measured against
each other and against the theory. All three answer "might this be in the set?" in far less
space than the set: "no" is always right, and "yes" is wrong at a rate you choose.

```
cargo run --release --example compare
```

985,000 items in each filter, then 2,000,000 lookups of items that aren't there:

| Filter | Bits per item | Expected false positives | Measured | Insert ns | Lookup ns | Deletes |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| Bloom, k=7 | 9.6 | 1.0039% | 1.0089% | 47 | 53 | no |
| Bloom, k=10 | 14.4 | 0.1000% | 0.1020% | 55 | 52 | no |
| Bloom, k=13 | 19.2 | 0.0100% | 0.0104% | 76 | 52 | no |
| Xor, 8-bit fingerprints | 9.8 | 0.3906% | 0.3886% | 221 | 35 | no |
| Xor, 12-bit fingerprints | 14.8 | 0.0244% | 0.0249% | 201 | 25 | no |
| Xor, 16-bit fingerprints | 19.7 | 0.0015% | 0.0017% | 206 | 28 | no |
| Cuckoo, 8-bit fingerprints | 8.5 | 2.9097% | 2.9361% | 60 | 36 | yes |
| Cuckoo, 12-bit fingerprints | 12.8 | 0.1834% | 0.1873% | 55 | 32 | yes |
| Cuckoo, 16-bit fingerprints | 17.0 | 0.0115% | 0.0103% | 73 | 36 | yes |

An xor filter's "insert" is its whole build, divided by the number of items.

"Expected" is what the formulas below predict for each filter's actual size, and every measured
rate is close to it. The cuckoo filters are 94% full, the load they really run at. Times are from
one run on a Windows laptop, so read them as rough.

## Which to use

- **An xor filter** is the smallest at any rate and has the fastest lookups, but it's built once
  from a fixed set and can't change. At 0.39% it takes 9.8 bits per item, where a Bloom filter
  would need about 11.5. Good for data that's written once, like an index file.
- **A Bloom filter** never fills up, works at any rate and takes new items any time, but can't
  delete. At around 1% it's smaller than a cuckoo filter.
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

**Xor** (Graf and Lemire, 2019): each item has three slots, one in each third of a table of about
`1.23·n` slots, and its fingerprint is the XOR of what's in them. To build it, take a slot that
only one item uses, set that item aside, and repeat; then fill the slots in reverse order, each
item's own slot last. A non-member's three slots XOR to its fingerprint by chance: `1/2^f`.

```rust
use bloom_cuckoo_rs::{Bloom, Cuckoo, Xor};

let xor = Xor::build(["alice", "bob"].iter().copied(), 8);
assert!(xor.contains("alice"));

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
by the formulas, that a cuckoo filter fills past 90% before refusing, that deleting keeps
every other item, that an xor filter builds for sets from 0 to 100,000 items and with repeats,
and that it uses 1.23 slots per item.

To check that the tests can fail, 18 bugs were planted one at a time, and every one was caught:
among them, an alternate bucket that isn't symmetric, a fingerprint allowed to be 0 (which means
empty), forgetting the item set aside when the table is full, probes that all hit the same bit,
and xor slots filled in peeling order instead of reverse. One survived the first time: after deletes, the item set aside should go back in the table,
but in the one scenario tested it happened to be among the deleted items. The test now runs five
scenarios.

## Limits

- **Not tuned for speed.** It hashes with std's SipHash, and has no SIMD or the cuckoo paper's
  semi-sorted buckets, which save another bit per item.
- **Cuckoo and xor fingerprints are stored in 16 bits** whatever their size, to keep the code
  simple. `bits()` reports the space the papers count, which a packed table would use.
- **Building an xor filter takes about 21 bytes per item of working memory** for a moment, on top
  of the filter: a hash per item and a count and XOR per slot.
- **Not concurrent.** Inserts take `&mut self`.

Claude, Anthropic's AI model, wrote this with Halil Utku Demirtaş directing the work.

## License

MIT
