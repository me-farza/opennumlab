// SPDX-License-Identifier: GPL-3.0-or-later
//! Exact bounded integer kernels for Session sum/mean/product; no floating arithmetic.

use mathlab_array::{ArrayErrorV1, DenseDouble2d};
use std::cmp::Ordering;

type Limbs = [u64; 33];
const SIGN: u64 = 1 << 63;
const INF: u64 = 0x7ff0_0000_0000_0000;
const NAN: u64 = 0x7ff8_0000_0000_0000;
const ONE: u64 = 0x3ff0_0000_0000_0000;
const FRACTION: u64 = (1 << 52) - 1;

pub(crate) fn reduce(input: &DenseDouble2d, product: bool) -> Result<DenseDouble2d, ArrayErrorV1> {
    // Session producers and resolved values already enforce the 16/256/2048 caps.
    let shape = input.shape();
    debug_assert!(shape[0] <= 16 && shape[1] <= 16 && input.as_slice().len() <= 256);
    let fibers = if shape[0] == 1 || shape == [0, 0] { 1 } else { shape[1] as usize };
    let terms = if shape[0] == 1 { shape[1] } else { shape[0] } as usize;
    let mut output = [0.0; 16];
    for (fiber, value) in output[..fibers].iter_mut().enumerate() {
        let words = &input.as_slice()[fiber * terms..(fiber + 1) * terms];
        *value = f64::from_bits(if product { product_word(words) } else { additive_word(words, true) });
    }
    DenseDouble2d::try_from_col_major([1, fibers as u64], &output[..fibers], super::OUTPUT_LIMITS)
}

// Sum keeps the same default geometry; only mean normalizes additive fibers.
pub(crate) fn sum(input: &DenseDouble2d) -> Result<DenseDouble2d, ArrayErrorV1> {
    let shape = input.shape();
    debug_assert!(shape[0] <= 16 && shape[1] <= 16 && input.as_slice().len() <= 256);
    let fibers = if shape[0] == 1 || shape == [0, 0] { 1 } else { shape[1] as usize };
    let terms = if shape[0] == 1 { shape[1] } else { shape[0] } as usize;
    let mut output = [0.0; 16];
    for (fiber, value) in output[..fibers].iter_mut().enumerate() {
        *value = f64::from_bits(additive_word(&input.as_slice()[fiber * terms..(fiber + 1) * terms], false));
    }
    DenseDouble2d::try_from_col_major([1, fibers as u64], &output[..fibers], super::OUTPUT_LIMITS)
}

fn add_shift(a: &mut Limbs, significand: u64, shift: usize) {
    let index = shift / 64;
    let digits = (significand as u128) << (shift % 64); // At most 116 bits.
    let mut carry = 0_u128;
    for (i, limb) in a.iter_mut().enumerate() {
        let digit = if i == index { digits as u64 } else if i == index + 1 { (digits >> 64) as u64 } else { 0 };
        let wide = *limb as u128 + digit as u128 + carry; // At most 65 bits.
        *limb = wide as u64;
        carry = wide >> 64;
    }
    debug_assert_eq!(carry, 0); // Signed subtotals are <2^2102; capacity is 2112 bits.
}

fn compare(a: &Limbs, b: &Limbs) -> Ordering {
    for i in (0..33).rev() {
        let order = a[i].cmp(&b[i]);
        if order != Ordering::Equal { return order; }
    }
    Ordering::Equal
}

fn subtract(a: &mut Limbs, b: &Limbs) {
    let mut borrow = 0_u128;
    for i in 0..33 {
        let wide = a[i] as u128 + (1_u128 << 64) - b[i] as u128 - borrow;
        a[i] = wide as u64;
        borrow = u128::from(wide < (1_u128 << 64));
    }
    debug_assert_eq!(borrow, 0); // Caller compared unsigned magnitudes first.
}

fn divide(a: &mut Limbs, divisor: u64) -> u64 {
    let mut remainder = 0_u64;
    for limb in a.iter_mut().rev() {
        // Two base2^32 digits keep every numerator <2^36; no u128 division.
        let high = (remainder << 32) | (*limb >> 32);
        let q_high = high / divisor;
        remainder = high - q_high * divisor;
        let low = (remainder << 32) | (*limb & 0xffff_ffff);
        let q_low = low / divisor;
        remainder = low - q_low * divisor;
        *limb = (q_high << 32) | q_low;
    }
    remainder
}

fn multiply(a: &mut Limbs, significand: u64) {
    let mut carry = 0_u128;
    for limb in &mut a[..14] {
        let wide = *limb as u128 * significand as u128 + carry; // <2^117.
        *limb = wide as u64;
        carry = wide >> 64; // Always <significand.
    }
    debug_assert_eq!(carry, 0); // At most 848 significand bits fit 14*64=896.
}

fn top_bit(a: &Limbs) -> Option<i32> {
    for (i, &word) in a.iter().enumerate().rev() {
        if word != 0 { return Some(i as i32 * 64 + 63 - word.leading_zeros() as i32); }
    }
    None
}

fn window(a: &Limbs, shift: i32) -> u64 {
    debug_assert!(shift >= 0);
    let index = shift as usize / 64;
    if index >= 33 { return 0; }
    let pair = a[index] as u128 | ((a.get(index + 1).copied().unwrap_or(0) as u128) << 64);
    ((pair >> (shift as usize % 64)) as u64) & ((1 << 53) - 1)
}

