[`core::hint::spin_loop()`]: https://doc.rust-lang.org/stable/core/hint/fn.spin_loop.html
[`Condvar`]: https://doc.rust-lang.org/stable/std/sync/struct.Condvar.html

# rcurs

A simple [RCU](https://en.wikipedia.org/wiki/Read-copy-update) with an oxidized interface. Read more at the [docs](https://docs.rs/rcurs).

The crate supports running both with or without the `std` library but has a hard dependency on `alloc`. If your environment allows, you should try to keep the `std` feature enabled as that contains typically more efficient implementations of blocking primitives.

Without the `std` feature, the only way to block is to spin in place using whatever optimization [`core::hint::spin_loop()`] can provide. But with the standard library, blocking is done using [`Condvar`]s. [`Condvar`]s call out to the kernel for blocking. The kernel can then choose what is best, spin itself, or usually give control back to the scheduler to run other processes.

## Features

- `std`: Enable use of primitives in the standard library
