use super::Notify;

use core::sync::atomic::{AtomicBool, Ordering};

/// A [`Notify`] backend that spins in place.
pub struct Spin {
	wants_wake: AtomicBool,
}

impl Notify for Spin {
	fn new() -> Self {
		Self { wants_wake: AtomicBool::new(false) }
	}

	fn wait(&self) {
		while !self.wants_wake.load(Ordering::Relaxed) {
			core::hint::spin_loop();
		}
	}

	fn notify(&self) {
		self.wants_wake.store(true, Ordering::Relaxed);
	}
}
