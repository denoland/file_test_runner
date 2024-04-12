# file_test_runner

File-based test runner for running tests found in files via `cargo test`.

This does two main steps:

1. Collects all the tests from the file system (`file_test_runner::collect_tests`).
1. Runs all the tests with a custom test runner (`file_test_runner::run_tests`).

## Setup

1. Add a `[[test]]` section to your Cargo.toml:

   ```
   [[test]]
   name = "specs"
   path = "tests/spec_test.rs"
   harness = false
   ```

2. Add a `tests/spec_test.rs` file to run the tests with a main function:

   ```rs
   pub fn main() {
     file_test_runner::collect_and_run_tests(
       file_test_runner::CollectOptions {
        // ...
       },
       file_test_runner::RunOptions {
        // ...
       },
       Arc::new(|test| {
         // ...custom function to run the test goes here...
         // The `test` object only contains the test name and
         // the path to the file on the file system which you can
         // then use to determine how to run your test

         // or return `Failed` with the test output
         file_test_runner::TestResult::Passed
       })
     )
   }
   ```

3. Run `cargo test` to run the tests. Filtering should work OOTB.
