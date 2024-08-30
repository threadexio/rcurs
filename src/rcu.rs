use core::{marker::PhantomData, ops::Deref};

use alloc::boxed::Box;

use portable_atomic::{AtomicPtr, Ordering};

use crate::refs::Refs;

struct Inner<T> {
	/// The number of active references to the specific `Inner`.
	refs: Refs,
	/// The data.
	data: T,
}

/// The RCU implementation.
pub struct Rcu<T> {
	ptr: AtomicPtr<Inner<T>>,
}

impl<T> Rcu<T> {
	/// Create a new [`Rcu`] with an initial value of `data`.
	pub fn new(data: T) -> Self {
		let ptr = alloc(Inner { data, refs: Refs::one() });
		Self { ptr: AtomicPtr::new(ptr) }
	}

	/// Update the value inside the [`Rcu`] and return the old one.
	///
	/// The new value will be immediately available to [`get`] calls _before_
	/// [`update`] returns. You must make sure that when calling this function
	/// the new value is fully initialized beforehand.
	///
	/// This function does _not_ block execution.
	///
	/// [`get`]: Self::get
	/// [`update`]: Self::update
	pub fn update(&self, new: T) {
		let new_ptr = alloc(Inner { data: new, refs: Refs::one() });
		let old_ptr = self.ptr.swap(new_ptr, Ordering::Relaxed);
		unsafe { drop_inner(old_ptr) };
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
	pub fn get(&self) -> Guard<'_, T> {
		let inner = self.ptr.load(Ordering::Relaxed).cast_const();
		unsafe { (*inner).refs.take_ref() };
		Guard { _marker: PhantomData, inner }
	}
}

impl<T> Drop for Rcu<T> {
	fn drop(&mut self) {
		unsafe { drop_inner(self.ptr.load(Ordering::Relaxed)) };
	}
}

unsafe impl<T> Sync for Rcu<T> {}
unsafe impl<T> Send for Rcu<T> {}

/// The RAII guard returned by [`Rcu`].
///
/// See: [`Rcu::get`].
pub struct Guard<'a, T> {
	_marker: PhantomData<&'a ()>,
	inner: *const Inner<T>,
}

impl<'a, T> Deref for Guard<'a, T> {
	type Target = T;

	fn deref(&self) -> &Self::Target {
		unsafe { &(*self.inner).data }
	}
}

impl<'a, T> Drop for Guard<'a, T> {
	fn drop(&mut self) {
		unsafe { drop_inner(self.inner.cast_mut()) };
	}
}

unsafe impl<T> Sync for Guard<'_, T> {}
unsafe impl<T> Send for Guard<'_, T> {}

/// Release a ref from `x` and drop it if there are no more refs.
unsafe fn drop_inner<T>(x: *mut Inner<T>) {
	if (*x).refs.release_ref() {
		free(x);
	}
}

fn alloc<T>(x: T) -> *mut T {
	Box::into_raw(Box::new(x))
}

unsafe fn free<T>(x: *mut T) {
	drop(Box::from_raw(x));
}

#[cfg(all(test, feature = "std"))]
mod tests {
	use super::*;

	use std::thread::{scope, sleep};
	use std::time::Duration;

	type UserRcu = Rcu<User>;

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
			user.update(User::B);
		});
	}
}
