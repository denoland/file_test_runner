# file_test_runner

File-based test runner for running tests found in files via `cargo test`.

This does two main steps:

1. Collects all files from a specified directory using a provided strategy
   (`file_test_runner::collect_tests`).
1. Runs all the files as tests with a custom test runner
   (`file_test_runner::run_tests`).

The files it collects may be in any format. It's up to you to decide how they
should be structured.

## Examples

- https://github.com/denoland/deno_doc/blob/main/tests/specs_test.rs
- https://github.com/denoland/deno_graph/blob/main/tests/specs_test.rs
- https://github.com/denoland/deno/tree/main/tests/specs

## Setup

1. Add a `[[test]]` section to your Cargo.toml:

   ```toml
   [[test]]
   name = "specs"
   path = "tests/spec_test.rs"
   harness = false
   ```

2. Add a `tests/spec_test.rs` file to run the tests with a main function:

   ```rs
   use file_test_runner::collect_and_run_tests;
   use file_test_runner::collection::CollectedTest;
   use file_test_runner::collection::CollectOptions;
   use file_test_runner::collection::strategies::TestPerFileCollectionStrategy;
   use file_test_runner::RunOptions;
   use file_test_runner::TestResult;

   fn main() {
     collect_and_run_tests(
       CollectOptions {
         base: "tests/specs".into(),
         strategy: Box::new(TestPerFileCollectionStrategy {
          file_pattern: None
         }),
         filter_override: None,
       },
       RunOptions {
         parallel: false,
       },
       // custom function to run the test...
       Arc::new(|test| {
         // do something like this, or do some checks yourself and
         // return a value like TestResult::Passed
         TestResult::from_maybe_panic(AssertUnwindSafe(|| {
          run_test(test);
         }))
       })
     )
   }

   // The `test` object only contains the test name and
   // the path to the file on the file system which you can
   // then use to determine how to run your test
   fn run_test(test: &CollectedTest) {
     // Properties:
     // * `test.name` - Fully resolved name of the test.
     // * `test.path` - Path to the test file this test is associated with.
     // * `test.data` - Data associated with the test that may have been set
     //                 by the collection strategy.

     // helper function to get the text
     let file_text = test.read_to_string().unwrap();

     // now you may do whatever with the file text and
     // assert it using assert_eq! or whatever
   }
   ```

3. Add some files to the `tests/specs` directory or within sub directories of
   that directory.

4. Run `cargo test` to run the tests. Filtering should work OOTB.
