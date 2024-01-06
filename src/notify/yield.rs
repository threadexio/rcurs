use super::Notify;

use core::sync::atomic::{AtomicBool, Ordering};

/// A [`Notify`] backend that yields to the OS scheduler.
pub struct Yield {
	wants_wake: AtomicBool,
}

impl Notify for Yield {
	fn new() -> Self {
		Self { wants_wake: AtomicBool::new(false) }
	}

	fn wait(&self) {
		while !self.wants_wake.load(Ordering::Relaxed) {
			std::thread::yield_now();
		}
	}

	fn notify(&self) {
		self.wants_wake.store(true, Ordering::Relaxed);
	}
}
