// Copyright 2018-2024 the Deno authors. MIT license.

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
}

#[derive(Debug, Clone)]
pub struct CollectedTest<T = ()> {
  /// Fully resolved name of the test.
  pub name: String,
  /// Path to the test file.
  pub path: PathBuf,
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
#[error("Invalid test name ({0}). Use only alphanumeric and underscore characters so tests can be filtered via the command line.")]
pub struct InvalidTestNameError(String);

fn parse_cli_arg_filter() -> Option<String> {
  let args: Vec<String> = std::env::args().collect();
  let maybe_filter =
    args.get(1).filter(|s| !s.starts_with('-') && !s.is_empty());
  maybe_filter.cloned()
}
