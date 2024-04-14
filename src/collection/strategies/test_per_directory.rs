// Copyright 2018-2024 the Deno authors. MIT license.

use std::path::Path;

use crate::collection::CollectTestsError;
use crate::collection::CollectedCategoryOrTest;
use crate::collection::CollectedTest;
use crate::collection::CollectedTestCategory;
use crate::PathedIoError;

use super::helpers::append_to_category_name;
use super::helpers::read_dir_entries;
use super::FileCollectionStrategy;

/// Once a matching test file is found in a directory, traversing will stop.
#[derive(Debug, Clone)]
pub struct TestPerDirectoryCollectionStrategy {
  pub file_name: String,
}

impl FileCollectionStrategy for TestPerDirectoryCollectionStrategy {
  fn collect_tests(
    &self,
    base: &Path,
  ) -> Result<CollectedTestCategory, CollectTestsError> {
    fn collect_test_per_directory(
      category_name: &str,
      dir_path: &Path,
      dir_test_file_name: &str,
    ) -> Result<Vec<CollectedCategoryOrTest>, CollectTestsError> {
      let mut tests = vec![];

      let mut found_dir = false;
      for entry in read_dir_entries(dir_path)? {
        let path = entry.path();
        let file_type = entry
          .file_type()
          .map_err(|err| PathedIoError::new(&path, err))?;
        if file_type.is_dir() {
          found_dir = true;
          let test_file_path = path.join(dir_test_file_name);
          if test_file_path.exists() {
            let test = CollectedTest {
              name: append_to_category_name(
                category_name,
                &path.file_name().unwrap().to_string_lossy(),
              ),
              path: test_file_path,
            };
            tests.push(CollectedCategoryOrTest::Test(test));
          } else {
            let category_name = append_to_category_name(
              category_name,
              &path.file_name().unwrap().to_string_lossy(),
            );
            let children = collect_test_per_directory(
              &category_name,
              &path,
              dir_test_file_name,
            )?;
            if !children.is_empty() {
              tests.push(CollectedCategoryOrTest::Category(
                CollectedTestCategory {
                  name: category_name,
                  directory_path: path,
                  children,
                },
              ));
            }
          }
        }
      }

      // Error when the directory file can't be found in order to catch people
      // accidentally not naming the test file correctly
      // (ex. `__test__.json` instead of `__test__.jsonc` in Deno's case)
      if !found_dir {
        return Err(anyhow::anyhow!("Could not find '{}' in directory tree '{}'. Perhaps the file is named incorrectly.", dir_test_file_name, dir_path.display()).into());
      }

      Ok(tests)
    }

    let category_name = base.file_name().unwrap().to_string_lossy();
    let children =
      collect_test_per_directory(&category_name, base, &self.file_name)?;
    Ok(CollectedTestCategory {
      name: category_name.to_string(),
      directory_path: base.to_path_buf(),
      children,
    })
  }
}
