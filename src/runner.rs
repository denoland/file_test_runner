// Copyright 2018-2025 the Deno authors. MIT license.

use std::cell::RefCell;
use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;

use parking_lot::Mutex;
use rayon::ThreadPool;

use crate::NO_CAPTURE;
use crate::collection::CollectedCategoryOrTest;
use crate::collection::CollectedTest;
use crate::collection::CollectedTestCategory;
use crate::reporter::LogReporter;
use crate::reporter::Reporter;
use crate::reporter::ReporterContext;
use crate::reporter::ReporterFailure;
use crate::utils::Notify;

type RunTestFunc<TData> =
  Arc<dyn (Fn(&CollectedTest<TData>) -> TestResult) + Send + Sync>;

struct Context<TData: Clone + Send + 'static> {
  failures: Vec<ReporterFailure<TData>>,
  parallelism: NonZeroUsize,
  run_test: RunTestFunc<TData>,
  reporter: Arc<dyn Reporter<TData>>,
  pending_tests: Arc<Mutex<HashMap<String, Instant>>>,
  pool: ThreadPool,
}

static GLOBAL_PANIC_HOOK_COUNT: Mutex<usize> = Mutex::new(0);

type PanicHook = Box<dyn Fn(&std::panic::PanicHookInfo) + Sync + Send>;

thread_local! {
  static LOCAL_PANIC_HOOK: RefCell<Option<PanicHook>> = RefCell::new(None);
}

#[derive(Debug, Clone)]
pub struct SubTestResult {
  pub name: String,
  pub result: TestResult,
}

#[must_use]
#[derive(Debug, Clone)]
pub enum TestResult {
  /// Test passed.
  Passed {
    /// Optional duration to report.
    duration: Option<Duration>,
  },
  /// Test was ignored.
  Ignored,
  /// Test failed, returning the captured output of the test.
  Failed {
    /// Optional duration to report.
    duration: Option<Duration>,
    /// Test failure output that should be shown to the user.
    output: Vec<u8>,
  },
  /// Multiple sub tests were run.
  SubTests {
    /// Optional duration to report.
    duration: Option<Duration>,
    sub_tests: Vec<SubTestResult>,
  },
}

impl TestResult {
  pub fn duration(&self) -> Option<Duration> {
    match self {
      TestResult::Passed { duration } => *duration,
      TestResult::Ignored => None,
      TestResult::Failed { duration, .. } => *duration,
      TestResult::SubTests { duration, .. } => *duration,
    }
  }

  pub fn is_failed(&self) -> bool {
    match self {
      TestResult::Passed { .. } | TestResult::Ignored => false,
      TestResult::Failed { .. } => true,
      TestResult::SubTests { sub_tests, .. } => {
        sub_tests.iter().any(|s| s.result.is_failed())
      }
    }
  }

  /// Allows using a closure that may panic, capturing the panic message and
  /// returning it as a TestResult::Failed.
  ///
  /// Ensure the code is unwind safe and use with `AssertUnwindSafe(|| { /* test code */ })`.
  pub fn from_maybe_panic(
    func: impl FnOnce() + std::panic::UnwindSafe,
  ) -> Self {
    Self::from_maybe_panic_or_result(|| {
      func();
      TestResult::Passed { duration: None }
    })
  }

  /// Allows using a closure that may panic, capturing the panic message and
  /// returning it as a TestResult::Failed. If a panic does not occur, uses
  /// the returned TestResult.
  ///
  /// Ensure the code is unwind safe and use with `AssertUnwindSafe(|| { /* test code */ })`.
  pub fn from_maybe_panic_or_result(
    func: impl FnOnce() -> TestResult + std::panic::UnwindSafe,
  ) -> Self {
    // increment the panic hook
    {
      let mut hook_count = GLOBAL_PANIC_HOOK_COUNT.lock();
      if *hook_count == 0 {
        let _ = std::panic::take_hook();
        std::panic::set_hook(Box::new(|info| {
          LOCAL_PANIC_HOOK.with(|hook| {
            if let Some(hook) = &*hook.borrow() {
              hook(info);
            }
          });
        }));
      }
      *hook_count += 1;
      drop(hook_count); // explicit for clarity, drop after setting the hook
    }

    let panic_message = Arc::new(Mutex::new(Vec::<u8>::new()));

    let previous_panic_hook = LOCAL_PANIC_HOOK.with(|hook| {
      let panic_message = panic_message.clone();
      hook.borrow_mut().replace(Box::new(move |info| {
        let backtrace = capture_backtrace();
        panic_message.lock().extend(
          format!(
            "{}{}",
            info,
            backtrace
              .map(|trace| format!("\n{}", trace))
              .unwrap_or_default()
          )
          .into_bytes(),
        );
      }))
    });

    let result = std::panic::catch_unwind(func);

    // restore or clear the local panic hook
    LOCAL_PANIC_HOOK.with(|hook| {
      *hook.borrow_mut() = previous_panic_hook;
    });

    // decrement the global panic hook
    {
      let mut hook_count = GLOBAL_PANIC_HOOK_COUNT.lock();
      *hook_count -= 1;
      if *hook_count == 0 {
        let _ = std::panic::take_hook();
      }
      drop(hook_count); // explicit for clarity, drop after taking the hook
    }

    result.unwrap_or_else(|_| TestResult::Failed {
      duration: None,
      output: panic_message.lock().clone(),
    })
  }
}

