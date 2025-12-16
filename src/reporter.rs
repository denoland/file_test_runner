// Copyright 2018-2025 the Deno authors. MIT license.

use std::time::Duration;

use deno_terminal::colors;

use crate::NO_CAPTURE;
use crate::SubTestResult;
use crate::TestResult;
use crate::collection::CollectedTest;
use crate::collection::CollectedTestCategory;

#[derive(Clone)]
pub struct ReporterContext {
  pub is_parallel: bool,
}

pub struct ReporterFailure<TData> {
  pub test: CollectedTest<TData>,
  pub output: Vec<u8>,
}

pub trait Reporter<TData = ()>: Send + Sync {
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
  /// Reports all the currently running tests every 1 second until this method
  /// returns `true` for the test or the test is no longer running.
  ///
  /// This can be useful to report a test has been running for too long
  /// or to update a progress bar with running tests.
  fn report_running_test(&self, test_name: &str, duration: Duration) -> bool;
  fn report_failures(
    &self,
    failures: &[ReporterFailure<TData>],
    total_tests: usize,
  );
}

pub struct LogReporter;

impl LogReporter {
  pub fn write_report_category_start<TData, W: std::io::Write>(
    writer: &mut W,
    category: &CollectedTestCategory<TData>,
  ) -> std::io::Result<()> {
    writeln!(writer)?;
    writeln!(
      writer,
      "     {} {}",
      colors::green_bold("Running"),
      category.name
    )?;
    writeln!(writer)?;
    Ok(())
  }

  pub fn write_report_test_start<TData, W: std::io::Write>(
    writer: &mut W,
    test: &CollectedTest<TData>,
    context: &ReporterContext,
  ) -> std::io::Result<()> {
    if !context.is_parallel {
      if *NO_CAPTURE {
        writeln!(writer, "test {} ...", test.name)?;
      } else {
        write!(writer, "test {} ... ", test.name)?;
      }
    }
    Ok(())
  }

  pub fn write_report_test_end<TData, W: std::io::Write>(
    writer: &mut W,
    test: &CollectedTest<TData>,
    duration: Duration,
    result: &TestResult,
    context: &ReporterContext,
  ) -> std::io::Result<()> {
    if context.is_parallel {
      write!(writer, "test {} ... ", test.name)?;
    }
    Self::write_end_test_message(writer, result, duration)?;
    Ok(())
  }

  pub fn write_end_test_message<W: std::io::Write>(
    writer: &mut W,
    result: &TestResult,
    duration: Duration,
  ) -> std::io::Result<()> {
    fn output_sub_tests<W: std::io::Write>(
      writer: &mut W,
      indent: &str,
      sub_tests: &[SubTestResult],
    ) -> std::io::Result<()> {
      for sub_test in sub_tests {
        let duration_display = sub_test
          .result
          .duration()
          .map(|d| format!(" {}", format_duration(d)))
          .unwrap_or_default();
        match &sub_test.result {
          TestResult::Passed { .. } => {
            writeln!(
              writer,
              "{}{} {}{}",
              indent,
              sub_test.name,
              colors::green_bold("ok"),
              duration_display,
            )?;
          }
          TestResult::Ignored => {
            writeln!(
              writer,
              "{}{} {}{}",
              indent,
              sub_test.name,
              colors::gray("ignored"),
              duration_display,
            )?;
          }
          TestResult::Failed { .. } => {
            writeln!(
              writer,
              "{}{} {}{}",
              indent,
              sub_test.name,
              colors::red_bold("fail"),
              duration_display,
            )?;
          }
          TestResult::SubTests { sub_tests, .. } => {
            writeln!(
              writer,
              "{}{}{}",
              indent, sub_test.name, duration_display
            )?;
            if sub_tests.is_empty() {
              writeln!(
                writer,
                "{}  {}",
                indent,
                colors::gray("<no sub-tests>")
              )?;
            } else {
              output_sub_tests(writer, &format!("{}  ", indent), sub_tests)?;
            }
          }
        }
      }
      Ok(())
    }

    let duration_display =
      format_duration(result.duration().unwrap_or(duration));
    match result {
      TestResult::Passed { .. } => {
        writeln!(writer, "{} {}", colors::green_bold("ok"), duration_display)?;
      }
      TestResult::Ignored => {
        writeln!(writer, "{}", colors::gray("ignored"))?;
      }
      TestResult::Failed { .. } => {
        writeln!(writer, "{} {}", colors::red_bold("fail"), duration_display)?;
      }
      TestResult::SubTests { sub_tests, .. } => {
        writeln!(writer, "{}", duration_display)?;
        output_sub_tests(writer, "  ", sub_tests)?;
      }
    }

    Ok(())
  }

