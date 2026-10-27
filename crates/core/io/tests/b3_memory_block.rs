//! B3: `MemoryBlock` against a plain `Vec<u8>` model.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use proptest::prelude::*;
use syrp_io::{IoErrorKind, MemoryBlock};

#[derive(Debug, Clone)]
enum Op {
    SetSize(usize),
    EnsureSize(usize),
    Fill(u8),
    Append(Vec<u8>),
    Insert(usize, Vec<u8>),
    RemoveSection(usize, usize),
    ReplaceAll(Vec<u8>),
    CopyFrom(Vec<u8>, isize),
    CopyTo(usize, isize),
    BitRange(usize, usize),
    SetBitRange(usize, usize, u32),
    Swap(Vec<u8>),
}

fn op() -> impl Strategy<Value = Op> {
    let bytes = || proptest::collection::vec(any::<u8>(), 0..12);
    let size = 0usize..40;
    prop_oneof![
        size.clone().prop_map(Op::SetSize),
        size.clone().prop_map(Op::EnsureSize),
        any::<u8>().prop_map(Op::Fill),
        bytes().prop_map(Op::Append),
        (size.clone(), bytes()).prop_map(|(o, b)| Op::Insert(o, b)),
        (size.clone(), size.clone()).prop_map(|(s, n)| Op::RemoveSection(s, n)),
        bytes().prop_map(Op::ReplaceAll),
        (bytes(), -20isize..40).prop_map(|(b, o)| Op::CopyFrom(b, o)),
        (0usize..12, -20isize..40).prop_map(|(n, o)| Op::CopyTo(n, o)),
        (0usize..340, 0usize..40).prop_map(|(s, n)| Op::BitRange(s, n)),
        (0usize..340, 0usize..40, any::<u32>()).prop_map(|(s, n, v)| Op::SetBitRange(s, n, v)),
        bytes().prop_map(Op::Swap),
    ]
}

/// The model's bit `bit`, or false past the end.
fn model_bit(m: &[u8], bit: usize) -> bool {
    m.get(bit / 8).is_some_and(|b| b & (1 << (bit % 8)) != 0)
}

/// Applies `op` to the block and the model, comparing any value an operation returns.
fn apply(block: &mut MemoryBlock, m: &mut Vec<u8>, op: Op) {
    match op {
        Op::SetSize(n) => {
            block.set_size(n, false).unwrap();
            m.resize(n, 0);
        }
        Op::EnsureSize(n) => {
            block.ensure_size(n).unwrap();
            while m.len() < n {
                m.push(0);
            }
        }
        Op::Fill(b) => {
            block.fill(b);
            m.iter_mut().for_each(|x| *x = b);
        }
        Op::Append(b) => {
            block.append(&b);
            m.extend(b);
        }
        Op::Insert(o, b) => {
            block.insert(o, &b);
            let at = o.min(m.len());
            for (i, x) in b.into_iter().enumerate() {
                m.insert(at + i, x);
            }
        }
        Op::RemoveSection(s, n) => {
            block.remove_section(s, n);
            for _ in 0..n {
                if s < m.len() {
                    m.remove(s);
                }
            }
        }
        Op::ReplaceAll(b) => {
            block.replace_all(&b);
            *m = b;
        }
        Op::CopyFrom(b, o) => {
            block.copy_from(&b, o);
            for (i, x) in b.into_iter().enumerate() {
                let at = o + i as isize;
                if at >= 0 && (at as usize) < m.len() {
                    m[at as usize] = x;
                }
            }
        }
        Op::CopyTo(n, o) => {
            let mut out = vec![0xAA; n];
            block.copy_to(&mut out, o);
            let expected: Vec<u8> = (0..n)
                .map(|i| {
                    let at = o + i as isize;
                    if at >= 0 {
                        m.get(at as usize).copied().unwrap_or(0)
                    } else {
                        0
                    }
                })
                .collect();
            assert_eq!(out, expected);
        }
        Op::BitRange(s, n) => {
            let expected = (0..n.min(32))
                .filter(|&i| model_bit(m, s + i))
                .fold(0u32, |acc, i| acc | 1 << i);
            assert_eq!(block.bit_range(s, n), expected);
        }
        Op::SetBitRange(s, n, v) => {
            block.set_bit_range(s, n, v);
            for i in 0..n.min(32) {
                let bit = s + i;
                if let Some(b) = m.get_mut(bit / 8) {
                    if v & (1 << i) != 0 {
                        *b |= 1 << (bit % 8);
                    } else {
                        *b &= !(1 << (bit % 8));
                    }
                }
            }
        }
        Op::Swap(b) => {
            let mut other = MemoryBlock::from_slice(&b);
            block.swap_with(&mut other);
            assert_eq!(&other[..], &m[..]);
            *m = b;
        }
    }
}

proptest! {
    #[test]
    fn b3_operation_sequences_agree_with_vec_model(
        start in proptest::collection::vec(any::<u8>(), 0..16),
        ops in proptest::collection::vec(op(), 0..40),
    ) {
        let mut block = MemoryBlock::from_slice(&start);
        let mut model = start;
        for op in ops {
            apply(&mut block, &mut model, op);
            prop_assert_eq!(&block[..], &model[..]);
            prop_assert_eq!(block.len(), model.len());
            prop_assert_eq!(block.is_empty(), model.is_empty());
        }
    }
}

fn hash(b: &MemoryBlock) -> u64 {
    let mut h = DefaultHasher::new();
    b.hash(&mut h);
    h.finish()
}

#[test]
fn b3_construction_equality_hash_and_views() {
    let a = MemoryBlock::with_size(4, false).unwrap();
    assert_eq!(&a[..], &[0; 4]);
    let mut b = MemoryBlock::from(vec![0u8; 4]);
    assert_eq!(a, b);
    assert_eq!(hash(&a), hash(&b));
    b[1] = 9;
    assert_ne!(a, b);
    assert_eq!(b.as_view(), &[0, 9, 0, 0]);
    assert_eq!(b.clone().into_vec(), vec![0, 9, 0, 0]);
    assert_eq!(
        MemoryBlock::from_slice(b"h\xC3\xA9").to_string_lossy(),
        "hé"
    );
    assert!(MemoryBlock::new().is_empty());
}

#[test]
fn b3_huge_sizes_are_syrp_io_0005_not_an_abort() {
    let err = MemoryBlock::with_size(usize::MAX, true).unwrap_err();
    assert_eq!(err.kind(), IoErrorKind::OutOfMemory);
    let mut block = MemoryBlock::from_slice(b"keep");
    assert!(block.set_size(usize::MAX / 2, true).is_err());
    assert!(block.ensure_size(usize::MAX).is_err());
    assert_eq!(&block[..], b"keep");
}

#[test]
fn b3_bit_ranges_at_extreme_offsets_do_not_panic() {
    let mut block = MemoryBlock::from_slice(&[0xFF; 2]);
    assert_eq!(block.bit_range(usize::MAX - 3, 32), 0);
    block.set_bit_range(usize::MAX - 3, 32, u32::MAX);
    assert_eq!(block.bit_range(0, 64), 0xFFFF);
    block.copy_from(&[1], isize::MIN);
    block.copy_from(&[1], isize::MAX);
    let mut out = [5u8; 2];
    block.copy_to(&mut out, isize::MIN);
    assert_eq!(out, [0, 0]);
    block.remove_section(usize::MAX, usize::MAX);
    block.insert(usize::MAX, &[3]);
    assert_eq!(&block[..], &[0xFF, 0xFF, 3]);
}
