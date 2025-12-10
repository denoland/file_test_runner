// Copyright 2018-2024 the Deno authors. MIT license.

use std::time::Duration;

use deno_terminal::colors;

use crate::collection::CollectedTest;
use crate::collection::CollectedTestCategory;
use crate::SubTestResult;
use crate::TestResult;

pub struct ReporterContext {
  pub is_parallel: bool,
}

pub trait Reporter<TData = ()> {
  fn report_category_start(
    &self,
    category: &CollectedTestCategory<TData>,
    context: &ReporterContext,
  );
  fn report_category_end(
    &self,
    category: &CollectedTestCategory<TData>,
    context: &ReporterContext,
  );
  fn report_test_start(
    &self,
    test: &CollectedTest<TData>,
    context: &ReporterContext,
  );
  fn report_test_end(
    &self,
    test: &CollectedTest<TData>,
    duration: Duration,
    result: &TestResult,
    context: &ReporterContext,
  );
}

impl<'a, TData> Default for &'a dyn Reporter<TData> {
  fn default() -> Self {
    &DefaultReporter
  }
}

pub struct DefaultReporter;

impl<TData> Reporter<TData> for DefaultReporter {
  fn report_category_start(
    &self,
    category: &CollectedTestCategory<TData>,
    _context: &ReporterContext,
  ) {
    eprintln!();
    eprintln!("     {} {}", colors::green_bold("Running"), category.name);
    eprintln!();
  }

  fn report_category_end(
    &self,
    _category: &CollectedTestCategory<TData>,
    _context: &ReporterContext,
  ) {
  }

  fn report_test_start(
    &self,
    test: &CollectedTest<TData>,
    context: &ReporterContext,
  ) {
    if !context.is_parallel {
      eprint!("test {} ... ", test.name);
    }
  }

  fn report_test_end(
    &self,
    test: &CollectedTest<TData>,
    duration: Duration,
    result: &TestResult,
    context: &ReporterContext,
  ) {
    let runner_output = build_end_test_message(result, duration);
    if context.is_parallel {
      eprint!("test {} ... {}", test.name, runner_output);
    } else {
      eprint!("{}", runner_output);
    }
  }
}

fn build_end_test_message(result: &TestResult, duration: Duration) -> String {
  fn output_sub_tests(
    indent: &str,
    sub_tests: &[SubTestResult],
    runner_output: &mut String,
  ) {
    for sub_test in sub_tests {
      match &sub_test.result {
        TestResult::Passed => {
          runner_output.push_str(&format!(
            "{}{} {}\n",
            indent,
            sub_test.name,
            colors::green_bold("ok"),
          ));
        }
        TestResult::Ignored => {
          runner_output.push_str(&format!(
            "{}{} {}\n",
            indent,
            sub_test.name,
            colors::gray("ignored"),
          ));
        }
        TestResult::Failed { .. } => {
          runner_output.push_str(&format!(
            "{}{} {}\n",
            indent,
            sub_test.name,
            colors::red_bold("fail")
          ));
        }
        TestResult::SubTests(sub_tests) => {
          runner_output.push_str(&format!("{}{}\n", indent, sub_test.name));
          if sub_tests.is_empty() {
            runner_output.push_str(&format!(
              "{}  {}\n",
              indent,
              colors::gray("<no sub-tests>")
            ));
          } else {
            output_sub_tests(
              &format!("{}  ", indent),
              sub_tests,
              runner_output,
            );
          }
        }
      }
    }
  }

  let mut runner_output = String::new();
  let duration_display = colors::gray(format!("({}ms)", duration.as_millis()));
  match result {
    TestResult::Passed => {
      runner_output.push_str(&format!(
        "{} {}\n",
        colors::green_bold("ok"),
        duration_display
      ));
    }
    TestResult::Ignored => {
      runner_output.push_str(&format!("{}\n", colors::gray("ignored")));
    }
    TestResult::Failed { .. } => {
      runner_output.push_str(&format!(
        "{} {}\n",
        colors::red_bold("fail"),
        duration_display
      ));
    }
    TestResult::SubTests(sub_tests) => {
      runner_output.push_str(&format!("{}\n", duration_display));
      output_sub_tests("  ", sub_tests, &mut runner_output);
    }
  }

  runner_output
}

#[cfg(test)]
mod test {
  use deno_terminal::colors;

  use super::*;

  #[test]
  fn test_build_end_test_message_passed() {
    assert_eq!(
      build_end_test_message(
        &super::TestResult::Passed,
        std::time::Duration::from_millis(100),
      ),
      format!("{} {}\n", colors::green_bold("ok"), colors::gray("(100ms)"))
    );
  }

  #[test]
  fn test_build_end_test_message_failed() {
    let message = build_end_test_message(
      &super::TestResult::Failed {
        output: b"error".to_vec(),
      },
      std::time::Duration::from_millis(100),
    );
    assert_eq!(
      message,
      format!("{} {}\n", colors::red_bold("fail"), colors::gray("(100ms)"))
    );
  }

  #[test]
  fn test_build_end_test_message_ignored() {
    assert_eq!(
      build_end_test_message(
        &super::TestResult::Ignored,
        std::time::Duration::from_millis(10),
      ),
      format!("{}\n", colors::gray("ignored"))
    );
  }

  #[test]
  fn test_build_end_test_message_sub_tests() {
    let message = build_end_test_message(
      &super::TestResult::SubTests(vec![
        super::SubTestResult {
          name: "step1".to_string(),
          result: super::TestResult::Passed,
        },
        super::SubTestResult {
          name: "step2".to_string(),
          result: super::TestResult::Failed {
            output: b"error1".to_vec(),
          },
        },
        super::SubTestResult {
          name: "step3".to_string(),
          result: super::TestResult::Failed {
            output: b"error2".to_vec(),
          },
        },
        super::SubTestResult {
          name: "step4".to_string(),
          result: super::TestResult::SubTests(vec![
            super::SubTestResult {
              name: "sub-step1".to_string(),
              result: super::TestResult::Passed,
            },
            super::SubTestResult {
              name: "sub-step2".to_string(),
              result: super::TestResult::Failed {
                output: b"error3".to_vec(),
              },
            },
          ]),
        },
      ]),
      std::time::Duration::from_millis(10),
    );

    assert_eq!(
      message,
      format!(
        "{}\n  step1 {}\n  step2 {}\n  step3 {}\n  step4\n    sub-step1 {}\n    sub-step2 {}\n",
        colors::gray("(10ms)"),
        colors::green_bold("ok"),
        colors::red_bold("fail"),
        colors::red_bold("fail"),
        colors::green_bold("ok"),
        colors::red_bold("fail"),
      )
    );
  }
}
