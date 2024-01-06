//! [`Rcu`] or Read-Copy-Update is a mechanism that allows sharing and updating
//! a piece of data while multiple parallel code blocks have access to it in a
//! lock-free manner. A better explanation is available at the [kernel docs](https://www.kernel.org/doc/html/latest/RCU/whatisRCU.html)
//! or even [Wikipedia](https://en.wikipedia.org/wiki/Read-copy-update).
//!
//! Internally, an RCU is composed of an _atomic_ pointer (the atomic part
//! is important) to some data along with the number of currently active
//! references to it. Something like a garbage-collector (but obviously way
//! simpler). So when you want to get at the actual data, you simply increment
//! the counter of references by one and decrement it when you are done. This
//! is the easy part, now how do we update the value without locking and
//! without messing up anyone already working? Well, each reference does not
//! keep a pointer to the RCU, but instead copies the value of the atomic
//! pointer when it is created. The key is that the pointer to the data is
//! atomic. So we simply create a new copy of data, make our changes to that
//! copy, and then atomically update the pointer. This way we ensure that a
//! reference that is created at the same time as we update it uses either:
//! the old value or the new value. In any case, it does not end up with an
//! invalid pointer. Because we are a model programmer, we must also not
//! forget to clean up the old data which is effectively outdated and useless.
//! But we can't just clean up the old data: there might be references
//! to it from before we did the update. Ah, so we will wait for the code
//! with old references to drop them, and because we swapped the pointer before,
//! no new references can get access to the old data. After waiting and ensuring
//! there are no remaining references to the old data, we can now safely free
//! it without worry.
//!
//! # Example
//!
//! ```rust,no_run
//! use std::thread;
//! use std::time::Duration;
//!
//! type Rcu<T> = rcurs::Rcu<T, rcurs::Spin>;
//!
//! #[derive(Debug, Clone, PartialEq, Eq)]
//! struct User {
//!     uid: i32,
//!     gid: i32,
//! }
//!
//! fn setugid(user: &Rcu<User>, uid: i32, gid: i32) {
//!     let mut new = user.get().clone();
//!
//!     if new.uid == uid && new.gid == gid {
//!         return;
//!     }
//!
//!     new.uid = uid;
//!     new.gid = gid;
//!
//!     user.update(new);
//! }
//!
//! // Basically a `sleep`` function that holds onto `user` and prints it after
//! // `sec` seconds have passed.
//! fn compute(user: &Rcu<User>, id: &str, sec: u64) {
//!     let user = user.get();
//!     thread::sleep(Duration::from_secs(sec));
//!     println!("compute[{id}]: finish work for {}:{}", user.uid, user.gid);
//! }
//!
//! fn thread_1(user: &Rcu<User>) {
//!     compute(user, "1", 3);
//!
//!     // This call will update `user`.
//!     setugid(user, 1000, 1000);
//!
//!     compute(user, "3", 4);
//! }
//!
//! fn thread_2(user: &Rcu<User>) {
//!     // The following compute call will always see the `User { uid: 0, gid: 0}`
//!     // as the `setugid` call happens 3 seconds after it has started executing.
//!     compute(user, "2", 5);
//! }
//!
//! fn main() {
//!     let user = Rcu::new(User { uid: 0, gid: 0 });
//!
//!     thread::scope(|scope| {
//!         scope.spawn(|| thread_1(&user));
//!         scope.spawn(|| thread_2(&user));
//!     });
//! }
//! ```
#![deny(missing_docs)]
#![warn(
	clippy::all,
	clippy::correctness,
	clippy::pedantic,
	clippy::cargo,
	clippy::nursery,
	clippy::perf,
	clippy::style
)]
#![allow(
	clippy::missing_panics_doc,
	clippy::significant_drop_tightening,
	clippy::needless_lifetimes
)]
#![cfg_attr(not(feature = "std"), no_std)]

mod cfg;

mod notify;
mod rcu;

#[doc(inline)]
pub use self::notify::*;

#[doc(inline)]
pub use self::rcu::{Guard, Rcu};
