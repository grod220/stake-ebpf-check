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
fn add_hi_mod(hi: &mut u64, add: u64, cp: u64) -> u64 {
    debug_assert!(cp > 0);
    if add == 0 {
        return 0;
    }

    if *hi >= cp {
        *hi %= cp;
    }

    let space = cp - *hi;
    if add >= space {
        *hi = add - space;
        1
    } else {
        *hi += add;
        0
    }
}

#[inline]
fn add_base_10k_mod(hi: &mut u64, lo: &mut u64, add_hi: u64, add_lo: u64, cp: u64) -> u64 {
    debug_assert!(cp > 0);
    let mut carry_hi = 0;

    let mut lo_sum = *lo + add_lo;
    if lo_sum >= BASIS_POINTS_PER_UNIT {
        lo_sum -= BASIS_POINTS_PER_UNIT;
        carry_hi = 1;
    }
    *lo = lo_sum;

    let mut wraps = add_hi_mod(hi, add_hi, cp);
    wraps += add_hi_mod(hi, carry_hi, cp);
    wraps
}

#[inline]
fn double_base_10k_reduce(hi: &mut u64, lo: &mut u64, cp: u64) -> u64 {
    debug_assert!(cp > 0);
    let prev_hi = *hi;
    let prev_lo = *lo;
    let wraps = add_base_10k_mod(hi, lo, prev_hi, prev_lo, cp);
    if wraps != 0 { 1 } else { 0 }
}

#[inline]
fn add_with_cap(current: u64, add: u64, cap: u64) -> Option<u64> {
    if add == 0 {
        return Some(current);
    }
    if add > cap {
        return None;
    }
    if current > cap - add {
        None
    } else {
        Some(current + add)
    }
}

#[inline]
fn double_with_cap(q: u64, bit: u64, cap: u64) -> Option<u64> {
    debug_assert!(bit <= 1);
    if bit > cap {
        return None;
    }
    let allowed = (cap - bit) >> 1;
    if q > allowed {
        None
    } else {
        Some((q << 1) | bit)
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
    let adder_q = if cp == 0 { return None; } else { a_hi_all / cp };
    let adder_hi = a_hi_all % cp;
    let adder_lo = a_lo;

    let mut q: u64 = 0;
    let mut r_hi: u64 = 0;
    let mut r_lo: u64 = 0;

    for i in (0..64).rev() {
        let bit = double_base_10k_reduce(&mut r_hi, &mut r_lo, cp);
        q = double_with_cap(q, bit, q_cap)?;

        if ((b >> i) & 1) != 0 {
            if let Some(new_q) = add_with_cap(q, adder_q, q_cap) {
                q = new_q;
            } else {
                return None;
            }

            let wraps = add_base_10k_mod(&mut r_hi, &mut r_lo, adder_hi, adder_lo, cp);
            if wraps != 0 {
                q = add_with_cap(q, wraps, q_cap)?;
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
    debug_assert!(cp > 0);
    let adder_hi = rem_hi;
    let adder_lo = rem_lo;

    let mut q: u64 = 0;
    let mut r_hi: u64 = 0;
    let mut r_lo: u64 = 0;

    for i in (0..64).rev() {
        let bit = double_base_10k_reduce(&mut r_hi, &mut r_lo, cp);
        q = (q << 1) | bit;
        if ((k >> i) & 1) != 0 {
            let wraps = add_base_10k_mod(&mut r_hi, &mut r_lo, adder_hi, adder_lo, cp);
            if wraps != 0 {
                q = q.wrapping_add(wraps);
            }
        }
    }
    q
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manual_mul_div(a: u64, b: u64, cp: u64) -> (u64, u64, u64) {
        let modulus = (cp as u128) * (BASIS_POINTS_PER_UNIT as u128);
        let prod = (a as u128) * (b as u128);
        let q = prod / modulus;
        let rem = prod % modulus;
        (
            q as u64,
            (rem / (BASIS_POINTS_PER_UNIT as u128)) as u64,
            (rem % (BASIS_POINTS_PER_UNIT as u128)) as u64,
        )
    }

    #[test]
    fn mul_div_matches_manual() {
        let a_vals = [1, 9_999, 50_000, 100_001, u32::MAX as u64, u64::MAX / 8];
        let b_vals = [1, 3, 7, 63, 1_000_000, u32::MAX as u64];
        let cp_vals = [1, 2, 3, 10, 10_000, 1_000_000_000, u64::MAX / 1024];

        for &a in &a_vals {
            for &b in &b_vals {
                for &cp in &cp_vals {
                    if cp == 0 {
                        continue;
                    }
                    let manual = manual_mul_div(a, b, cp);
                    let streaming = mul_div_by_cp10k_capped(a, b, cp, u64::MAX)
                        .expect("unexpected cap hit");
                    assert_eq!(
                        streaming, manual,
                        "mismatch for a={a}, b={b}, cp={cp}"
                    );
                }
            }
        }
    }

    fn manual_remainder_mul(rem_hi: u64, rem_lo: u64, k: u64, cp: u64) -> u64 {
        let remainder =
            (rem_hi as u128) * (BASIS_POINTS_PER_UNIT as u128) + rem_lo as u128;
        let modulus = (cp as u128) * (BASIS_POINTS_PER_UNIT as u128);
        let prod = remainder * k as u128;
        (prod / modulus) as u64
    }

    #[test]
    fn remainder_mul_matches_manual() {
        let rem_hi_vals = [0, 1, 123, 9_999_999];
        let rem_lo_vals = [0, 1, 9_999];
        let k_vals = [0, 1, 7, 900, 2_500];
        let cp_vals = [1, 2, 3, 10, 10_000, 1_000_000];

        for &hi in &rem_hi_vals {
            for &lo in &rem_lo_vals {
                for &k in &k_vals {
                    for &cp in &cp_vals {
                        if cp == 0 {
                            continue;
                        }
                        let manual = manual_remainder_mul(hi, lo, k, cp);
                        let streaming = remainder_mul_div(hi, lo, k, cp);
                        assert_eq!(
                            streaming, manual,
                            "rem mul mismatch hi={hi}, lo={lo}, k={k}, cp={cp}"
                        );
                    }
                }
            }
        }
    }
}
