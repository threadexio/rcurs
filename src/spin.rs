use portable_atomic::{AtomicBool, Ordering};

/// A lock that can be acquired by only one thread at a time.
pub struct Spinlock {
	locked: AtomicBool,
}

impl Spinlock {
	pub const fn new() -> Self {
		Self { locked: AtomicBool::new(false) }
	}

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

	pub unsafe fn unlock(&self) {
		self.locked.store(false, Ordering::Release);
	}

	pub fn with<O>(&self, f: impl FnOnce() -> O) -> O {
		unsafe {
			self.lock();
			let output = f();
			self.unlock();
			output
		}
	}
}
