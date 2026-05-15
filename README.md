## tiny-itoa

A zero-allocation, minimal, panic free integer to decimal implementation.

### Features
- `no_std`/WASM compatible.
- Zero heap allocations.
- Panic-free.
- Simple, no static memory or lookup tables.

### Alternatives
Consider [dtolnay/itoa](https://github.com/dtolnay/itoa/tree/1.0.18) if you prioritize speed over binary size, it uses of [lookup tables](https://github.com/dtolnay/itoa/blob/1.0.18/src/lib.rs#L221-L228) and other techniques to achieve performance. **tiny-itoa** instead is inspired by [musl](https://github.com/kraj/musl/blob/v1.2.6/src/stdlib/atoi.c) philosophy and provides a simple straigh-forward implementation, ideal for `wasm32-unknown-unknown` or embeeded targets.
