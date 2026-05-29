//! [![github]](https://github.com/Lohann/tiny-itoa)&ensp;[![crates-io]](https://crates.io/crates/tiny-itoa)&ensp;[![docs-rs]](https://docs.rs/tiny-itoa)
//!
//! [github]: https://img.shields.io/badge/github-8da0cb?style=for-the-badge&labelColor=555555&logo=github
//! [crates-io]: https://img.shields.io/badge/crates.io-fc8d62?style=for-the-badge&labelColor=555555&logo=rust
//! [docs-rs]: https://img.shields.io/badge/docs.rs-66c2a5?style=for-the-badge&labelColor=555555&logo=docs.rs
//!
//! <br>
//!
//! This crate provides zero-allocation, minimal, panic free integer primitives to decimal
//! decimal strings, but avoid performance penalty of going through [`core::fmt::Formatter`]
//! and uses less static storage and code compared to [itoa], ideal for `wasm32-unknown-unknown`
//! and embedded targets.
//!
//! [libcore]: https://github.com/rust-lang/rust/blob/1.92.0/library/core/src/fmt/num.rs#L190-L253
//! [itoa]: https://github.com/dtolnay/itoa
//!
//! # Example
//!
//! ```
//! use tiny_itoa::itoa;
//! use std::ffi::CStr;
//!
//! fn main() {
//!     let mut buffer = [0u8; 100];
//!     {
//!         let (x, rest) = itoa(123_456_789, &mut buffer[..]).unwrap();
//!         rest[0] = b'-';
//!         let (y, _) = itoa(u64::MAX, &mut rest[1..]).unwrap();
//!         assert_eq!(x, "123456789");
//!         assert_eq!(y, "18446744073709551615");
//!     }
//!     let ptr = buffer.as_ptr();
//!     let z = unsafe { CStr::from_ptr(ptr.cast()) };
//!     assert_eq!(z.to_string_lossy(), "123456789-18446744073709551615");
//! }
//! ```

// Copyright 2026
// This file is licensed as MIT.
//
// Implementation of itoa (integer to ASCII), which converts an 64-bit to
// decimal string. This implementation focus on smaller code footprint instead
// raw performance, no lookup tables or caching, ideal for no_std environments
// like wasm32-unknown-unknown and embeeded.
//
// @author Lohann Paterno Coutinho Ferreira <developer@lohann.dev>
#![allow(clippy::inline_always)]
#![cfg_attr(not(test), no_std)]

use ::const_fn::const_fn;
use ::core::{num::NonZeroUsize, result::Result, str};

// slice.split_at_mut_unchecked is only available after rust 1.79.0
#[rustversion::before(1.79.0)]
macro_rules! split_at_mut_unchecked {
    ($slice:ident, $mid:ident) => {
        unsafe {
            let ptr = $slice.as_mut_ptr();
            (
                ::core::slice::from_raw_parts_mut(ptr, $mid),
                ::core::slice::from_raw_parts_mut(ptr.add($mid), $slice.len() - $mid),
            )
        }
    };
}
#[rustversion::since(1.79.0)]
macro_rules! split_at_mut_unchecked {
    ($slice:ident, $mid:ident) => {
        unsafe { $slice.split_at_mut_unchecked($mid) }
    };
}

/// Compute the numbers of decimal digits of `x`.
#[inline]
#[must_use]
const fn decimal_digits(mut x: u64) -> NonZeroUsize {
    // For better performance, avoid branches by assembling the solution
    // we get two possible bit patterns above the low 17 bits,
    // depending on whether val is below or above the threshold.
    const C1: u64 = 0b011_00000000000000000 - 10; // 393206
    const C2: u64 = 0b100_00000000000000000 - 100; // 524188
    const C3: u64 = 0b111_00000000000000000 - 1000; // 916504
    const C4: u64 = 0b100_00000000000000000 - 10000; // 514288

    let mut digits = 1;
    if x >= 10_000_000_000 {
        x = x.div_euclid(10_000_000_000);
        digits += 10;
    }
    if x >= 100_000 {
        x = x.div_euclid(100_000);
        digits += 5;
    }

    // Value of top bits:
    //                +c1  +c2  1&2  +c3  +c4  3&4   ^
    //         0..=9  010  011  010  110  011  010  000 = 0
    //       10..=99  011  011  011  110  011  010  001 = 1
    //     100..=999  011  100  000  110  011  010  010 = 2
    //   1000..=9999  011  100  000  111  011  011  011 = 3
    // 10000..=99999  011  100  000  111  100  100  100 = 4
    x = (((x + C1) & (x + C2)) ^ ((x + C3) & (x + C4))) >> 17;
    digits += (x & 0b0111) as usize;

    // SAFETY: digits is a value between 1 and 20
    unsafe { NonZeroUsize::new_unchecked(digits) }
}

