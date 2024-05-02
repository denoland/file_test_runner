use std::path::Path;

use crate::collection::CollectTestsError;
use crate::collection::CollectedCategoryOrTest;
use crate::collection::CollectedTest;
use crate::collection::CollectedTestCategory;

use super::FileCollectionStrategy;

/// Maps collected tests
#[derive(Debug, Clone)]
pub struct FileTestMapperStrategy<
  TData: Clone + Send + 'static,
  TMapper: Fn(
    CollectedTest<()>,
  ) -> Result<CollectedCategoryOrTest<TData>, CollectTestsError>,
  TBaseStrategy: FileCollectionStrategy<()>,
> {
  /// Base strategy to use for collecting files.
  pub base_strategy: TBaseStrategy,
  /// Map function to map tests to a category or another test.
  pub map: TMapper,
}

impl<
    TData: Clone + Send + 'static,
    TMapper: Fn(
      CollectedTest<()>,
    ) -> Result<CollectedCategoryOrTest<TData>, CollectTestsError>,
    TBaseStrategy: FileCollectionStrategy<()>,
  > FileTestMapperStrategy<TData, TMapper, TBaseStrategy>
{
  fn map_category(
    &self,
    category: CollectedTestCategory<()>,
  ) -> Result<CollectedTestCategory<TData>, CollectTestsError> {
    let mut new_children = Vec::with_capacity(category.children.len());
    for child in category.children {
      match child {
        CollectedCategoryOrTest::Category(c) => {
          new_children
            .push(CollectedCategoryOrTest::Category(self.map_category(c)?));
        }
        CollectedCategoryOrTest::Test(t) => {
          new_children.push((self.map)(t)?);
        }
      }
    }
    Ok(CollectedTestCategory {
      name: category.name,
      path: category.path,
      children: new_children,
    })
  }
}

impl<
    TData: Clone + Send + 'static,
    TMapper: Fn(
      CollectedTest<()>,
    ) -> Result<CollectedCategoryOrTest<TData>, CollectTestsError>,
    TBaseStrategy: FileCollectionStrategy<()>,
  > FileCollectionStrategy<TData>
  for FileTestMapperStrategy<TData, TMapper, TBaseStrategy>
{
  fn collect_tests(
    &self,
    base: &Path,
  ) -> Result<CollectedTestCategory<TData>, CollectTestsError> {
    let category = self.base_strategy.collect_tests(base)?;
    self.map_category(category)
  }
}