fn bit(a: &Limbs, index: i32) -> bool {
    if !(0..2112).contains(&index) { return false; }
    a[index as usize / 64] >> (index as usize % 64) & 1 != 0
}

fn lower_nonzero(a: &Limbs, bits: i32) -> bool {
    for (i, &word) in a.iter().enumerate() {
        let used = (bits - i as i32 * 64).clamp(0, 64);
        let mask = if used == 64 { u64::MAX } else { (1_u64 << used) - 1 };
        if word & mask != 0 { return true; }
    }
    false
}

// Pack (a + remainder/divisor)*2^exponent once, nearest/ties-even.
// Only mean has a remainder (exponent=-1074); negative shifts are exact product.
fn pack(a: &Limbs, exponent: i32, remainder: u64, divisor: u64, negative: bool) -> u64 {
    let sign = if negative { SIGN } else { 0 };
    let Some(high) = top_bit(a) else { return sign | u64::from(2 * remainder > divisor); };
    let mut binade = high + exponent;
    if binade >= 1024 { return sign | INF; }
    let normal = binade >= -1022;
    let shift = if normal { high - 52 } else { -1074 - exponent };
    let mut retained = if shift < 0 {
        debug_assert!(remainder == 0 && -shift <= 52);
        a[0] << -shift
    } else {
        window(a, shift)
    };
    if shift == 0 {
        retained += u64::from(2 * remainder > divisor || (2 * remainder == divisor && retained & 1 != 0));
    } else if shift > 0 {
        let guard = bit(a, shift - 1);
        let sticky = remainder != 0 || lower_nonzero(a, shift - 1);
        retained += u64::from(guard && (sticky || retained & 1 != 0));
    }
    if !normal {
        debug_assert!(retained <= 1 << 52);
        return sign | retained; // Carry becomes the smallest normal encoding.
    }
    if retained == 1 << 53 { retained >>= 1; binade += 1; }
    if binade >= 1024 { return sign | INF; } // Genuine final overflow, including tie.
    debug_assert!(((1_u64 << 52)..(1_u64 << 53)).contains(&retained));
    sign | ((binade + 1023) as u64) << 52 | (retained - (1 << 52))
}

fn additive_word(words: &[f64], normalize: bool) -> u64 {
    if words.is_empty() { return if normalize { NAN } else { 0 }; }
    if words.len() == 1 { return words[0].to_bits(); }
    let mut positive = [0_u64; 33];
    let mut negative = [0_u64; 33];
    let mut positive_inf = false;
    let mut negative_inf = false;
    let mut all_negative_zero = true;
    for &value in words {
        let word = value.to_bits();
        let exponent = (word >> 52) & 0x7ff;
        let fraction = word & FRACTION;
        all_negative_zero &= word == SIGN;
        if exponent == 0x7ff {
            if fraction != 0 { return word; } // First NaN wins even after infinities.
            if word & SIGN == 0 { positive_inf = true; } else { negative_inf = true; }
            continue;
        }
        let significand = if exponent == 0 { fraction } else { (1 << 52) | fraction };
        if significand != 0 {
            add_shift(if word & SIGN == 0 { &mut positive } else { &mut negative }, significand, exponent.saturating_sub(1) as usize);
        }
    }
    if positive_inf && negative_inf { return NAN; }
    if positive_inf { return INF; }
    if negative_inf { return SIGN | INF; }
    match compare(&positive, &negative) {
        Ordering::Equal => if all_negative_zero { SIGN } else { 0 },
        Ordering::Greater => {
            subtract(&mut positive, &negative);
            let divisor = if normalize { words.len() as u64 } else { 1 };
            let remainder = if normalize { divide(&mut positive, divisor) } else { 0 };
            pack(&positive, -1074, remainder, divisor, false)
        }
        Ordering::Less => {
            subtract(&mut negative, &positive);
            let divisor = if normalize { words.len() as u64 } else { 1 };
            let remainder = if normalize { divide(&mut negative, divisor) } else { 0 };
            pack(&negative, -1074, remainder, divisor, true)
        }
    }
}

fn product_word(words: &[f64]) -> u64 {
    if words.is_empty() { return ONE; }
    if words.len() == 1 { return words[0].to_bits(); }
    let mut magnitude = [0_u64; 33];
    magnitude[0] = 1;
    let mut exponent_sum = 0_i32;
    let mut sign = 0_u64;
    let mut infinity = false;
    let mut zero = false;
    for &value in words {
        let word = value.to_bits();
        let exponent = (word >> 52) & 0x7ff;
        let fraction = word & FRACTION;
        sign ^= word & SIGN;
        if exponent == 0x7ff {
            if fraction != 0 { return word; }
            infinity = true;
            continue;
        }
        let significand = if exponent == 0 { fraction } else { (1 << 52) | fraction };
        if significand == 0 { zero = true; continue; }
        multiply(&mut magnitude, significand);
        exponent_sum += if exponent == 0 { -1074 } else { exponent as i32 - 1075 };
    }
    if infinity { return if zero { NAN } else { sign | INF }; }
    if zero { return sign; }
    pack(&magnitude, exponent_sum, 0, 1, sign != 0)
}
