//! Statistical checks: false positive rates are measured over many lookups and compared with
//! what the theory predicts for each filter's actual size.

use bloom_cuckoo_rs::{Bloom, Cuckoo, Full};

const N: u64 = 50_000;
const QUERIES: u64 = 500_000;

/// Members are 0..N; everything from 1<<40 up is a non-member.
fn measured(contains: impl Fn(u64) -> bool) -> f64 {
    (0..QUERIES).filter(|&q| contains((1 << 40) + q)).count() as f64 / QUERIES as f64
}

fn close(measured: f64, expected: f64, tolerance: f64) -> bool {
    (measured - expected).abs() <= expected * tolerance
}

#[test]
fn bloom_has_no_false_negatives_and_the_expected_rate() {
    for p in [0.1, 0.01, 0.001] {
        let mut bloom = Bloom::new(N as usize, p);
        (0..N).for_each(|i| bloom.insert(&i));
        assert!((0..N).all(|i| bloom.contains(&i)));
        let rate = measured(|q| bloom.contains(&q));
        let expected = bloom.expected_fpr();
        assert!(
            close(rate, expected, 0.15),
            "p={p}: measured {rate}, expected {expected}"
        );
        assert!(close(expected, p, 0.15), "p={p}: sized for {expected}");
    }
}

#[test]
fn bloom_sizes_itself_by_the_formula() {
    let bloom = Bloom::new(1000, 0.01);
    // m = -1000·ln(0.01)/ln(2)² = 9585.06, k = 9.586·ln(2) = 6.64
    assert_eq!(bloom.bits(), 9586);
    assert_eq!(bloom.hashes(), 7);
    let tiny = Bloom::new(0, 0.5);
    assert_eq!(tiny.bits(), 64);
}

#[test]
fn bloom_with_one_hash_or_too_many_is_worse() {
    let rate = |k| {
        let mut bloom = Bloom::with_size(N * 10, k);
        (0..N).for_each(|i| bloom.insert(&i));
        measured(|q| bloom.contains(&q))
    };
    let best = rate(7);
    assert!(
        rate(1) > best * 5.0 && rate(20) > best * 2.0,
        "k=1 {} k=7 {} k=20 {}",
        rate(1),
        best,
        rate(20)
    );
}

#[test]
fn cuckoo_has_no_false_negatives_and_the_expected_rate() {
    for fp_bits in [8, 12, 16] {
        let mut cuckoo = Cuckoo::new(N as usize, fp_bits);
        (0..N).for_each(|i| cuckoo.insert(&i).unwrap());
        assert!((0..N).all(|i| cuckoo.contains(&i)));
        let rate = measured(|q| cuckoo.contains(&q));
        let expected = cuckoo.expected_fpr();
        let tolerance = if fp_bits == 16 { 0.5 } else { 0.15 }; // few hits at 16 bits, so more noise
        assert!(
            close(rate, expected, tolerance),
            "{fp_bits} bits: measured {rate}, expected {expected}"
        );
    }
}

/// Inserts base, base + 1, base + 2… until the filter refuses one, and returns how many went in.
fn fill(cuckoo: &mut Cuckoo, base: u64) -> u64 {
    (0..1_000_000u64)
        .find(|j| cuckoo.insert(&(base + j)).is_err())
        .expect("a cuckoo filter fills up")
}

#[test]
fn cuckoo_fills_past_ninety_percent_before_refusing() {
    let mut cuckoo = Cuckoo::new(4096, 12);
    let i = fill(&mut cuckoo, 0);
    assert!(cuckoo.load() > 0.9, "full at {}", cuckoo.load());
    // Everything that went in, including the one that had nowhere to go, is still found.
    assert!((0..i).all(|j| cuckoo.contains(&j)));
    assert_eq!(cuckoo.insert(&u64::MAX), Err(Full));
}

#[test]
fn cuckoo_deletes() {
    let mut cuckoo = Cuckoo::new(N as usize, 16);
    (0..N).for_each(|i| cuckoo.insert(&i).unwrap());
    (0..N)
        .filter(|i| i % 2 == 0)
        .for_each(|i| assert!(cuckoo.remove(&i)));
    assert_eq!(cuckoo.len(), N as usize / 2);
    assert!(
        (0..N).filter(|i| i % 2 == 1).all(|i| cuckoo.contains(&i)),
        "deleting must not lose other items"
    );
    let still_found = (0..N).filter(|i| i % 2 == 0 && cuckoo.contains(i)).count();
    assert!(still_found < 50, "{still_found} deleted items still found");
}

