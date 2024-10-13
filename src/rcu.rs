use core::marker::PhantomData;
use core::ops::Deref;

use alloc::boxed::Box;

use portable_atomic::{AtomicPtr, Ordering};

use crate::refs::Refs;
use crate::spin::Spinlock;

struct Inner<T> {
	/// The number of active references to the specific `Inner`.
	refs: Refs,
	/// The data.
	data: T,
}

impl<T> Inner<T> {
	fn new(data: T) -> *mut Self {
		alloc(Self { refs: Refs::new(), data })
	}
}

/// The RCU implementation.
pub struct Rcu<T> {
	ptr: AtomicPtr<Inner<T>>,

	// A lock that protects `ptr` from being free'd before its ref count can be
	// incremented.
	lock: Spinlock,
}

impl<T> Rcu<T> {
	/// Create a new [`Rcu`] with an initial value of `data`.
	pub fn new(data: T) -> Self {
		Self {
			ptr: AtomicPtr::new(Inner::new(data)),
			lock: Spinlock::new(),
		}
	}

	/// Update the value inside the [`Rcu`] and return the old one.
	///
	/// The new value will be immediately available to [`get`] calls _before_
	/// [`update`] returns. You must make sure that when calling this function
	/// the new value is fully initialized beforehand.
	///
	/// [`get`]: Self::get
	/// [`update`]: Self::update
	pub fn update(&self, new: T) {
		let new_ptr = Inner::new(new);
		let old_ptr = self.ptr.swap(new_ptr, Ordering::Relaxed);
		// From this point and on, no new references to `old_ptr` can be created.

		// If any other thread is executing `Rcu::get` with the old pointer and it has not
		// incremented the old ref count, we wait. There is a possibility that this waits
		// for threads executing `Rcu::get` after the atomic swap of `self.ptr` (which is
		// unnecessary), but the cost is very minimal.
		self.lock.with(|| {});
		drop_reclaim_inner(old_ptr);
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
		// Getting a reference to the value inside the RCU is a 2 step process. First, you
		// have to read the pointer to the `Inner` struct (which holds the value). Afterwards,
		// you need to dereference that pointer and increment the ref count (also inside
		// the `Inner` struct).

		// In some rare cases it is possible that the thread trying to get the value reads
		// the pointer to the `Inner` struct and in the time it takes to dereference the
		// pointer and increment the ref count (since this operation is not atomic) another
		// thread `update`s the value and deallocates the `Inner` struct since it sees that
		// it has no references. However, the thread getting the value has read the
		// now-deallocated pointer and tries to dereference it and disaster in the form of
		// a use-after-free bug occurs. For this reason, we use a simple spinlock that
		// protects the time frame between reading the pointer and incrementing the ref
		// count. This time frame is very short (as long as it takes the CPU to do a memory
		// dereference and an atomic increment), so any other thread should never spin for
		// long.
		//
		// See issue #1: https://github.com/threadexio/rcurs/issues/1
		let ptr = self.lock.with(|| unsafe {
			let ptr = self.ptr.load(Ordering::Relaxed);

			// See: `tests::test_issue_1_rcu_race_in_get`
			#[cfg(test)]
			std::thread::sleep(std::time::Duration::from_millis(150));

			(*ptr).refs.take_ref();
			ptr
		});

		Guard::new(ptr)
	}
}

impl<T> Drop for Rcu<T> {
	fn drop(&mut self) {
		let ptr = self.ptr.load(Ordering::Relaxed);

		// See the comment in `Rcu::update`.
		self.lock.with(|| {});
		drop_reclaim_inner(ptr);
	}
}

unsafe impl<T> Sync for Rcu<T> {}
unsafe impl<T> Send for Rcu<T> {}

fn drop_reclaim_inner<T>(ptr: *mut Inner<T>) -> T {
	unsafe {
		let inner = &*ptr;

		// TODO: Find some better way to wait until all references have been dropped.
		while inner.refs.count() > 1 {
			core::hint::spin_loop();
		}

		let Inner { data, .. } = dealloc(ptr);
		data
	}
}

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

impl<'a, T> Guard<'a, T> {
	fn new(inner: *const Inner<T>) -> Self {
		Self { _marker: PhantomData, inner }
	}
}

impl<'a, T> Drop for Guard<'a, T> {
	fn drop(&mut self) {
		unsafe {
			let ptr = self.inner.cast_mut();

			// SAFETY: When this `Guard` was created a reference was taken, we now need to
			//         give back that reference. We are releasing our own reference here.
			let _ = (*ptr).refs.release_ref();

			// It is not the `Guard`'s responsibility to free the underlying memory,
			// so we don't have to do anything else.
		}
	}
}

unsafe impl<T> Sync for Guard<'_, T> {}
unsafe impl<T> Send for Guard<'_, T> {}

fn alloc<T>(x: T) -> *mut T {
	Box::into_raw(Box::new(x))
}

unsafe fn dealloc<T>(x: *mut T) -> T {
	*Box::from_raw(x)
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

	// Issue: https://github.com/threadexio/rcurs/issues/1
	#[test]
	fn test_issue_1_rcu_race_in_get() {
		let rcu = Rcu::new(42);

		scope(|scope| {
			scope.spawn(|| {
				// We need `get` to delay incrementing the ref count in order to give time
				// to the other thread to update the value and allow the race to occur.
				let val = rcu.get();
				assert_eq!(*val, 42);
			});

			scope.spawn(|| {
				sleep(Duration::from_millis(100));
				rcu.update(32);
			});
		});
	}
}
