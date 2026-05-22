// Copyright 2026
// This file is licensed as MIT.
//
// Implementation of itoa (integer to ASCII), which converts an 64-bit to
// decimal string. This implementation focus on smaller code footprint instead
// raw performance, no lookup tables or caching, ideal for no_std environments
// like wasm32-unknown-unknown and embeeded.
//
// @author Lohann Paterno Coutinho Ferreira <developer@lohann.dev>
#![cfg_attr(not(test), no_std)]

use ::core::{num::NonZeroU8, ptr::NonNull, result::Result, str};

/// Writes the decimal represetation of an an 64-bit unsigned integer to
/// decimal to buffer. Returns a pointer to the start of the string.
///
/// # SAFETY
///
/// caller must guarantee `end` points to end of the buffer and is has
/// sufficient space to store `x` decimal digits.
#[inline]
#[must_use]
pub const unsafe fn itoa_unchecked(mut x: u64, end: NonNull<u8>) -> NonNull<u8> {
    // write digit by digit to buffer.
    let mut ptr = end;
    loop {
        unsafe {
            ptr = ptr.sub(1);
            ptr.write(b'0'.wrapping_add((x % 10) as u8));
        };
        x = x.div_euclid(10);
        if x == 0 {
            break ptr;
        }
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

    let mut digits = NonZeroU8::MIN;
    if x >= 10_000_000_000 {
        x = x.div_euclid(10_000_000_000);
        digits = digits.saturating_add(10u8);
    }
    if x >= 100_000 {
        x = x.div_euclid(100_000);
        digits = digits.saturating_add(5u8);
    }
    // Value of top bits:
    //                +c1  +c2  1&2  +c3  +c4  3&4   ^
    //         0..=9  010  011  010  110  011  010  000 = 0
    //       10..=99  011  011  011  110  011  010  001 = 1
    //     100..=999  011  100  000  110  011  010  010 = 2
    //   1000..=9999  011  100  000  111  011  011  011 = 3
    // 10000..=99999  011  100  000  111  100  100  100 = 4
    x = (((x + C1) & (x + C2)) ^ ((x + C3) & (x + C4))) >> 17;
    digits.saturating_add((x & 0b0111) as u8)
}

/// Shrinks a mutable slice to the minimum between `slice.len()` and `max_len`.
#[inline]
#[must_use]
const fn shrink_to<T>(slice: &mut [T], max_len: usize) -> &mut [T] {
    // Rust borrow checker doesn’t understand disjoint in slices, shrink it in
    // a const fn needs unsafe code.
    // Referece: <https://doc.rust-lang.org/nomicon/borrow-splitting.html>
    if max_len <= slice.len() {
        // SAFETY: `[ptr; mid]` and `[mid; len]` are inside `slice`, which
        // fulfills the requirements of `split_at_unchecked`.
        unsafe { slice.split_at_mut_unchecked(max_len).0 }
    } else {
        slice
    }
}

/// Writes the decimal represetation of an 64-bit unsigned integer to the
/// output buffer, returns the decimal string slice on success.
///
/// # Errors
///
/// Returns `Err` when the decimal string doesn't fit inside the output buffer.
/// The returned `Err` provides the total number of bytes (or digits) required.
/// On failure the first `out.len()` decimal digits are written to the output
/// buffer.
/// 
/// # Examples
/// 
/// ```
/// use tiny_itoa::itoa;
/// 
/// let mut buffer = [0u8; 20];
/// let n: &mut str = itoa(123_456_678, &mut buffer).unwrap();
/// println!("n: {n}"); // output: 12345678
/// ```
#[inline]
#[allow(clippy::cast_possible_truncation)]
pub const fn itoa(mut x: u64, out: &mut [u8]) -> Result<&mut str, NonZeroU8> {
    // Compute the number of decimal digits of `x`
    let digits = decimal_digits(x);

    // Shrinks the output buffer so it have at maximum `digits` in length, we
    // need to know where it ends because digits are written in inverse order.
    let out = shrink_to(out, digits.get() as usize);

    // Return the number of digits as `Result::Err` when `out.is_empty()`
    let Some(mut len) = NonZeroU8::new(out.len() as u8) else {
        return Result::Err(digits);
    };

    // Truncate last digits when `out.len() < digits`.
    while len.get() < digits.get() {
        x = x.div_euclid(10);
        len = len.saturating_add(1);
    }

    // SAFETY: We guaranteed that `out.len() > 0` and all digits fits inside
    // the buffer.
    let _ = unsafe {
        let end = out.as_mut_ptr().add(out.len());
        itoa_unchecked(x, NonNull::new_unchecked(end))
    };

    if out.len() >= digits.get() as usize {
        // SAFETY: The `itoa_unchecked` only writes valid utf8 decimal digits.
        let string = unsafe { str::from_utf8_unchecked_mut(out) };
        Result::Ok(string)
    } else {
        Result::Err(digits)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::itoa;
    use ::core::ffi::CStr;

    #[test]
    fn itoa_works() {
        let test_cases = [0u64, 1u64, 123_456_789, u64::MAX];
        let mut buffer = [0u8; 22];

        // Run for each number
        for value in test_cases {
            let expected = format!("{value}");
            let slice = expected.as_str();
            let str_len = slice.len();

            // Try different output buffer sizes
            for i in 0..buffer.len() {
                buffer.fill(0);
                let Some(buf) = buffer.get_mut(0..i) else {
                    break;
                };
                let buf_len = buf.len();
                let expected_len = usize::min(str_len, buf_len);
                let decimal = {
                    match itoa(value, buf) {
                        Ok(s) => {
                            assert_eq!(s, slice);
                            assert!(buf_len >= str_len);
                        }
                        Err(len) => {
                            assert_eq!(len.get() as usize, str_len);
                            assert!(buf_len < str_len);
                        }
                    }
                    CStr::from_bytes_until_nul(&buffer[0..])
                        .unwrap()
                        .to_str()
                        .unwrap()
                };
                assert_eq!(decimal, &slice[0..expected_len as usize]);
            }
        }
    }
}
