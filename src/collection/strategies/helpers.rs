// Copyright 2018-2024 the Deno authors. MIT license.

use std::path::Path;

use crate::PathedIoError;

pub(crate) fn read_dir_entries(
  dir_path: &Path,
) -> Result<Vec<std::fs::DirEntry>, PathedIoError> {
  let mut entries = std::fs::read_dir(dir_path)
    .map_err(|err| PathedIoError::new(dir_path, err))?
    .collect::<Result<Vec<_>, _>>()
    .map_err(|err| PathedIoError::new(dir_path, err))?;
  entries.retain(|e| {
    !e.file_name().to_string_lossy().starts_with('.')
      && e.file_name().to_ascii_lowercase() != "readme.md"
  });
  entries.sort_by_key(|a| a.file_name());
  Ok(entries)
}

pub(crate) fn append_to_category_name(
  category_name: &str,
  new_part: &str,
) -> String {
  format!("{}::{}", category_name, new_part)
}
