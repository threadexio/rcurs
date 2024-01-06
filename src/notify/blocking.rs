use super::Notify;

use std::sync::{Condvar, Mutex};

/// A [`Notify`] backend that uses a [`Condvar`] to achieve true blocking.
pub struct Blocking {
    /* Keep track of both whether notify was called and how many waiter there are.
     * This way, the waiter who wakes up last can know to reset the `notified` flag
     * again to prepare for the next `notify`.
     */
    lock: Mutex<(bool, u8)>,
    var: Condvar,
}

impl Notify for Blocking {
    fn new() -> Self {
        Self {
            lock: Mutex::new((false, 0)),
            var: Condvar::new(),
        }
    }

    fn wait(&self) {
        let mut guard = self.lock.lock().unwrap();
        guard.1 += 1;

        let mut guard = self
            .var
            .wait_while(guard, |(notified, _)| !*notified)
            .unwrap();
        guard.1 -= 1;

        if guard.1 == 0 {
            guard.0 = false;
        }
    }

    fn notify(&self) {
        let mut guard = self.lock.lock().unwrap();
        guard.0 = true;
        self.var.notify_all();
    }
}

impl Default for Blocking {
    fn default() -> Self {
        Self::new()
    }
}
