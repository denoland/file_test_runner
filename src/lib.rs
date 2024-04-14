// Copyright 2018-2024 the Deno authors. MIT license.

pub mod collection;
mod runner;

pub use runner::*;

use std::path::Path;
use std::path::PathBuf;

use collection::collect_tests_or_exit;
use collection::CollectOptions;
use thiserror::Error;

#[derive(Debug, Error)]
#[error("{:#} ({})", err, path.display())]
pub struct PathedIoError {
  path: PathBuf,
  err: std::io::Error,
}

impl PathedIoError {
  pub fn new(path: &Path, err: std::io::Error) -> Self {
    Self {
      path: path.to_path_buf(),
      err,
    }
  }
}

/// Helper function to collect and run the tests.
pub fn collect_and_run_tests(
  collect_options: CollectOptions,
  run_options: RunOptions,
  run_test: RunTestFunc,
) {
  let category = collect_tests_or_exit(collect_options);
  run_tests(&category, run_options, run_test)
}
