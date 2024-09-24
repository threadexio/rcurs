use portable_atomic::{fence, AtomicUsize, Ordering};

const REF_COUNT_MAX: usize = usize::MAX;

/// An atomic counter for references.
///
/// Ref counting functions are based on implementation of `Arc` from the standard library.
#[derive(Debug)]
pub struct Refs {
	refs: AtomicUsize,
}

impl Refs {
	/// Create a new [`Refs`] counter with one reference.
	pub const fn new() -> Self {
		Self { refs: AtomicUsize::new(1) }
	}

	/// Increment the ref count by one.
	pub fn take_ref(&self) {
		let old_refs = self.refs.fetch_add(1, Ordering::Relaxed);

		// If the number of refs before we incremented it above is equal to the maximum
		// value, then our increment results in an overflow.
		if old_refs == REF_COUNT_MAX {
			panic_ref_count_overflow();
		}
	}

	/// Decrement the ref count by one.
	///
	/// Returns `true` if this ref was the last one. Otherwise it returns `false`.
	pub unsafe fn release_ref(&self) -> bool {
		let old_refs = self.refs.fetch_sub(1, Ordering::Release);

		match old_refs {
			// If the number of refs before out decrement operation is 0, then that operation
			// will have overflowed the count back up to the maximum value.
			0 => panic_ref_count_overflow(),
			1 => {
				fence(Ordering::Acquire);
				true
			},
			_ => false,
		}
	}
}

#[cold]
#[inline(never)]
fn panic_ref_count_overflow() -> ! {
	panic!("ref count overflowed")
}
