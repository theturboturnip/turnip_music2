use crate::tests::cli::basic_test_hierarchy;

use super::test_dir;

#[test]
fn test_basic_init() {
    let fs = basic_test_hierarchy(test_dir!(
        ()
    ), false)
}
