// Copyright 2018-2025 the Deno authors. MIT license.

use std::path::PathBuf;

use deno_terminal::colors;
use thiserror::Error;

use crate::PathedIoError;

use self::strategies::TestCollectionStrategy;

pub mod strategies;

#[derive(Debug, Clone)]
pub enum CollectedCategoryOrTest<T = ()> {
  Category(CollectedTestCategory<T>),
  Test(CollectedTest<T>),
}

#[derive(Debug, Clone)]
pub struct CollectedTestCategory<T = ()> {
  /// Fully resolved name of the test category.
  pub name: String,
  /// Path to the test category. May be a file or directory
  /// depending on how the test strategy collects tests.
  pub path: PathBuf,
  /// Children of the category.
  pub children: Vec<CollectedCategoryOrTest<T>>,
}

impl<T> CollectedTestCategory<T> {
  pub fn test_count(&self) -> usize {
    self
      .children
      .iter()
      .map(|child| match child {
        CollectedCategoryOrTest::Category(c) => c.test_count(),
        CollectedCategoryOrTest::Test(_) => 1,
      })
      .sum()
  }

  pub fn filter_children(&mut self, filter: &str) {
    self.children.retain_mut(|mut child| match &mut child {
      CollectedCategoryOrTest::Category(c) => {
        c.filter_children(filter);
        !c.is_empty()
      }
      CollectedCategoryOrTest::Test(t) => t.name.contains(filter),
    });
  }

  pub fn is_empty(&self) -> bool {
    for child in &self.children {
      match child {
        CollectedCategoryOrTest::Category(category) => {
          if !category.is_empty() {
            return false;
          }
        }
        CollectedCategoryOrTest::Test(_) => {
          return false;
        }
      }
    }

    true
  }

  /// Splits this category into two separate categories based on a predicate.
  /// The first category contains tests matching the predicate, the second contains those that don't.
  /// Both categories preserve the same name and path as the original.
  pub fn partition<F>(self, predicate: F) -> (Self, Self)
  where
    F: Fn(&CollectedTest<T>) -> bool + Copy,
  {
    let mut matching_children = Vec::new();
    let mut non_matching_children = Vec::new();

    for child in self.children {
      match child {
        CollectedCategoryOrTest::Category(category) => {
          let (matching_cat, non_matching_cat) = category.partition(predicate);
          if !matching_cat.is_empty() {
            matching_children
              .push(CollectedCategoryOrTest::Category(matching_cat));
          }
          if !non_matching_cat.is_empty() {
            non_matching_children
              .push(CollectedCategoryOrTest::Category(non_matching_cat));
          }
        }
        CollectedCategoryOrTest::Test(test) => {
          if predicate(&test) {
            matching_children.push(CollectedCategoryOrTest::Test(test));
          } else {
            non_matching_children.push(CollectedCategoryOrTest::Test(test));
          }
        }
      }
    }

    let matching = CollectedTestCategory {
      name: self.name.clone(),
      path: self.path.clone(),
      children: matching_children,
    };

    let non_matching = CollectedTestCategory {
      name: self.name,
      path: self.path,
      children: non_matching_children,
    };

    (matching, non_matching)
  }
}

#[derive(Debug, Clone)]
pub struct CollectedTest<T = ()> {
  /// Fully resolved name of the test.
  pub name: String,
  /// Path to the test file.
  pub path: PathBuf,
  /// Zero-indexed line and column of the test in the file.
  pub line_and_column: Option<(u32, u32)>,
  /// Data associated with the test that may have been
  /// set by the collection strategy.
  pub data: T,
}

impl<T> CollectedTest<T> {
  /// Helper to read the test file to a string.
  pub fn read_to_string(&self) -> Result<String, PathedIoError> {
    std::fs::read_to_string(&self.path)
      .map_err(|err| PathedIoError::new(&self.path, err))
  }
}

pub struct CollectOptions<TData> {
  /// Base path to start from when searching for tests.
  pub base: PathBuf,
  /// Strategy to use for collecting tests.
  pub strategy: Box<dyn TestCollectionStrategy<TData>>,
  /// Override the filter provided on the command line.
  ///
  /// Generally, just provide `None` here.
  pub filter_override: Option<String>,
}

/// Collect all the tests or exit if there are any errors.
pub fn collect_tests_or_exit<TData>(
  options: CollectOptions<TData>,
) -> CollectedTestCategory<TData> {
  match collect_tests(options) {
    Ok(category) => category,
    Err(err) => {
      eprintln!("{}: {}", colors::red_bold("error"), err);
      std::process::exit(1);
    }
  }
}