fn capture_backtrace() -> Option<String> {
  let backtrace = std::backtrace::Backtrace::capture();
  if backtrace.status() != std::backtrace::BacktraceStatus::Captured {
    return None;
  }
  let text = format!("{}", backtrace);
  // strip the code in this crate from the start of the backtrace
  let lines = text.lines().collect::<Vec<_>>();
  let last_position = lines
    .iter()
    .position(|line| line.contains("core::panicking::panic_fmt"));
  Some(match last_position {
    Some(position) => lines[position + 2..].join("\n"),
    None => text,
  })
}

#[derive(Clone)]
pub struct RunOptions<TData> {
  pub parallelism: NonZeroUsize,
  pub reporter: Arc<dyn Reporter<TData>>,
}

impl<TData> Default for RunOptions<TData> {
  fn default() -> Self {
    Self {
      parallelism: NonZeroUsize::new(if *NO_CAPTURE {
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
      })
      .unwrap(),
      reporter: Arc::new(LogReporter),
    }
  }
}

pub fn run_tests<TData: Clone + Send + 'static>(
  category: &CollectedTestCategory<TData>,
  options: RunOptions<TData>,
  run_test: impl (Fn(&CollectedTest<TData>) -> TestResult) + Send + Sync + 'static,
) {
  let total_tests = category.test_count();
  if total_tests == 0 {
    return; // no tests to run because they were filtered out
  }

  let run_test = Arc::new(run_test);
  let max_parallelism = options.parallelism;

  // Create a rayon thread pool
  let pool = rayon::ThreadPoolBuilder::new()
    // +2 is one thread for long running tests checker and second
    // thread is the thread that drives tests into the pool of receivers
    .num_threads(max_parallelism.get() + 2)
    .build()
    .expect("Failed to create thread pool");

  // thread that checks for any long running tests
  let pending_tests = Arc::new(Mutex::new(
    HashMap::<String, Instant>::with_capacity(max_parallelism.get()),
  ));
  let exit_notify = Arc::new(Notify::default());
  pool.spawn({
    let pending_tests = pending_tests.clone();
    let reporter = options.reporter.clone();
    let exit_notify = exit_notify.clone();
    move || loop {
      if exit_notify.wait_timeout(std::time::Duration::from_secs(1)) {
        return;
      }
      let pending = pending_tests.lock().clone();
      let to_remove = pending
        .into_iter()
        .filter_map(|(test_name, start_time)| {
          if reporter.report_running_test(&test_name, start_time.elapsed()) {
            Some(test_name)
          } else {
            None
          }
        })
        .collect::<Vec<_>>();
      {
        let mut pending_tests = pending_tests.lock();
        for key in to_remove {
          pending_tests.remove(&key);
        }
      }
    }
  });

  let mut context = Context {
    failures: Vec::new(),
    run_test,
    parallelism: options.parallelism,
    reporter: options.reporter,
    pool,
    pending_tests,
  };
  run_category(category, &mut context);

  exit_notify.notify();

  context
    .reporter
    .report_failures(&context.failures, total_tests);
  if !context.failures.is_empty() {
    panic!("{} failed of {}", context.failures.len(), total_tests);
  }
}

fn run_category<TData: Clone + Send>(
  category: &CollectedTestCategory<TData>,
  context: &mut Context<TData>,
) {
  let mut tests = Vec::new();
  let mut categories = Vec::new();
  for child in &category.children {
    match child {
      CollectedCategoryOrTest::Category(c) => {
        categories.push(c);
      }
      CollectedCategoryOrTest::Test(t) => {
        tests.push(t.clone());
      }
    }
  }

  if !tests.is_empty() {
    run_tests_for_category(category, tests, context);
  }

  for category in categories {
    run_category(category, context);
  }
}

