// Copyright 2018-2024 the Deno authors. MIT license.

mod collection;
mod runner;

pub use collection::*;
pub use runner::*;

pub fn collect_and_run_tests(
  collect_options: CollectOptions,
  run_options: RunOptions,
  run_test: RunTestFunc,
) {
  let category = collect_tests_or_exit(collect_options);
  run_tests(&category, run_options, run_test)
}
