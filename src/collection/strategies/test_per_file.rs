// Copyright 2018-2025 the Deno authors. MIT license.

use std::path::Path;

use regex::Regex;

use crate::PathedIoError;
use crate::collection::CollectTestsError;
use crate::collection::CollectedCategoryOrTest;
use crate::collection::CollectedTest;
use crate::collection::CollectedTestCategory;

use super::TestCollectionStrategy;
use super::helpers::append_to_category_name;
use super::helpers::read_dir_entries;

/// All the files in every sub directory will be traversed
/// to find tests that match the pattern.
///
/// Provide `None` to match all files.
///
/// Note: This ignores readme.md files and hidden directories
/// starting with a period.
#[derive(Debug, Clone, Default)]
pub struct TestPerFileCollectionStrategy {
  pub file_pattern: Option<String>,
}

impl TestCollectionStrategy<()> for TestPerFileCollectionStrategy {
  fn collect_tests(
    &self,
    base: &Path,
  ) -> Result<CollectedTestCategory<()>, CollectTestsError> {
    fn collect_test_per_file(
      category_name: &str,
      dir_path: &Path,
      pattern: Option<&Regex>,
    ) -> Result<Vec<CollectedCategoryOrTest<()>>, CollectTestsError> {
      let mut tests = vec![];

      for entry in read_dir_entries(dir_path)? {
        let path = entry.path();
        let file_type = entry
          .file_type()
          .map_err(|err| PathedIoError::new(&path, err))?;
        if file_type.is_dir() {
          let category_name = append_to_category_name(
            category_name,
            &path.file_name().unwrap().to_string_lossy(),
          );
          let children = collect_test_per_file(&category_name, &path, pattern)?;
          if !children.is_empty() {
            tests.push(CollectedCategoryOrTest::Category(
              CollectedTestCategory {
                name: category_name,
                path,
                children,
              },
            ));
          }
        } else if file_type.is_file() {
          if let Some(pattern) = pattern
            && !pattern.is_match(path.to_str().unwrap()) {
              continue;
            }
          let test = CollectedTest {
            name: append_to_category_name(
              category_name,
              &path.file_stem().unwrap().to_string_lossy(),
            ),
            path,
            data: (),
          };
          tests.push(CollectedCategoryOrTest::Test(test));
        }
      }

      Ok(tests)
    }

    let pattern = match self.file_pattern.as_ref() {
      Some(pattern) => Some(Regex::new(pattern).map_err(anyhow::Error::from)?),
      None => None,
    };
    let category_name = base.file_name().unwrap().to_string_lossy();
    let children =
      collect_test_per_file(&category_name, base, pattern.as_ref())?;
    Ok(CollectedTestCategory {
      name: category_name.to_string(),
      path: base.to_path_buf(),
      children,
    })
  }
}
