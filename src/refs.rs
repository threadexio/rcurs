use portable_atomic::{AtomicUsize, Ordering};

const REF_COUNT_MAX: usize = usize::MAX;

#[derive(Debug)]
pub struct Refs {
	refs: AtomicUsize,
}

impl Refs {
	pub const fn one() -> Self {
		Self { refs: AtomicUsize::new(1) }
	}

	/// Get the number of refs.
	pub fn count(&self) -> usize {
		self.refs.load(Ordering::Relaxed)
	}

	/// Increment the ref count by one.
	pub fn take_ref(&self) {
		let r = self.refs.fetch_add(1, Ordering::Relaxed);

		if r == REF_COUNT_MAX {
			panic_ref_count_overflow();
		}
	}

	/// Decrement the ref count by one.
	///
	/// Returns `true` if this ref was the last one. Otherwise it returns `false`.
	pub unsafe fn release_ref(&self) -> bool {
		let r = self.refs.fetch_sub(1, Ordering::Release);
		if r == 1 {
			let _ = self.refs.load(Ordering::Acquire);
			true
		} else if r == 0 {
			panic_ref_count_overflow()
		} else {
			false
		}
	}
}

#[cold]
#[inline(never)]
fn panic_ref_count_overflow() -> ! {
	panic!("ref count overflowed")
}