  pub fn write_report_long_running_test<W: std::io::Write>(
    writer: &mut W,
    test_name: &str,
  ) -> std::io::Result<()> {
    writeln!(
      writer,
      "test {} has been running for more than 60 seconds",
      test_name,
    )?;
    Ok(())
  }

  pub fn write_report_failures<TData, W: std::io::Write>(
    writer: &mut W,
    failures: &[ReporterFailure<TData>],
    total_tests: usize,
  ) -> std::io::Result<()> {
    writeln!(writer)?;
    if !failures.is_empty() {
      writeln!(writer, "failures:")?;
      writeln!(writer)?;
      for failure in failures {
        writeln!(writer, "---- {} ----", failure.test.name)?;
        writeln!(writer, "{}", String::from_utf8_lossy(&failure.output))?;
        if let Some(line_and_column) = failure.test.line_and_column {
          writeln!(
            writer,
            "Test file: {}:{}:{}",
            failure.test.path.display(),
            line_and_column.0 + 1,
            line_and_column.1 + 1
          )?;
        } else {
          writeln!(writer, "Test file: {}", failure.test.path.display())?;
        }
        writeln!(writer)?;
      }
      writeln!(writer, "failed tests:")?;
      for failure in failures {
        writeln!(writer, "    {}", failure.test.name)?;
      }
    } else {
      writeln!(writer, "{} tests passed", total_tests)?;
    }
    writeln!(writer)?;
    Ok(())
  }
}

impl<TData> Reporter<TData> for LogReporter {
  fn report_category_start(
    &self,
    category: &CollectedTestCategory<TData>,
    _context: &ReporterContext,
  ) {
    let _ = LogReporter::write_report_category_start(
      &mut std::io::stderr(),
      category,
    );
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
    let _ = LogReporter::write_report_test_start(
      &mut std::io::stderr(),
      test,
      context,
    );
  }

  fn report_test_end(
    &self,
    test: &CollectedTest<TData>,
    duration: Duration,
    result: &TestResult,
    context: &ReporterContext,
  ) {
    let _ = LogReporter::write_report_test_end(
      &mut std::io::stderr(),
      test,
      duration,
      result,
      context,
    );
  }

  fn report_running_test(&self, test_name: &str, duration: Duration) -> bool {
    if duration.as_secs() > 60 {
      let _ = LogReporter::write_report_long_running_test(
        &mut std::io::stderr(),
        test_name,
      );
      true
    } else {
      false // keep reporting until hit
    }
  }

  fn report_failures(
    &self,
    failures: &[ReporterFailure<TData>],
    total_tests: usize,
  ) {
    let _ = LogReporter::write_report_failures(
      &mut std::io::stderr(),
      failures,
      total_tests,
    );
  }
}

fn format_duration(duration: Duration) -> colors::Style<String> {
  colors::gray(format!("({}ms)", duration.as_millis()))
}

#[cfg(test)]
mod test {
  use deno_terminal::colors;

  use super::*;

  fn build_end_test_message(
    result: &TestResult,
    duration: std::time::Duration,
  ) -> String {
    let mut output = Vec::new();
    LogReporter::write_end_test_message(&mut output, result, duration).unwrap();
    String::from_utf8(output).unwrap()
  }

  #[test]
  fn test_build_end_test_message_passed() {
    assert_eq!(
      build_end_test_message(
        &super::TestResult::Passed { duration: None },
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
        duration: None,
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
      &super::TestResult::SubTests {
        duration: None,
        sub_tests: vec![
          super::SubTestResult {
            name: "step1".to_string(),
            result: super::TestResult::Passed {
              duration: Some(Duration::from_millis(20)),
            },
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
              duration: Some(Duration::from_millis(200)),
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
      },
      std::time::Duration::from_millis(10),
    );

    assert_eq!(
      message,
      format!(
        "{}\n  step1 {} {}\n  step2 {}\n  step3 {} {}\n  step4\n    sub-step1 {}\n    sub-step2 {}\n",
        colors::gray("(10ms)"),
        colors::green_bold("ok"),
        colors::gray("(20ms)"),
        colors::red_bold("fail"),
        colors::red_bold("fail"),
        colors::gray("(200ms)"),
        colors::green_bold("ok"),
        colors::red_bold("fail"),
      )
    );
  }
}
