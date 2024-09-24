use portable_atomic::{AtomicBool, Ordering};

/// A lock that can be acquired by only one thread at a time.
///
/// This lock does not implement poisoning. Panicking while the lock is held will leave the
/// lock in a permanent locked state, unless manually [`unlock`]ed. The lock makes no attempt
/// to lock down the API and enforce that locking and unlocking must be done by the same thread.
pub struct Spinlock {
	locked: AtomicBool,
}

impl Spinlock {
	/// Create a new lock that is in the unlocked state.
	pub const fn new() -> Self {
		Self { locked: AtomicBool::new(false) }
	}

	/// Lock the data structure.
	///
	/// This method is a low-level building block of more complex functionality. In general,
	/// [`lock_guard`] and [`with`] should be preferred.
	///
	/// This method marks the lock as currently locked and causes any other threads wanting
	/// to obtain the lock to spin. As such, if the lock is already held this method will
	/// spin until it is released. In cases of lock contention, the order of which threads
	/// will obtain the lock is undefined and can be safely assumed as random.
	///
	/// [`lock_guard`]: Self::lock_guard
	/// [`with`]: Self::with
	pub unsafe fn lock(&self) {
		while self
			.locked
			.compare_exchange(
				false,
				true,
				Ordering::Release,
				Ordering::Relaxed,
			)
			.is_err()
		{
			core::hint::spin_loop();
		}
	}

	/// Unlock the data structure.
	///
	/// This method releases the lock and makes it available for other thread to lock. It
	/// is good practice to avoid using [`unlock`] without a previous call to [`lock`].
	///
	/// [`lock`]: Self::lock
	/// [`unlock`]: Self::unlock
	pub unsafe fn unlock(&self) {
		self.locked.store(false, Ordering::Release);
	}

	/// Lock the data structure and obtain a guard that, when dropped, will automatically
	/// [`unlock`] it.
	///
	/// [`unlock`]: Self::unlock
	pub const fn lock_guard(&self) -> Guard<'_> {
		Guard::new(self)
	}

	/// Lock the data structure for the duration of _f_.
	pub fn with<O>(&self, f: impl FnOnce() -> O) -> O {
		unsafe {
			self.lock();
			let output = f();
			self.unlock();
			output
		}
	}
}

/// RAII guard for [`Spinlock`].
pub struct Guard<'a> {
	lock: &'a Spinlock,
}

impl<'a> Guard<'a> {
	const fn new(lock: &'a Spinlock) -> Self {
		Self { lock }
	}
}

impl<'a> Drop for Guard<'a> {
	fn drop(&mut self) {
		unsafe { self.lock.unlock() }
	}
}
