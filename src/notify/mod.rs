use crate::cfg::cfg_std;

/// An interface for general purpose notification objects.
pub trait Notify: Sized {
	/// Create a new [`Notify`] object.
	fn new() -> Self;

	/// Block the current thread until [`notify`] is called.
	///
	/// This function will sleep until the first [`notify`] call. It does
	/// not suffer from spurious wake-ups. When it returns, it is guaranteed
	/// that [`notify`] was called at least once.
	///
	/// [`notify`]: Self::notify
	fn wait(&self);

	/// Notify all of the threads [`wait`]ing to wake up.
	///
	/// This function does _not_ block.
	///
	/// [`wait`]: Self::wait
	fn notify(&self);
}

mod spin;
cfg_std! {
	mod blocking;
	mod r#yield;
}

pub use self::spin::Spin;
cfg_std! {
	pub use self::r#yield::Yield;
	pub use self::blocking::Blocking;
}

#[cfg(all(test, feature = "std"))]
mod tests {
	use super::*;

	use std::sync::atomic::{AtomicI32, Ordering};
	use std::thread::{scope, sleep};
	use std::time::{Duration, Instant};

	fn time<F>(f: F) -> Duration
	where
		F: FnOnce(),
	{
		let start = Instant::now();
		f();
		start.elapsed()
	}

	fn test_notify<N: Notify + Sync>() {
		let notify = N::new();
		let finished = AtomicI32::new(0);

		scope(|scope| {
			scope.spawn(|| {
				notify.wait();
				finished.fetch_add(1, Ordering::Relaxed);
			});

			scope.spawn(|| {
				notify.wait();
				finished.fetch_add(1, Ordering::Relaxed);
			});

			assert_eq!(finished.load(Ordering::Relaxed), 0);

			sleep(Duration::from_secs(1));
			notify.notify();
			sleep(Duration::from_secs(1));

			assert_eq!(finished.load(Ordering::Relaxed), 2);

			sleep(Duration::from_secs(1));
			notify.notify();
			sleep(Duration::from_secs(1));

			assert_eq!(finished.load(Ordering::Relaxed), 2);
		});
	}

	fn test_wait<N: Notify + Sync>() {
		// Quarter second precision is horrible but good enough for this test
		const EPSILON: Duration = Duration::new(0, 250 * 1_000_000);

		const EXPECTED: Duration = Duration::new(4, 0);

		let notify = N::new();

		scope(|scope| {
			scope.spawn(|| {
				sleep(EXPECTED);
				notify.notify();
			});

			let t = time(|| {
				notify.wait();
			});

			assert!(
				f64::abs(EXPECTED.as_secs_f64() - t.as_secs_f64())
					< EPSILON.as_secs_f64()
			);
		});
	}

	macro_rules! test_implementations {
        (@impl, $test_fn:ident) => {
            $test_fn::<Blocking>();
            $test_fn::<Spin>();
            $test_fn::<Yield>();
        };
        ($(
            $test_fn:ident => $test_fn_impl:ident,
        )*) => {
            $(
                #[test]
                fn $test_fn_impl() {
                    test_implementations! { @impl, $test_fn }
                }
            )*
        };
    }

	test_implementations! {
		test_notify => test_notify_impl,
		test_wait => test_wait_impl,
	}
}
