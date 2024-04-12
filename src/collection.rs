// Copyright 2018-2024 the Deno authors. MIT license.

use std::path::Path;
use std::path::PathBuf;

use regex::Regex;
use thiserror::Error;

#[derive(Debug, Clone)]
pub enum CollectedCategoryOrTest {
  Category(CollectedTestCategory),
  Test(CollectedTest),
}

#[derive(Debug, Clone)]
pub struct CollectedTestCategory {
  pub name: String,
  pub directory_path: PathBuf,
  pub children: Vec<CollectedCategoryOrTest>,
}

impl CollectedTestCategory {
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
pub struct CollectedTest {
  pub name: String,
  pub path: PathBuf,
}

impl CollectedTest {
  pub fn read_to_string(&self) -> Result<String, PathedIoError> {
    std::fs::read_to_string(&self.path)
      .map_err(|err| PathedIoError::new(&self.path, err))
  }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileCollectionStrategy {
  /// All the files in every sub directory will be traversed
  /// to find tests that match the pattern.
  ///
  /// Provide `None` to match all files.
  TestPerFile { file_pattern: Option<String> },
  /// Once a matching test file is found in a directory, traversing will stop.
  TestPerDirectory {
    /// The filename to find in the directory.
    file_name: String,
  },
}

pub struct CollectTestsOptions {
  pub base: PathBuf,
  pub strategy: FileCollectionStrategy,
  /// Name of the category to use at the top level.
  ///
  /// Ex. providing `"specs"` here will cause all tests
  /// to be prefixed with `specs::` in their name.
  pub root_category_name: String,
  /// Override the filter provided on the command line.
  ///
  /// Generally, just provide `None` here.
  pub filter_override: Option<String>,
}

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

#[derive(Debug, Error)]
pub enum CollectTestsError {
  #[error(transparent)]
  InvalidTestName(#[from] InvalidTestNameError),
  #[error(transparent)]
  Io(#[from] PathedIoError),
  #[error(transparent)]
  Regex(#[from] regex::Error),
  #[error("No tests found")]
  NoTestsFound,
  #[error("Could not find '{}' in directory tree '{}'. Perhaps the file is named incorrectly.", file_name, dir_path.display())]
  MissingDirectoryTestFile {
    dir_path: PathBuf,
    file_name: String,
  },
}

pub fn collect_tests(
  options: CollectTestsOptions,
) -> Result<CollectedTestCategory, CollectTestsError> {
  let mut category = CollectedTestCategory {
    name: options.root_category_name,
    directory_path: options.base.clone(),
    children: vec![],
  };

  match &options.strategy {
    FileCollectionStrategy::TestPerFile { file_pattern } => {
      let pattern = match file_pattern.as_ref() {
        Some(pattern) => Some(Regex::new(pattern)?),
        None => None,
      };
      category.children =
        collect_test_per_file(&category.name, &options.base, pattern.as_ref())?;
    }
    FileCollectionStrategy::TestPerDirectory { file_name } => {
      category.children =
        collect_test_per_directory(&category.name, &options.base, file_name)?;
    }
  }

  // error when no tests are found before filtering
  if category.is_empty() {
    return Err(CollectTestsError::NoTestsFound);
  }

  // filter
  let maybe_filter = options.filter_override.or_else(parse_cli_arg_filter);
  if let Some(filter) = &maybe_filter {
    category.filter_children(filter);
  }

  Ok(category)
}

fn collect_test_per_file(
  category_name: &str,
  dir_path: &PathBuf,
  pattern: Option<&Regex>,
) -> Result<Vec<CollectedCategoryOrTest>, CollectTestsError> {
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
      )?;
      let children = collect_test_per_file(&category_name, &path, pattern)?;
      if !children.is_empty() {
        tests.push(CollectedCategoryOrTest::Category(CollectedTestCategory {
          name: category_name,
          directory_path: path,
          children,
        }));
      }
    } else if file_type.is_file() {
      if let Some(pattern) = pattern {
        if !pattern.is_match(path.to_str().unwrap()) {
          continue;
        }
      }
      let test = CollectedTest {
        name: append_to_category_name(
          category_name,
          &path.file_stem().unwrap().to_string_lossy(),
        )?,
        path,
      };
      tests.push(CollectedCategoryOrTest::Test(test));
    }
  }

  Ok(tests)
}

fn collect_test_per_directory(
  category_name: &str,
  dir_path: &PathBuf,
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
          )?,
          path: test_file_path,
        };
        tests.push(CollectedCategoryOrTest::Test(test));
      } else {
        let category_name = append_to_category_name(
          category_name,
          &path.file_name().unwrap().to_string_lossy(),
        )?;
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
    return Err(CollectTestsError::MissingDirectoryTestFile {
      dir_path: dir_path.clone(),
      file_name: dir_test_file_name.to_string(),
    });
  }

  Ok(tests)
}

fn read_dir_entries(
  dir_path: &Path,
) -> Result<Vec<std::fs::DirEntry>, PathedIoError> {
  let mut entries = std::fs::read_dir(dir_path)
    .map_err(|err| PathedIoError::new(dir_path, err))?
    .collect::<Result<Vec<_>, _>>()
    .map_err(|err| PathedIoError::new(dir_path, err))?;
  entries.retain(|e| e.file_name() != ".git");
  entries.sort_by(|a, b| a.file_name().cmp(&b.file_name()));
  Ok(entries)
}

#[derive(Debug, Error)]
#[error("Invalid test name ({0}). Use only alphanumeric and underscore characters so tests can be filtered via the command line.")]
pub struct InvalidTestNameError(String);

fn append_to_category_name(
  category_name: &str,
  new_part: &str,
) -> Result<String, InvalidTestNameError> {
  let name = format!("{}::{}", category_name, new_part,);

  // only support characters that work with filtering with `cargo test`
  if !name
    .chars()
    .all(|c| c.is_alphanumeric() || matches!(c, '_' | ':'))
  {
    return Err(InvalidTestNameError(name));
  }

  Ok(name)
}

fn parse_cli_arg_filter() -> Option<String> {
  let args: Vec<String> = std::env::args().collect();
  let maybe_filter =
    args.get(1).filter(|s| !s.starts_with('-') && !s.is_empty());
  maybe_filter.cloned()
}
