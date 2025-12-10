// Copyright 2018-2024 the Deno authors. MIT license.

use parking_lot::Condvar;
use parking_lot::Mutex;
use std::num::NonZeroUsize;

struct Permits {
  max: usize,
  used: usize,
}

pub struct Semaphore {
  permits: Mutex<Permits>,
  condvar: Condvar,
}

impl Semaphore {
  pub fn new(max_permits: usize) -> Self {
    Semaphore {
      permits: Mutex::new(Permits {
        max: max_permits,
        used: 0,
      }),
      condvar: Condvar::new(),
    }
  }

  pub fn acquire(&self) {
    let mut permits = self.permits.lock();
    while permits.used >= permits.max {
      self.condvar.wait(&mut permits);
    }
    permits.used += 1;
  }

  pub fn release(&self) {
    let mut permits = self.permits.lock();
    permits.used -= 1;
    if permits.used < permits.max {
      drop(permits);
      self.condvar.notify_one();
    }
  }

  pub fn set_max(&self, n: NonZeroUsize) {
    let mut permits = self.permits.lock();
    let is_greater = n.get() > permits.max;
    permits.max = n.get();
    drop(permits);
    if is_greater {
      self.condvar.notify_all(); // Wake up waiting threads
    }
  }
}
