// Copyright 2018-2024 the Deno authors. MIT license.

use core::panic;
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;

use deno_terminal::colors;
use parking_lot::Mutex;

use crate::CollectedCategoryOrTest;
use crate::CollectedTest;
use crate::CollectedTestCategory;

pub type RunTestFunc =
  Arc<dyn (Fn(&CollectedTest) -> TestResult) + Send + Sync>;

struct Failure {
  test: CollectedTest,
  output: Vec<u8>,
}

struct Context {
  thread_pool_runner: Option<ThreadPoolTestRunner>,
  failures: Vec<Failure>,
  run_test: RunTestFunc,
}

static GLOBAL_PANIC_HOOK_COUNT: AtomicUsize = AtomicUsize::new(0);

type PanicHook = Box<dyn Fn(&std::panic::PanicInfo) + Sync + Send>;

thread_local! {
  static LOCAL_PANIC_HOOK: RefCell<Option<PanicHook>> = RefCell::new(None);
}

#[derive(Debug)]
pub enum TestResult {
  /// Test passed.
  Passed,
  /// Test was ignored.
  Ignored,
  /// Test failed, returning the captured output of the test.
  Failed { output: Vec<u8> },
}

impl TestResult {
  /// Allows using a closure that may panic, capturing the panic message and
  /// returning it as a TestResult::Failed.
  ///
  /// Ensure the code is unwind safe and use with `AssertUnwindSafe(|| { /* test code */ })`.
  pub fn from_maybe_panic(
    func: impl FnOnce() + std::panic::UnwindSafe,
  ) -> Self {
    if GLOBAL_PANIC_HOOK_COUNT.fetch_add(1, Ordering::SeqCst) == 0 {
      let _ = std::panic::take_hook();
      std::panic::set_hook(Box::new(|info| {
        LOCAL_PANIC_HOOK.with(|hook| {
          if let Some(hook) = &*hook.borrow() {
            hook(info);
          }
        });
      }));
    }

    let panic_message = Arc::new(Mutex::new(Vec::<u8>::new()));

    LOCAL_PANIC_HOOK.with(|hook| {
      let panic_message = panic_message.clone();
      *hook.borrow_mut() = Some(Box::new(move |info| {
        panic_message
          .lock()
          .extend(format!("{}", info).into_bytes());
      }));
    });

    let result = std::panic::catch_unwind(func);

    if GLOBAL_PANIC_HOOK_COUNT.fetch_sub(1, Ordering::SeqCst) == 1 {
      let _ = std::panic::take_hook();
    }

    result
      .map(|_| TestResult::Passed)
      .unwrap_or_else(|_| TestResult::Failed {
        output: panic_message.lock().clone(),
      })
  }
}

#[derive(Debug, Clone)]
pub struct RunOptions {
  pub parallel: bool,
}

pub fn run_tests(
  category: &CollectedTestCategory,
  options: RunOptions,
  run_test: RunTestFunc,
) {
  let total_tests = category.test_count();
  if total_tests == 0 {
    return; // no tests to run because they were filtered out
  }

  let parallelism = if options.parallel {
    std::cmp::max(
      1,
      std::thread::available_parallelism()
        .map(|v| v.get())
        .unwrap_or(2)
        - 1,
    )
  } else {
    1
  };
  let thread_pool_runner = if parallelism > 1 {
    Some(ThreadPoolTestRunner::new(parallelism, run_test.clone()))
  } else {
    None
  };
  let mut context = Context {
    thread_pool_runner,
    failures: Vec::new(),
    run_test,
  };
  run_category(category, &mut context);

  eprintln!();
  if !context.failures.is_empty() {
    eprintln!("spec failures:");
    eprintln!();
    for failure in &context.failures {
      eprintln!("---- {} ----", failure.test.name);
      eprintln!("{}", String::from_utf8_lossy(&failure.output));
      eprintln!("Test file: {}", failure.test.path.display());
      eprintln!();
    }
    panic!("{} failed of {}", context.failures.len(), total_tests);
  } else {
    eprintln!("{} tests passed", total_tests);
  }
  eprintln!();
}

fn run_category(category: &CollectedTestCategory, context: &mut Context) {
  let mut tests = Vec::new();
  let mut categories = Vec::new();
  for child in &category.children {
    match child {
      CollectedCategoryOrTest::Category(c) => {
        categories.push(c);
      }
      CollectedCategoryOrTest::Test(t) => {
        tests.push(t);
      }
    }
  }

  if !tests.is_empty() {
    run_tests_for_category(category, &tests, context);
  }

  for category in categories {
    run_category(category, context);
  }
}

