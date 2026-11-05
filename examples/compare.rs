//! cargo run --release --example compare
//!
//! The same items in each filter, at the load a cuckoo filter really runs at: 94% of its slots.

use std::hint::black_box;
use std::time::Instant;

use bloom_cuckoo_rs::{Bloom, Cuckoo, Xor};

const N: u64 = 985_000; // 94% of 2^20 cuckoo slots
const QUERIES: u64 = 2_000_000;

fn row(
    name: &str,
    bits: u64,
    expected: f64,
    insert_ns: f64,
    contains: impl Fn(u64) -> bool,
    deletes: bool,
) {
    assert!((0..N).all(&contains), "{name}: a false negative");
    let started = Instant::now();
    let hits = (0..QUERIES)
        .filter(|&q| contains(black_box((1 << 40) + q)))
        .count();
    let lookup_ns = started.elapsed().as_nanos() as f64 / QUERIES as f64;
    let measured = hits as f64 / QUERIES as f64;
    println!(
        "| {name} | {:.1} | {:.4}% | {:.4}% | {insert_ns:.0} | {lookup_ns:.0} | {} |",
        bits as f64 / N as f64,
        expected * 100.0,
        measured * 100.0,
        if deletes { "yes" } else { "no" }
    );
}

fn timed(insert: impl FnMut(u64)) -> f64 {
    let started = Instant::now();
    (0..N).for_each(insert);
    started.elapsed().as_nanos() as f64 / N as f64
}

fn main() {
    println!("{N} items, {QUERIES} lookups of items that aren't there.\n");
    println!(
        "| Filter | Bits per item | Expected false positives | Measured | Insert ns | Lookup ns | Deletes |"
    );
    println!("| --- | ---: | ---: | ---: | ---: | ---: | --- |");
    for p in [0.01, 0.001, 0.0001] {
        let mut bloom = Bloom::new(N as usize, p);
        let ns = timed(|i| bloom.insert(&i));
        let name = format!("Bloom, k={}", bloom.hashes());
        row(
            &name,
            bloom.bits(),
            bloom.expected_fpr(),
            ns,
            |q| bloom.contains(&q),
            false,
        );
    }
    let items: Vec<u64> = (0..N).collect();
    for fp_bits in [8, 12, 16] {
        let started = Instant::now();
        let xor = Xor::build(&items, fp_bits);
        let ns = started.elapsed().as_nanos() as f64 / N as f64;
        let name = format!("Xor, {fp_bits}-bit fingerprints");
        row(
            &name,
            xor.bits(),
            xor.expected_fpr(),
            ns,
            |q| xor.contains(&q),
            false,
        );
    }
    for fp_bits in [8, 12, 16] {
        let mut cuckoo = Cuckoo::new(N as usize, fp_bits);
        let ns = timed(|i| cuckoo.insert(&i).expect("room for every item"));
        let name = format!("Cuckoo, {fp_bits}-bit fingerprints");
        row(
            &name,
            cuckoo.bits(),
            cuckoo.expected_fpr(),
            ns,
            |q| cuckoo.contains(&q),
            true,
        );
    }
}
