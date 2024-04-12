// Copyright 2018-2024 the Deno authors. MIT license.

use std::sync::Arc;

use deno_terminal::colors;

use crate::CollectedCategoryOrTest;
use crate::CollectedTest;
use crate::CollectedTestCategory;

type RunTestBox = Arc<dyn (Fn(&CollectedTest) -> TestResult) + Send + Sync>;

struct Failure {
  test: CollectedTest,
  output: Vec<u8>,
}

struct Context {
  thread_pool_runner: Option<ThreadPoolTestRunner>,
  failures: Vec<Failure>,
  run_test: RunTestBox,
}

#[derive(Debug)]
pub enum TestResult {
  /// Test passed.
  Passed,
  /// Test failed, returning the captured output of the test.
  Failed { output: Vec<u8> },
}

#[derive(Debug, Clone)]
pub struct RunTestOptions {
  pub parallel: bool,
}

pub fn run_tests(
  category: &CollectedTestCategory,
  options: &RunTestOptions,
  run_test: RunTestBox,
) {
  let total_tests = category.test_count();
  let thread_pool_runner = if options.parallel {
    let parallelism = std::cmp::max(
      1,
      std::thread::available_parallelism()
        .map(|v| v.get())
        .unwrap_or(2)
        - 1,
    );
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
          eprintln!("test {} ... ", test.name);
          runner.queue_test((*test).clone());
          thread_pool_pending -= 1;
        } else {
          break;
        }
      }
      let (test, result) = runner.receive_result();
      eprint!("test {} ... ", test.name);
      match result {
        TestResult::Passed => {
          eprintln!("{}", colors::green_bold("ok"));
        }
        TestResult::Failed { output } => {
          eprintln!("{}", colors::green_bold("fail"));
          context.failures.push(Failure { test, output })
        }
      }
      pending -= 1;
      thread_pool_pending += 1;
    }
  } else {
    for test in tests {
      eprint!("test {} ... ", test.name);
      let result = (context.run_test)(test);
      match result {
        TestResult::Passed => {
          eprintln!("{}", colors::green_bold("ok"));
        }
        TestResult::Failed { output } => {
          eprintln!("{}", colors::green_bold("fail"));
          context.failures.push(Failure {
            test: (*test).clone(),
            output,
          })
        }
      }
    }
  }
}

struct ThreadPoolTestRunner {
  size: usize,
  sender: crossbeam::channel::Sender<CollectedTest>,
  receiver: crossbeam::channel::Receiver<(CollectedTest, TestResult)>,
}

impl ThreadPoolTestRunner {
  pub fn new(size: usize, run_test: RunTestBox) -> ThreadPoolTestRunner {
    let send_channel = crossbeam::channel::bounded::<CollectedTest>(size);
    let receive_channel =
      crossbeam::channel::unbounded::<(CollectedTest, TestResult)>();
    for _ in 0..size {
      let receiver = send_channel.1.clone();
      let sender = receive_channel.0.clone();
      let run_test = run_test.clone();
      std::thread::spawn(move || {
        let run_test = &run_test;
        while let Ok(value) = receiver.recv() {
          let result = (run_test)(&value);
          sender.send((value, result)).unwrap();
        }
      });
    }
    ThreadPoolTestRunner {
      size,
      sender: send_channel.0,
      receiver: receive_channel.1,
    }
  }

  pub fn queue_test(&self, test: CollectedTest) {
    self.sender.send(test).unwrap()
  }

  pub fn receive_result(&self) -> (CollectedTest, TestResult) {
    self.receiver.recv().unwrap()
  }
}
