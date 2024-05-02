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
pub trait FileCollectionStrategy<TData = ()> {
  fn collect_tests(
    &self,
    base: &Path,
  ) -> Result<CollectedTestCategory<TData>, CollectTestsError>;
}
