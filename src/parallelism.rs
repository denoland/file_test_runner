// Copyright 2018-2025 the Deno authors. MIT license.

use std::num::NonZeroUsize;

use crate::NO_CAPTURE;
use crate::utils::Semaphore;

/// Trait to dynamically set the amount of parallelism that
/// should be done.
pub trait ParallelismProvider: Send + Sync {
  /// Number of threads that should be created at th estart.
  fn max_parallelism(&self) -> NonZeroUsize;
  /// Called by a thread when the test starts.
  ///
  /// The implementation can block in this call in order
  /// to hold up a test thread.
  fn on_test_start(&self);
  fn on_test_end(&self);
}

pub struct Parallelism {
  max: NonZeroUsize,
  sempahore: Semaphore,
}

impl Parallelism {
  pub fn new(max_parallelism: NonZeroUsize) -> Self {
    Self {
      max: max_parallelism,
      sempahore: Semaphore::new(max_parallelism.get()),
    }
  }

  pub fn none() -> Self {
    Self::new(NonZeroUsize::new(1).unwrap())
  }

  /// By default, this will parallelize the tests across all available
  /// threads, minus one.
  ///
  /// This can be overridden by setting the `FILE_TEST_RUNNER_PARALLELISM`
  /// environment variable to the desired number of parallel threads.
  pub fn from_env() -> Self {
    let amount = if *NO_CAPTURE {
      1
    } else {
      std::cmp::max(
        1,
        std::env::var("FILE_TEST_RUNNER_PARALLELISM")
          .ok()
          .and_then(|v| v.parse().ok())
          .unwrap_or_else(|| {
            std::thread::available_parallelism()
              .map(|v| v.get())
              .unwrap_or(2)
              - 1
          }),
      )
    };
    Parallelism::new(NonZeroUsize::new(amount).unwrap())
  }

  /// Can be used to reduce the amount of parallelism.
  ///
  /// Note that increasing the parallelism beyond the max
  /// parallelism won't do anything.
  pub fn set_parallelism(&self, parallelism: NonZeroUsize) {
    self.sempahore.set_max(parallelism);
  }
}

impl ParallelismProvider for Parallelism {
  fn max_parallelism(&self) -> NonZeroUsize {
    self.max
  }

  fn on_test_start(&self) {
    self.sempahore.acquire();
  }

  fn on_test_end(&self) {
    self.sempahore.release();
  }
}
