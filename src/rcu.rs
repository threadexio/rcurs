extern crate alloc;

use core::ops::Deref;
use core::sync::atomic::{AtomicPtr, AtomicUsize, Ordering};

use alloc::boxed::Box;

use crate::Notify;

/// The RCU implementation.
pub struct Rcu<T, N>
where
	N: Notify,
{
	ptr: AtomicPtr<Inner<T, N>>,
}

impl<T, N> Rcu<T, N>
where
	N: Notify,
{
	/// Create a new [`Rcu`] with an initial value of `data`.
	pub fn new(data: T) -> Self {
		let ptr = Inner::new(data, N::new()).into_owned_ptr();

		Self { ptr: AtomicPtr::new(ptr) }
	}

	/// Update the value inside the [`Rcu`] and return the old one.
	///
	/// The new value will be immediately available to [`get`] calls _before_
	/// [`update`] returns. You must make sure that when calling this function
	/// the new value is fully initialized beforehand.
	///
	/// This function _will_ block execution until all guards referring to the
	/// old value are dropped.
	///
	/// [`get`]: Self::get
	/// [`update`]: Self::update
	pub fn update(&self, new: T) -> T {
		let new_ptr = Inner::new(new, N::new()).into_owned_ptr();

		let old_ptr = self.ptr.swap(new_ptr, Ordering::Relaxed);
		// Any new refs past this point reference the new data.

		/* SAFETY:
		 * We can't call from_owned_ptr right away because that function
		 * will move Inner in memory and invalidate any references that
		 * might still exist.
		 */
		let old_ref = unsafe { &*old_ptr };

		// Notify must be called only when there are no more refs.
		old_ref.notify.wait();

		// Now it is safe to move Inner as no references to it exist.
		let old = Inner::from_owned_ptr(old_ptr);
		old.data
	}

	/// Get the value inside the [`Rcu`].
	///
	/// This function returns a RAII guard that automatically keeps track
	/// when you have stopped using the value.
	///
	/// If the value is [`update`]d while the guard is live, the guard does
	/// _not_ reference the new one. It keeps referencing the old one until
	/// it is dropped and a new guard is created. In simple terms, a guard
	/// "remembers" the value the [`Rcu`] had when the guard was created for
	/// its whole lifetime.
	///
	/// This function does _not_ block execution.
	///
	/// [`update`]: Self::update
	pub fn get(&self) -> Guard<'_, T, N> {
		let inner = unsafe { &*self.ptr.load(Ordering::Relaxed) };
		inner.refs.fetch_add(1, Ordering::Relaxed);
		Guard { inner }
	}
}

impl<T, N: Notify> Drop for Rcu<T, N> {
	fn drop(&mut self) {
		/* We must not forget to call `T`'s drop code when the RCU is
		 * actually dropped.
		 */
		let ptr = self.ptr.load(Ordering::Relaxed);
		drop(Inner::from_owned_ptr(ptr));
	}
}

unsafe impl<T, N: Notify> Sync for Rcu<T, N> {}
unsafe impl<T, N: Notify> Send for Rcu<T, N> {}

/// The RAII guard returned by [`Rcu`].
///
/// See: [`Rcu::get`].
pub struct Guard<'a, T, N>
where
	N: Notify,
{
	inner: &'a Inner<T, N>,
}

impl<'a, T, N> Deref for Guard<'a, T, N>
where
	N: Notify,
{
	type Target = T;

	fn deref(&self) -> &Self::Target {
		&self.inner.data
	}
}

impl<'a, T, N> Drop for Guard<'a, T, N>
where
	N: Notify,
{
	fn drop(&mut self) {
		let refs = self.inner.refs.fetch_sub(1, Ordering::Relaxed);

		/* This check is a bit unintuitive because `fetch_sub` returns the
		 * previous value before the subtraction. So in order to avoid
		 * having to do an extra `load`, we use the return value.
		 */
		if refs == 1 {
			self.inner.notify.notify();
		}
	}
}

unsafe impl<T, N: Notify> Sync for Guard<'_, T, N> {}
unsafe impl<T, N: Notify> Send for Guard<'_, T, N> {}

struct Inner<T, N>
where
	N: Notify,
{
	/// The number of active references to the specific `Inner`.
	refs: AtomicUsize,
	/// A simple notify utility. This will wake us up when told to do so.
	notify: N,
	/// The data.
	data: T,
}

impl<T, N> Inner<T, N>
where
	N: Notify,
{
	const fn new(data: T, notify: N) -> Self {
		Self { refs: AtomicUsize::new(0), notify, data }
	}

	fn into_owned_ptr(self) -> *mut Self {
		let boxed = Box::new(self);
		Box::into_raw(boxed)
	}

	fn from_owned_ptr(ptr: *mut Self) -> Self {
		let boxed = unsafe { Box::from_raw(ptr) };
		*boxed
	}
}

#[cfg(all(test, feature = "std"))]
mod tests {
	use super::*;

	use std::thread::{scope, sleep};
	use std::time::Duration;

	type UserRcu = Rcu<User, crate::notify::Blocking>;

	#[derive(Debug, PartialEq, Eq)]
	struct User {
		id: i32,
		name: &'static str,
	}

	impl User {
		const A: Self = Self { id: 1, name: "user 1" };

		const B: Self = Self { id: 2, name: "user 2" };
	}

	#[test]
	fn test_rcu() {
		fn routine<'a>(
			start_in: u64,
			run_for: u64,
			rcu: &'a UserRcu,
			expected: User,
		) -> impl FnOnce() + Send + 'a {
			const CHECK_COUNT: u32 = 5;

			move || {
				sleep(Duration::from_secs(start_in));

				let user = rcu.get();

				let t = Duration::from_secs(run_for) / CHECK_COUNT;
				for _ in 0..CHECK_COUNT {
					sleep(t);
					assert_eq!(*user, expected);
				}
			}
		}

		let user = Rcu::new(User::A);

		scope(|scope| {
			scope.spawn(routine(0, 10, &user, User::A));
			scope.spawn(routine(4, 15, &user, User::A));

			// Any readers past t=5 must see User::B
			scope.spawn(routine(6, 4, &user, User::B));
			scope.spawn(routine(8, 5, &user, User::B));
			scope.spawn(routine(10, 7, &user, User::B));

			sleep(Duration::from_secs(5));
			let old = user.update(User::B);
			assert_eq!(old, User::A);
		});
	}
}
