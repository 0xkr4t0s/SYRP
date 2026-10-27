// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! Unsigned magnitude arithmetic on little-endian `u64` limbs.
//!
//! A magnitude is normalised when it has no zero limb at the top; zero is the empty slice.
//! Every function here takes normalised input and returns normalised output.

use std::cmp::Ordering;

/// Drops zero limbs from the top.
pub(super) fn normalise(v: &mut Vec<u64>) {
    while v.last() == Some(&0) {
        v.pop();
    }
}

/// Numeric comparison of two normalised magnitudes.
pub(super) fn cmp(a: &[u64], b: &[u64]) -> Ordering {
    a.len()
        .cmp(&b.len())
        .then_with(|| a.iter().rev().cmp(b.iter().rev()))
}

/// `a + b`.
pub(super) fn add(a: &[u64], b: &[u64]) -> Vec<u64> {
    let (long, short) = if a.len() >= b.len() { (a, b) } else { (b, a) };
    let mut out = Vec::with_capacity(long.len() + 1);
    let mut carry = false;
    for (i, &x) in long.iter().enumerate() {
        let y = short.get(i).copied().unwrap_or(0);
        let (s, c1) = x.overflowing_add(y);
        let (s, c2) = s.overflowing_add(u64::from(carry));
        out.push(s);
        carry = c1 || c2;
    }
    if carry {
        out.push(1);
    }
    out
}

/// `a - b`; requires `a >= b`.
pub(super) fn sub(a: &[u64], b: &[u64]) -> Vec<u64> {
    debug_assert!(cmp(a, b) != Ordering::Less);
    let mut out = Vec::with_capacity(a.len());
    let mut borrow = false;
    for (i, &x) in a.iter().enumerate() {
        let y = b.get(i).copied().unwrap_or(0);
        let (d, b1) = x.overflowing_sub(y);
        let (d, b2) = d.overflowing_sub(u64::from(borrow));
        out.push(d);
        borrow = b1 || b2;
    }
    normalise(&mut out);
    out
}

/// `a * b` (schoolbook).
pub(super) fn mul(a: &[u64], b: &[u64]) -> Vec<u64> {
    if a.is_empty() || b.is_empty() {
        return Vec::new();
    }
    let mut out = vec![0_u64; a.len() + b.len()];
    for (i, &x) in a.iter().enumerate() {
        let mut carry = 0_u128;
        for (j, &y) in b.iter().enumerate() {
            let t = u128::from(x) * u128::from(y) + u128::from(out[i + j]) + carry;
            out[i + j] = t as u64;
            carry = t >> 64;
        }
        out[i + b.len()] = carry as u64;
    }
    normalise(&mut out);
    out
}

/// `(a / d, a % d)` for a single non-zero limb `d`.
fn div_rem_limb(a: &[u64], d: u64) -> (Vec<u64>, u64) {
    let mut q = vec![0_u64; a.len()];
    let mut rem = 0_u128;
    let d = u128::from(d);
    for i in (0..a.len()).rev() {
        let cur = (rem << 64) | u128::from(a[i]);
        q[i] = (cur / d) as u64;
        rem = cur % d;
    }
    normalise(&mut q);
    (q, rem as u64)
}

/// `(a / b, a % b)`; requires `b` non-zero. Long division with normalised divisor and
/// two-limb quotient estimates, corrected by at most one add-back per step.
pub(super) fn div_rem(a: &[u64], b: &[u64]) -> (Vec<u64>, Vec<u64>) {
    debug_assert!(!b.is_empty());
    if cmp(a, b) == Ordering::Less {
        return (Vec::new(), a.to_vec());
    }
    if b.len() == 1 {
        let (q, r) = div_rem_limb(a, b[0]);
        let mut r = vec![r];
        normalise(&mut r);
        return (q, r);
    }
    let s = b[b.len() - 1].leading_zeros();
    let vn = shl(b, u64::from(s));
    let mut un = shl(a, u64::from(s));
    un.resize(a.len() + 1, 0);
    let n = vn.len();
    let m = a.len() - n;
    let mut q = vec![0_u64; m + 1];
    let vtop = u128::from(vn[n - 1]);
    let vsec = u128::from(vn[n - 2]);
    for j in (0..=m).rev() {
        let num = (u128::from(un[j + n]) << 64) | u128::from(un[j + n - 1]);
        let mut qhat = num / vtop;
        let mut rhat = num % vtop;
        while qhat >> 64 != 0 || qhat * vsec > ((rhat << 64) | u128::from(un[j + n - 2])) {
            qhat -= 1;
            rhat += vtop;
            if rhat >> 64 != 0 {
                break;
            }
        }
        let mut carry = 0_u128;
        let mut borrow = false;
        for i in 0..n {
            let p = qhat * u128::from(vn[i]) + carry;
            carry = p >> 64;
            let (t, b1) = un[i + j].overflowing_sub(p as u64);
            let (t, b2) = t.overflowing_sub(u64::from(borrow));
            un[i + j] = t;
            borrow = b1 || b2;
        }
        let (t, b1) = un[j + n].overflowing_sub(carry as u64);
        let (t, b2) = t.overflowing_sub(u64::from(borrow));
        un[j + n] = t;
        if b1 || b2 {
            qhat -= 1;
            let mut c = 0_u128;
            for i in 0..n {
                let s = u128::from(un[i + j]) + u128::from(vn[i]) + c;
                un[i + j] = s as u64;
                c = s >> 64;
            }
            un[j + n] = un[j + n].wrapping_add(c as u64);
        }
        q[j] = qhat as u64;
    }
    normalise(&mut q);
    un.truncate(n);
    normalise(&mut un);
    (q, shr(&un, u64::from(s)))
}

/// `a << bits`.
pub(super) fn shl(a: &[u64], bits: u64) -> Vec<u64> {
    if a.is_empty() {
        return Vec::new();
    }
    let limbs = usize::try_from(bits / 64).expect("shift exceeds the address space");
    let s = (bits % 64) as u32;
    let mut out = vec![0_u64; limbs];
    out.reserve(a.len() + 1);
    if s == 0 {
        out.extend_from_slice(a);
    } else {
        let mut carry = 0_u64;
        for &x in a {
            out.push((x << s) | carry);
            carry = x >> (64 - s);
        }
        out.push(carry);
    }
    normalise(&mut out);
    out
}

/// `a >> bits`.
pub(super) fn shr(a: &[u64], bits: u64) -> Vec<u64> {
    let limbs = usize::try_from(bits / 64).unwrap_or(usize::MAX);
    if limbs >= a.len() {
        return Vec::new();
    }
    let s = (bits % 64) as u32;
    let src = &a[limbs..];
    let mut out = Vec::with_capacity(src.len());
    if s == 0 {
        out.extend_from_slice(src);
    } else {
        for (i, &x) in src.iter().enumerate() {
            let hi = src.get(i + 1).map_or(0, |&h| h << (64 - s));
            out.push((x >> s) | hi);
        }
    }
    normalise(&mut out);
    out
}