fn run_tests_for_category(
  category: &CollectedTestCategory,
  tests: &[&CollectedTest],
  context: &mut Context,
) {
  if tests.is_empty() {
    return; // ignore empty categories if they exist for some reason
  }

  eprintln!();
  eprintln!("     {} {}", colors::green_bold("Running"), category.name);
  eprintln!();

  if let Some(runner) = context
    .thread_pool_runner
    .as_ref()
    .filter(|_| tests.len() > 1)
  {
    let mut test_iterator = tests.iter();
    let mut pending = tests.len();
    let mut thread_pool_pending = runner.size;
    while pending > 0 {
      while thread_pool_pending > 0 {
        if let Some(test) = test_iterator.next() {
          runner.queue_test((*test).clone());
          thread_pool_pending -= 1;
        } else {
          break;
        }
      }
      let (test, duration, result) = runner.receive_result();
      let duration_display =
        colors::gray(format!("({}ms)", duration.as_millis()));
      match result {
        TestResult::Passed => {
          eprintln!(
            "test {} ... {} {}",
            test.name,
            colors::green_bold("ok"),
            duration_display
          );
        }
        TestResult::Ignored => {
          // output nothing
        }
        TestResult::Failed { output } => {
          eprintln!(
            "test {} ... {} {}",
            test.name,
            colors::green_bold("fail"),
            duration_display
          );
          context.failures.push(Failure { test, output })
        }
      }
      pending -= 1;
      thread_pool_pending += 1;
    }
  } else {
    for test in tests {
      eprint!("test {} ... ", test.name);
      let start = Instant::now();
      let result = (context.run_test)(test);
      let duration_display =
        colors::gray(format!("({}ms)", start.elapsed().as_millis()));
      match result {
        TestResult::Passed => {
          eprintln!("{} {}", colors::green_bold("ok"), duration_display);
        }
        TestResult::Ignored => {
          eprintln!("{}", colors::gray("ignored"));
        }
        TestResult::Failed { output } => {
          eprintln!("{} {}", colors::green_bold("fail"), duration_display);
          context.failures.push(Failure {
            test: (*test).clone(),
            output,
          })
        }
      }
    }
  }
}

#[derive(Default)]
struct PendingTests {
  finished: bool,
  pending: HashMap<String, Instant>,
}

struct ThreadPoolTestRunner {
  size: usize,
  sender: crossbeam_channel::Sender<CollectedTest>,
  receiver: crossbeam_channel::Receiver<(CollectedTest, Duration, TestResult)>,
  pending_tests: Arc<Mutex<PendingTests>>,
}

impl ThreadPoolTestRunner {
  pub fn new(size: usize, run_test: RunTestFunc) -> ThreadPoolTestRunner {
    let pending_tests = Arc::new(Mutex::new(PendingTests::default()));
    let send_channel = crossbeam_channel::bounded::<CollectedTest>(size);
    let receive_channel =
      crossbeam_channel::unbounded::<(CollectedTest, Duration, TestResult)>();
    for _ in 0..size {
      let receiver = send_channel.1.clone();
      let sender = receive_channel.0.clone();
      let run_test = run_test.clone();
      std::thread::spawn(move || {
        let run_test = &run_test;
        while let Ok(value) = receiver.recv() {
          let start = Instant::now();
          let result = (run_test)(&value);
          sender.send((value, start.elapsed(), result)).unwrap();
        }
      });
    }

    // thread that checks for any long running tests
    std::thread::spawn({
      let pending_tests = pending_tests.clone();
      move || loop {
        std::thread::sleep(std::time::Duration::from_secs(1));
        let mut data = pending_tests.lock();
        if data.finished {
          break;
        }
        let mut long_tests = Vec::new();
        for (key, value) in &data.pending {
          if value.elapsed().as_secs() > 60 {
            long_tests.push(key.clone());
          }
        }
        for test in long_tests {
          eprintln!("test {} has been running for more than 60 seconds", test);
          data.pending.remove(&test);
        }
      }
    });

    ThreadPoolTestRunner {
      size,
      sender: send_channel.0,
      receiver: receive_channel.1,
      pending_tests,
    }
  }

  pub fn queue_test(&self, test: CollectedTest) {
    self
      .pending_tests
      .lock()
      .pending
      .insert(test.name.clone(), Instant::now());
    self.sender.send(test).unwrap()
  }

  pub fn receive_result(&self) -> (CollectedTest, Duration, TestResult) {
    let data = self.receiver.recv().unwrap();
    self.pending_tests.lock().pending.remove(&data.0.name);
    data
  }
}