#[derive(Debug, Error)]
pub enum CollectTestsError {
  #[error(transparent)]
  InvalidTestName(#[from] InvalidTestNameError),
  #[error(transparent)]
  Io(#[from] PathedIoError),
  #[error("No tests found")]
  NoTestsFound,
  #[error(transparent)]
  Other(#[from] anyhow::Error),
}

pub fn collect_tests<TData>(
  options: CollectOptions<TData>,
) -> Result<CollectedTestCategory<TData>, CollectTestsError> {
  let mut category = options.strategy.collect_tests(&options.base)?;

  // error when no tests are found before filtering
  if category.is_empty() {
    return Err(CollectTestsError::NoTestsFound);
  }

  // ensure all test names are valid
  ensure_valid_test_names(&category)?;

  // filter
  let maybe_filter = options.filter_override.or_else(parse_cli_arg_filter);
  if let Some(filter) = &maybe_filter {
    category.filter_children(filter);
  }

  Ok(category)
}

fn ensure_valid_test_names<TData>(
  category: &CollectedTestCategory<TData>,
) -> Result<(), InvalidTestNameError> {
  for child in &category.children {
    match child {
      CollectedCategoryOrTest::Category(category) => {
        ensure_valid_test_names(category)?;
      }
      CollectedCategoryOrTest::Test(test) => {
        // only support characters that work with filtering with `cargo test`
        if !test
          .name
          .chars()
          .all(|c| c.is_alphanumeric() || matches!(c, '_' | ':'))
        {
          return Err(InvalidTestNameError(test.name.clone()));
        }
      }
    }
  }
  Ok(())
}

#[derive(Debug, Error)]
#[error(
  "Invalid test name ({0}). Use only alphanumeric and underscore characters so tests can be filtered via the command line."
)]
pub struct InvalidTestNameError(String);

/// Parses the filter from the CLI args. This can be used
/// with `category.filter_children(filter)`.
pub fn parse_cli_arg_filter() -> Option<String> {
  std::env::args()
    .nth(1)
    .filter(|s| !s.starts_with('-') && !s.is_empty())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_partition() {
    // Create a test category with nested structure
    let category = CollectedTestCategory {
      name: "root".to_string(),
      path: PathBuf::from("/root"),
      children: vec![
        CollectedCategoryOrTest::Test(CollectedTest {
          name: "test_foo".to_string(),
          path: PathBuf::from("/root/foo.rs"),
          line_and_column: None,
          data: (),
        }),
        CollectedCategoryOrTest::Test(CollectedTest {
          name: "test_bar".to_string(),
          path: PathBuf::from("/root/bar.rs"),
          line_and_column: None,
          data: (),
        }),
        CollectedCategoryOrTest::Category(CollectedTestCategory {
          name: "nested".to_string(),
          path: PathBuf::from("/root/nested"),
          children: vec![
            CollectedCategoryOrTest::Test(CollectedTest {
              name: "test_baz".to_string(),
              path: PathBuf::from("/root/nested/baz.rs"),
              line_and_column: None,
              data: (),
            }),
            CollectedCategoryOrTest::Test(CollectedTest {
              name: "test_qux".to_string(),
              path: PathBuf::from("/root/nested/qux.rs"),
              line_and_column: None,
              data: (),
            }),
          ],
        }),
      ],
    };

    // Partition based on whether name contains "ba"
    let (matching, non_matching) =
      category.partition(|test| test.name.contains("ba"));

    // Check matching category
    assert_eq!(matching.name, "root");
    assert_eq!(matching.path, PathBuf::from("/root"));
    assert_eq!(matching.test_count(), 2);

    // Check that matching contains test_bar and nested/test_baz
    assert_eq!(matching.children.len(), 2);
    match &matching.children[0] {
      CollectedCategoryOrTest::Test(test) => assert_eq!(test.name, "test_bar"),
      _ => panic!("Expected test"),
    }
    match &matching.children[1] {
      CollectedCategoryOrTest::Category(cat) => {
        assert_eq!(cat.name, "nested");
        assert_eq!(cat.children.len(), 1);
        match &cat.children[0] {
          CollectedCategoryOrTest::Test(test) => {
            assert_eq!(test.name, "test_baz")
          }
          _ => panic!("Expected test"),
        }
      }
      _ => panic!("Expected category"),
    }

    // Check non-matching category
    assert_eq!(non_matching.name, "root");
    assert_eq!(non_matching.path, PathBuf::from("/root"));
    assert_eq!(non_matching.test_count(), 2);

    // Check that non-matching contains test_foo and nested/test_qux
    assert_eq!(non_matching.children.len(), 2);
    match &non_matching.children[0] {
      CollectedCategoryOrTest::Test(test) => assert_eq!(test.name, "test_foo"),
      _ => panic!("Expected test"),
    }
    match &non_matching.children[1] {
      CollectedCategoryOrTest::Category(cat) => {
        assert_eq!(cat.name, "nested");
        assert_eq!(cat.children.len(), 1);
        match &cat.children[0] {
          CollectedCategoryOrTest::Test(test) => {
            assert_eq!(test.name, "test_qux")
          }
          _ => panic!("Expected test"),
        }
      }
      _ => panic!("Expected category"),
    }
  }

  #[test]
  fn test_partition_empty_categories_filtered() {
    // Create a category where all tests in a nested category match
    let category = CollectedTestCategory {
      name: "root".to_string(),
      path: PathBuf::from("/root"),
      children: vec![
        CollectedCategoryOrTest::Test(CollectedTest {
          name: "test_match".to_string(),
          path: PathBuf::from("/root/match.rs"),
          line_and_column: None,
          data: (),
        }),
        CollectedCategoryOrTest::Category(CollectedTestCategory {
          name: "nested".to_string(),
          path: PathBuf::from("/root/nested"),
          children: vec![CollectedCategoryOrTest::Test(CollectedTest {
            name: "test_match2".to_string(),
            path: PathBuf::from("/root/nested/match2.rs"),
            line_and_column: None,
            data: (),
          })],
        }),
      ],
    };

    let (matching, non_matching) =
      category.partition(|test| test.name.contains("match"));

    // All tests match, so matching should have everything
    assert_eq!(matching.test_count(), 2);
    assert_eq!(matching.children.len(), 2);

    // Non-matching should be empty (no children, and nested category filtered out)
    assert_eq!(non_matching.test_count(), 0);
    assert_eq!(non_matching.children.len(), 0);
    assert!(non_matching.is_empty());
  }
}