#[test]
fn cuckoo_keeps_one_copy_per_insert() {
    let mut cuckoo = Cuckoo::new(100, 16);
    cuckoo.insert("x").unwrap();
    cuckoo.insert("x").unwrap();
    assert!(cuckoo.remove("x"));
    assert!(cuckoo.contains("x"));
    assert!(cuckoo.remove("x"));
    assert!(!cuckoo.contains("x"));
    assert!(!cuckoo.remove("x"));
    assert!(cuckoo.is_empty());
}

#[test]
fn cuckoo_takes_new_items_again_after_a_delete() {
    // Different items each run, so the item left with nowhere to go is a different one, and
    // most of the time it isn't among those deleted: then it has to be put back in the table.
    for run in 0..5u64 {
        let base = run << 32;
        let mut cuckoo = Cuckoo::new(1024, 12);
        let i = fill(&mut cuckoo, base);
        let len = cuckoo.len();
        // Free a tenth of the table, so there's room again.
        for j in 0..i / 10 {
            assert!(cuckoo.remove(&(base + j)));
        }
        assert_eq!(cuckoo.len(), len - (i / 10) as usize, "run {run}");
        assert_eq!(cuckoo.insert(&u64::MAX), Ok(()), "run {run}");
        assert!(
            (i / 10..i).all(|j| cuckoo.contains(&(base + j))),
            "run {run}"
        );
        assert!(cuckoo.contains(&u64::MAX));
    }
}

#[test]
fn cuckoo_needs_fewer_bits_than_bloom_at_low_rates() {
    // Around 0.02%, a cuckoo filter with 16-bit fingerprints beats a Bloom filter per item.
    let mut cuckoo = Cuckoo::new(N as usize, 16);
    (0..N).for_each(|i| cuckoo.insert(&i).unwrap());
    let bloom = Bloom::new(N as usize, cuckoo.expected_fpr());
    let per_item = |bits: u64| bits as f64 / N as f64;
    // Compared at the load a cuckoo filter really runs at, 95%.
    let cuckoo_bits = 16.0 / 0.95;
    assert!(
        cuckoo_bits < per_item(bloom.bits()),
        "cuckoo {cuckoo_bits}, bloom {}",
        per_item(bloom.bits())
    );
    assert!(
        per_item(cuckoo.bits()) > 16.0,
        "this table has slack, so it's bigger than that"
    );
}

#[test]
fn xor_has_no_false_negatives_and_the_expected_rate() {
    use bloom_cuckoo_rs::Xor;
    for fp_bits in [8, 12] {
        let xor = Xor::build(&(0..N).collect::<Vec<_>>(), fp_bits);
        assert!((0..N).all(|i| xor.contains(&i)));
        let rate = measured(|q| xor.contains(&q));
        let expected = xor.expected_fpr();
        let tolerance = if fp_bits == 12 { 0.3 } else { 0.15 }; // about 120 hits at 12 bits
        assert!(
            close(rate, expected, tolerance),
            "{fp_bits} bits: measured {rate}, expected {expected}"
        );
        // About 1.23 slots per item.
        let per_item = xor.bits() as f64 / N as f64 / fp_bits as f64;
        assert!(
            (1.23..1.24).contains(&per_item),
            "{per_item} slots per item"
        );
    }
}

#[test]
fn xor_builds_for_any_set() {
    use bloom_cuckoo_rs::Xor;
    for n in [0u64, 1, 2, 3, 10, 1000, 100_000] {
        let items: Vec<u64> = (0..n).collect();
        let xor = Xor::build(&items, 8);
        assert!(items.iter().all(|i| xor.contains(i)), "n={n}");
    }
    // Repeats don't stop it from building.
    let repeated = Xor::build(&[1u64, 2, 2, 3, 3, 3], 16);
    assert!([1u64, 2, 3].iter().all(|i| repeated.contains(i)));
    assert!(!repeated.contains(&4u64));
    let words = Xor::build(["alice", "bob"].iter().copied(), 16);
    assert!(words.contains("alice") && words.contains("bob") && !words.contains("carol"));
}

#[test]
fn xor_is_the_smallest_at_the_same_rate() {
    use bloom_cuckoo_rs::Xor;
    let xor = Xor::build(&(0..N).collect::<Vec<_>>(), 12);
    let rate = xor.expected_fpr();
    let per_item = |bits: u64| bits as f64 / N as f64;
    // A Bloom filter sized for the same rate.
    let bloom = Bloom::new(N as usize, rate);
    assert!(per_item(xor.bits()) < per_item(bloom.bits()));
    // The smallest cuckoo fingerprint that gets as low at 95% full: 8·0.95/(2^f - 1) ≤ rate.
    let f = (1..=16)
        .find(|f| 8.0 * 0.95 / ((1u64 << f) - 1) as f64 <= rate)
        .unwrap();
    assert_eq!(f, 15);
    assert!(per_item(xor.bits()) < f as f64 / 0.95);
}
