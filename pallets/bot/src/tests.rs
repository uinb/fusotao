#![cfg(test)]

use crate::mock::new_tester;

#[test]
fn test_register_bot() {
    new_tester().execute_with(|| {});
}