/// Shrinks a mutable slice to the minimum between `slice.len()` and `max_len`.
#[inline]
#[must_use]
#[allow(clippy::incompatible_msrv)]
#[const_fn("1.83")]
const fn shrink_to<T>(slice: &mut [T], max_len: usize) -> (&mut [T], &mut [T]) {
    // Rust borrow checker doesn’t understand disjoint in slices, shrink it in
    // a const fn needs unsafe code.
    // Referece: <https://doc.rust-lang.org/nomicon/borrow-splitting.html>
    if slice.len() > max_len {
        // SAFETY: `[ptr; mid]` and `[mid; len]` are inside `slice`, which
        // fulfills the requirements of `split_at_unchecked`.
        split_at_mut_unchecked!(slice, max_len)
    } else {
        (slice, &mut [])
    }
}

/// Same as [`tiny_itoa::itoa`][itoa], but with the `#[inline(always)]`
/// attribute. So the caller can choose when to force inline or leave
/// to the compiler decide.
#[doc(hidden)]
#[inline(always)]
#[const_fn("1.83")]
pub const fn itoa_inline(
    mut x: u64,
    out: &mut [u8],
) -> Result<(&mut str, &mut [u8]), NonZeroUsize> {
    // Compute the number of decimal digits of `x`
    let digits = decimal_digits(x);

    // Shrinks the output buffer so it have at maximum `digits` in length, we
    // need to know where it ends because digits are written in inverse order.
    let (out, rest) = shrink_to(out, digits.get());

    // Return the number of digits as `Result::Err` when `out.is_empty()`
    let len = if let Some(len) = NonZeroUsize::new(out.len()) {
        len
    } else {
        return Err(digits);
    };

    // Truncate last digits when `out.len() < digits`.
    {
        let mut len = len.get();
        while len < digits.get() {
            x = x.div_euclid(10);
            len += 1;
        }
    }

    // Truncate last digits when `out.len() < digits`.
    let mut next = unsafe { &mut *(out as *mut [u8]) };
    while let [tail @ .., last] = next {
        next = tail;
        *last = (x % 10) as u8 + b'0';
        x /= 10;
    }

    if len.get() >= digits.get() {
        // SAFETY: The `itoa_unchecked` only writes valid utf8 decimal digits.
        let string = unsafe { &mut *(out as *mut [u8] as *mut str) };
        Ok((string, rest))
    } else {
        Err(digits)
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
/// let mut buffer = [0u8; 40];
/// let (x, rest) = itoa(123_456_789, &mut buffer[..]).unwrap();
/// let (y, _) = itoa(u64::MAX, rest).unwrap();
/// assert_eq!(x, "123456789");
/// assert_eq!(y, "18446744073709551615");
/// ```
#[const_fn("1.83")]
pub const fn itoa(x: u64, out: &mut [u8]) -> Result<(&mut str, &mut [u8]), NonZeroUsize> {
    itoa_inline(x, out)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::itoa;
    use ::std::ffi::CStr;

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
                let buf = buffer.get_mut(0..i).unwrap();
                let buf_len = buf.len();
                let expected_len = usize::min(str_len, buf_len);
                let decimal = {
                    let len = match itoa(value, buf) {
                        Ok((s, _)) => {
                            assert_eq!(s, slice);
                            assert!(buf_len >= str_len);
                            s.len() + 1
                        }
                        Err(len) => {
                            assert_eq!(len.get(), str_len);
                            assert!(buf_len < str_len);
                            buf_len + 1
                        }
                    };
                    CStr::from_bytes_with_nul(&buffer[0..len])
                        .unwrap()
                        .to_str()
                        .unwrap()
                };
                assert_eq!(decimal, &slice[0..expected_len as usize]);
            }
        }
    }
}
