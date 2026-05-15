// Copyright 2026
// This file is licensed as MIT.
//
// Implementation of itoa (integer to ASCII), which converts an 64-bit to
// decimal string. This implementation focus on smaller code footprint instead
// raw performance, no lookup tables or caching, ideal for no_std environments
// like wasm32-unknown-unknown or embeeded.
//
// @author Lohann Paterno Coutinho Ferreira <developer@lohann.dev>
#![cfg_attr(not(test), no_std)]

use ::core::{ptr::NonNull, num::NonZeroU8};

/// Writes the decimal represetation of an an 64-bit unsigned integer to decimal
/// to buffer.
///
/// # SAFETY
/// caller must guarantee `end` is the end of the buffer, and it has sufficient
/// space to convert the string..
#[inline]
#[must_use]
pub const unsafe fn itoa_unchecked(mut x: u64, end: NonNull<u8>) -> usize {
    // write digit by digit to buffer.
	let mut ptr = end;
    loop {
        unsafe {
            ptr = ptr.sub(1);
            ptr.write(b'0'.wrapping_add((x % 10) as u8));
        };
        x = x.div_euclid(10);
        if x == 0 { return unsafe { end.byte_offset_from_unsigned(ptr) }; }
    }
}

/// Compute the numbers of decimal digits of `x`.
#[inline]
#[must_use]
const fn decimal_digits(mut x: u64) -> NonZeroU8 {
    // For better performance, avoid branches by assembling the solution
    // we get two possible bit patterns above the low 17 bits,
    // depending on whether val is below or above the threshold.
    const C1: u64 = 0b011_00000000000000000 - 10; // 393206
    const C2: u64 = 0b100_00000000000000000 - 100; // 524188
    const C3: u64 = 0b111_00000000000000000 - 1000; // 916504
    const C4: u64 = 0b100_00000000000000000 - 10000; // 514288

    let mut digits = 1u8;
    if x >= 10_000_000_000 {
        x /= 10_000_000_000;
        digits += 10u8;
    }
    if x >= 100_000 {
        x /= 100_000;
        digits += 5u8;
    }
    // Value of top bits:
    //                +c1  +c2  1&2  +c3  +c4  3&4   ^
    //         0..=9  010  011  010  110  011  010  000 = 0
    //       10..=99  011  011  011  110  011  010  001 = 1
    //     100..=999  011  100  000  110  011  010  010 = 2
    //   1000..=9999  011  100  000  111  011  011  011 = 3
    // 10000..=99999  011  100  000  111  100  100  100 = 4
    x = (((x + C1) & (x + C2)) ^ ((x + C3) & (x + C4))) >> 17;
    digits += (x & 0b0111) as u8;
    unsafe { NonZeroU8::new_unchecked(digits) }
}


/// Compute min(x, y) without branching
/// ref: <https://graphics.stanford.edu/~seander/bithacks.html#IntegerMinOrMax>
#[inline]
#[must_use]
const fn branchless_min(mut x: usize, mut y: usize) -> (usize, usize) {
    x = x.wrapping_sub(y);
    let mask = x.cast_signed().wrapping_shr(isize::BITS - 1).cast_unsigned();
    x &= mask;
    y = y.wrapping_add(x);
    (mask, y)
}

/// base implementation which verifies output bondaries.
///
/// # SAFETY
/// caller must guarantee `ptr` is valid address and have up to `len` in size.
#[inline]
#[must_use]
#[allow(clippy::cast_possible_truncation)]
pub const unsafe fn itoa_raw(mut x: u64, ptr: *mut u8, len: usize) -> isize {
    // Compute the number of decimal digits of `x`
    let digits = decimal_digits(x);

    // find where the fist digit starts by computing:
    let (mut mask, mut len) = branchless_min(len, digits.get() as usize);
    let end: *mut u8 = unsafe { ptr.add(len) };
    len = (len ^ mask).wrapping_sub(mask);
    let mut digits = (digits.get() as usize ^ mask).wrapping_sub(mask);

    {
        let mut mask = unsafe { ptr.byte_offset_from(::core::ptr::null::<u8>()).cast_unsigned() };
        mask = mask.wrapping_sub(1);
        mask |= unsafe { end.byte_offset_from(::core::ptr::null::<u8>()).cast_unsigned() };
        mask = !(mask.cast_signed().wrapping_shr(isize::BITS - 1).cast_unsigned());
        digits &= mask;
        len &= mask;
    }
    
    if len == 0 { return digits.cast_signed(); }

    // skip least significant digits when `len < digits`
    mask |= 1;
    while len != digits {
        x = x.div_euclid(10);
        len = len.wrapping_add(mask);
    }
    let _ = unsafe { itoa_unchecked(x, NonNull::new_unchecked(end)) };
    digits.cast_signed()
}

#[must_use]
pub const fn itoa(x: u64, output: &mut [u8]) -> isize {
	// SAFETY: Rust guarantees output ptr and len are valid.
	unsafe {
		itoa_raw(x, output.as_mut_ptr(), output.len())
	}
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::{itoa, itoa_raw, branchless_min};
    use ::core::ffi::CStr;

    #[test]
    fn branchless_min_works() {
        for x in 0..=255usize {
            for y in 0..=255usize {
                let mask = if x < y { usize::MAX} else { 0 };
                assert_eq!(branchless_min(x, y), (mask, usize::min(x, y)));
            }
        }
    }

    #[test]
    fn itoa_raw_works() {
        let test_cases = [0u64, 1u64, 123_456_789, u64::MAX];
        let mut buffer = [0u8; 22];
        for value in test_cases {
            let expected = format!("{value}");
            let slice = expected.as_str();
            let str_len = slice.len();
            for i in 0..buffer.len() {
                buffer.fill(0);
                let Some(buf) = buffer.get_mut(0..i) else {
                    break;
                };
                let buf_len = buf.len();
                let expected_len = usize::min(str_len, buf_len);
                let decimal = unsafe {
                    let ptr = buf.as_mut_ptr();
                    let mut str_len = str_len.cast_signed();
                    if buf_len < str_len.cast_unsigned() {
                        str_len = -str_len;
                    }
                    assert_eq!(itoa_raw(value, ptr, buf_len), str_len);
                    let res = itoa_raw(value, ptr, buf_len);
                    assert_eq!(res, str_len, "itoa_raw({value}, {buf_len}) | {res} != {str_len}");
                    CStr::from_bytes_until_nul(&buffer[0..]).unwrap().to_str().unwrap()
                };
                assert_eq!(decimal, &slice[0..expected_len as usize]);
            }
        }
    }

	#[test]
    fn itoa_works() {
        let test_cases = [0u64, 1u64, 123_456_789, u64::MAX];
        let mut buffer = [0u8; 22];
        for value in test_cases {
            let expected = format!("{value}");
            let slice = expected.as_str();
            let str_len = slice.len();
            for i in 0..buffer.len() {
                buffer.fill(0);
                let Some(buf) = buffer.get_mut(0..i) else {
                    break;
                };
                let buf_len = buf.len();
                let expected_len = usize::min(str_len, buf_len);
                {
                    let mut str_len = str_len.cast_signed();
                    if buf_len < str_len.cast_unsigned() {
                        str_len = -str_len;
                    }
                    let res = itoa(value, buf);
                    assert_eq!(res, str_len, "itoa({value}, {buf_len}) | {res} != {str_len}");
                }
                let decimal = CStr::from_bytes_until_nul(&buffer[0..]).unwrap().to_str().unwrap();
                assert_eq!(decimal, &slice[0..expected_len as usize]);
            }
        }
    }
}
