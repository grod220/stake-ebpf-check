#![no_std]
#![allow(clippy::needless_return)]
#![allow(clippy::manual_bits)]

// BPF-compatible streaming arithmetic using only u64 operations.
// No u128, no aggregate returns, minimal stack usage.

pub const BASIS_POINTS_PER_UNIT: u64 = 10_000;

#[inline]
pub fn split_base_10k(x: u64) -> (u64, u64) {
    (x / BASIS_POINTS_PER_UNIT, x % BASIS_POINTS_PER_UNIT)
}

#[inline]
pub fn add_base_10k(hi: &mut u64, lo: &mut u64, add_hi: u64, add_lo: u64) {
    let mut lo2 = *lo + add_lo;
    let carry = if lo2 >= BASIS_POINTS_PER_UNIT {
        lo2 -= BASIS_POINTS_PER_UNIT;
        1
    } else {
        0
    };
    *lo = lo2;
    *hi = hi.wrapping_add(add_hi).wrapping_add(carry);
}

// Double remainder r = (hi, lo) modulo (cp*10_000), updating quotient bit.
#[inline]
pub fn double_base_10k_reduce(
    hi: &mut u64,
    lo: &mut u64,
    cp: u64,
    q: &mut u64,
) {
    let mut lo2 = *lo << 1;
    let carry_lo = if lo2 >= BASIS_POINTS_PER_UNIT {
        lo2 -= BASIS_POINTS_PER_UNIT;
        1
    } else {
        0
    };
    *lo = lo2;

    let thresh = (cp - carry_lo + 1) >> 1;
    if *hi >= thresh {
        let t = cp - *hi - carry_lo;
        *hi = hi.wrapping_sub(t);
        *q = (*q << 1) | 1;
    } else {
        *hi = (*hi << 1) + carry_lo;
        *q <<= 1;
    }
}

// Multiply a*b with saturation to 'cap', using shift-add (no 64x64->128).
#[inline]
pub fn mul_cap(mut a: u64, mut b: u64, cap: u64) -> u64 {
    if a == 0 || b == 0 { return 0; }
    let mut res: u64 = 0;
    while b != 0 && res < cap {
        if (b & 1) != 0 {
            let room = cap.saturating_sub(res);
            if a >= room { return cap; }
            res += a;
        }
        if a > (cap >> 1) { a = cap; } else { a <<= 1; }
        b >>= 1;
    }
    if res > cap { cap } else { res }
}

// Compute floor((a*b)/ (cp*10k)), with early quotient cap (q_cap).
// Returns (q, rem_hi, rem_lo) where remainder = rem_hi*10k + rem_lo.
#[inline]
pub fn mul_div_by_cp10k_capped(
    a: u64,
    b: u64,
    cp: u64,
    q_cap: u64,
) -> Option<(u64, u64, u64)> {
    if a == 0 || b == 0 { return Some((0, 0, 0)); }

    let (a_hi_all, a_lo) = split_base_10k(a);
    let adder_hi = a_hi_all % cp;
    let adder_lo = a_lo;

    let mut q: u64 = 0;
    let mut r_hi: u64 = 0;
    let mut r_lo: u64 = 0;

    for i in (0..64).rev() {
        if q > (q_cap >> 1) { return None; }

        double_base_10k_reduce(&mut r_hi, &mut r_lo, cp, &mut q);

        if ((b >> i) & 1) != 0 {
            add_base_10k(&mut r_hi, &mut r_lo, adder_hi, adder_lo);
            if r_hi >= cp {
                r_hi -= cp;
                if q >= q_cap { return None; }
                q = q.wrapping_add(1);
            }
        }
    }
    Some((q, r_hi, r_lo))
}

// Compute floor( (rem * k) / (cp*10k) ) where rem = rem_hi*10k + rem_lo < cp*10k.
#[inline]
pub fn remainder_mul_div(
    rem_hi: u64,
    rem_lo: u64,
    k: u64,
    cp: u64,
) -> u64 {
    if k == 0 { return 0; }
    let adder_hi = rem_hi;
    let adder_lo = rem_lo;

    let mut q: u64 = 0;
    let mut r_hi: u64 = 0;
    let mut r_lo: u64 = 0;

    for i in (0..64).rev() {
        double_base_10k_reduce(&mut r_hi, &mut r_lo, cp, &mut q);
        if ((k >> i) & 1) != 0 {
            add_base_10k(&mut r_hi, &mut r_lo, adder_hi, adder_lo);
            if r_hi >= cp {
                r_hi -= cp;
                q = q.wrapping_add(1);
            }
        }
    }
    q
}