fn run_tests_for_category<TData: Clone + Send>(
  category: &CollectedTestCategory<TData>,
  tests: Vec<CollectedTest<TData>>,
  context: &mut Context<TData>,
) {
  enum SendMessage<TData> {
    Start {
      test: CollectedTest<TData>,
    },
    Result {
      test: CollectedTest<TData>,
      duration: Duration,
      result: TestResult,
    },
  }

  if tests.is_empty() {
    return; // ignore empty categories if they exist for some reason
  }

  let reporter = &context.reporter;
  let max_parallelism = context.parallelism.get();
  let reporter_context = ReporterContext {
    is_parallel: max_parallelism > 1,
  };
  reporter.report_category_start(category, &reporter_context);

  let receive_receiver = {
    let (receiver_sender, receive_receiver) =
      crossbeam_channel::unbounded::<SendMessage<TData>>();
    let (send_sender, send_receiver) =
      crossbeam_channel::bounded::<CollectedTest<TData>>(max_parallelism);
    for _ in 0..max_parallelism {
      let send_receiver = send_receiver.clone();
      let sender = receiver_sender.clone();
      let run_test = context.run_test.clone();
      let pending_tests = context.pending_tests.clone();
      context.pool.spawn(move || {
        let run_test = &run_test;
        while let Ok(test) = send_receiver.recv() {
          let start = Instant::now();
          // it's more deterministic to send this back to the main thread
          // for when the parallelism is 1
          _ = sender.send(SendMessage::Start { test: test.clone() });
          pending_tests.lock().insert(test.name.clone(), start);
          let result = (run_test)(&test);
          pending_tests.lock().remove(&test.name);
          if sender
            .send(SendMessage::Result {
              test,
              duration: start.elapsed(),
              result,
            })
            .is_err()
          {
            return;
          }
        }
      });
    }

    context.pool.spawn(move || {
      for test in tests {
        if send_sender.send(test).is_err() {
          return; // receiver dropped due to fail fast
        }
      }
    });

    receive_receiver
  };

  while let Ok(message) = receive_receiver.recv() {
    match message {
      SendMessage::Start { test } => {
        reporter.report_test_start(&test, &reporter_context)
      }
      SendMessage::Result {
        test,
        duration,
        result,
      } => {
        reporter.report_test_end(&test, duration, &result, &reporter_context);
        let is_failure = result.is_failed();
        let failure_output = collect_failure_output(result);
        if is_failure {
          context.failures.push(ReporterFailure {
            test,
            output: failure_output,
          });
        }
      }
    }
  }

  reporter.report_category_end(category, &reporter_context);
}

fn collect_failure_output(result: TestResult) -> Vec<u8> {
  fn output_sub_tests(
    sub_tests: &[SubTestResult],
    failure_output: &mut Vec<u8>,
  ) {
    for sub_test in sub_tests {
      match &sub_test.result {
        TestResult::Passed { .. } | TestResult::Ignored => {}
        TestResult::Failed { output, .. } => {
          if !failure_output.is_empty() {
            failure_output.push(b'\n');
          }
          failure_output.extend(output);
        }
        TestResult::SubTests { sub_tests, .. } => {
          if !sub_tests.is_empty() {
            output_sub_tests(sub_tests, failure_output);
          }
        }
      }
    }
  }

  let mut failure_output = Vec::new();
  match result {
    TestResult::Passed { .. } | TestResult::Ignored => {}
    TestResult::Failed { output, .. } => {
      failure_output = output;
    }
    TestResult::SubTests { sub_tests, .. } => {
      output_sub_tests(&sub_tests, &mut failure_output);
    }
  }

  failure_output
}

#[cfg(test)]
mod test {
  use super::*;

  #[test]
  fn test_collect_failure_output_failed() {
    let failure_output = collect_failure_output(super::TestResult::Failed {
      duration: None,
      output: b"error".to_vec(),
    });
    assert_eq!(failure_output, b"error");
  }

  #[test]
  fn test_collect_failure_output_sub_tests() {
    let failure_output = collect_failure_output(super::TestResult::SubTests {
      duration: None,
      sub_tests: vec![
        super::SubTestResult {
          name: "step1".to_string(),
          result: super::TestResult::Passed { duration: None },
        },
        super::SubTestResult {
          name: "step2".to_string(),
          result: super::TestResult::Failed {
            duration: None,
            output: b"error1".to_vec(),
          },
        },
        super::SubTestResult {
          name: "step3".to_string(),
          result: super::TestResult::Failed {
            duration: None,
            output: b"error2".to_vec(),
          },
        },
        super::SubTestResult {
          name: "step4".to_string(),
          result: super::TestResult::SubTests {
            duration: None,
            sub_tests: vec![
              super::SubTestResult {
                name: "sub-step1".to_string(),
                result: super::TestResult::Passed { duration: None },
              },
              super::SubTestResult {
                name: "sub-step2".to_string(),
                result: super::TestResult::Failed {
                  duration: None,
                  output: b"error3".to_vec(),
                },
              },
            ],
          },
        },
      ],
    });

    assert_eq!(
      String::from_utf8(failure_output).unwrap(),
      "error1\nerror2\nerror3"
    );
  }
}
