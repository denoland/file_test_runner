// Copyright 2018-2024 the Deno authors. MIT license.

use std::path::Path;

mod file_test_mapper;
mod helpers;
mod test_per_directory;
mod test_per_file;

pub use file_test_mapper::*;
pub use test_per_directory::*;
pub use test_per_file::*;

use crate::collection::CollectTestsError;
use crate::collection::CollectedTestCategory;

/// Strategy for collecting tests.
pub trait TestCollectionStrategy<TData = ()> {
  /// Return a list of tests found in the provided base path.
  ///
  /// Collected tests may return optional data. This might be useful
  /// in scenarios where you want to collect multiple tests within
  /// a file using the `file_test_runner::collection::strategies::FileTestMapperStrategy`.
  fn collect_tests(
    &self,
    base: &Path,
  ) -> Result<CollectedTestCategory<TData>, CollectTestsError>;
}
