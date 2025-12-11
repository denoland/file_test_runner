// Copyright 2018-2025 the Deno authors. MIT license.

use parking_lot::{Condvar, Mutex};
use std::time::Duration;

/// A simple notification mechanism using condition variables.
/// Allows threads to wait for a signal or timeout.
pub struct Notify {
  condvar: Condvar,
  mutex: Mutex<bool>,
}

impl Default for Notify {
  fn default() -> Self {
    Self {
      condvar: Condvar::new(),
      mutex: Mutex::new(false),
    }
  }
}

impl Notify {
  /// Waits for up to the specified duration for a notification.
  /// Returns true if notified, false if timed out.
  pub fn wait_timeout(&self, duration: Duration) -> bool {
    let mut notified = self.mutex.lock();

    // If already notified, return immediately
    if *notified {
      return true;
    }

    // Wait for notification or timeout
    self.condvar.wait_for(&mut notified, duration);
    *notified
  }

  /// Notifies all waiting threads that the flag has occurred.
  pub fn notify(&self) {
    let mut notified = self.mutex.lock();
    *notified = true;
    self.condvar.notify_all();
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::sync::Arc;
  use std::thread;
  use std::time::Instant;

  #[test]
  fn test_notify_basic() {
    let notify = Arc::new(Notify::default());
    let notify_clone = notify.clone();

    let handle = thread::spawn(move || {
      thread::sleep(Duration::from_millis(100));
      notify_clone.notify();
    });

    let start = Instant::now();
    let result = notify.wait_timeout(Duration::from_secs(5));
    let elapsed = start.elapsed();

    assert!(result, "Should be notified");
    assert!(elapsed < Duration::from_secs(1), "Should not timeout");

    handle.join().unwrap();
  }

  #[test]
  fn test_notify_timeout() {
    let notify = Notify::default();

    let start = Instant::now();
    let result = notify.wait_timeout(Duration::from_millis(100));
    let elapsed = start.elapsed();

    assert!(!result, "Should timeout");
    assert!(
      elapsed >= Duration::from_millis(100),
      "Should wait for the full timeout duration"
    );
  }

  #[test]
  fn test_notify_already_notified() {
    let notify = Notify::default();

    // Notify before waiting
    notify.notify();

    let start = Instant::now();
    let result = notify.wait_timeout(Duration::from_secs(5));
    let elapsed = start.elapsed();

    assert!(result, "Should return immediately when already notified");
    assert!(
      elapsed < Duration::from_millis(50),
      "Should not wait when already notified"
    );
  }

  #[test]
  fn test_notify_multiple_waiters() {
    let notify = Arc::new(Notify::default());
    let mut handles = vec![];

    // Spawn multiple waiting threads
    for _ in 0..5 {
      let notify_clone = notify.clone();
      let handle = thread::spawn(move || {
        notify_clone.wait_timeout(Duration::from_secs(5))
      });
      handles.push(handle);
    }

    // Give threads time to start waiting
    thread::sleep(Duration::from_millis(50));

    // Notify all
    notify.notify();

    // All threads should be notified
    for handle in handles {
      let result = handle.join().unwrap();
      assert!(result, "All waiters should be notified");
    }
  }

  #[test]
  fn test_notify_immediate() {
    let notify = Arc::new(Notify::default());
    let notify_clone = notify.clone();

    // Notify immediately before thread even waits
    notify.notify();

    let handle = thread::spawn(move || {
      thread::sleep(Duration::from_millis(50));
      notify_clone.wait_timeout(Duration::from_secs(1))
    });

    let result = handle.join().unwrap();
    assert!(result, "Should return true even if notified before waiting");
  }

  #[test]
  fn test_notify_zero_timeout() {
    let notify = Notify::default();

    let result = notify.wait_timeout(Duration::from_millis(0));

    assert!(!result, "Should timeout immediately with zero duration");
  }
}
