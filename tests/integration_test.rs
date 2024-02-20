//! All integration tests are skipped by default (using the `ignore` attribute)
//! since performing builds is slow. To run them use: `cargo test -- --ignored`.

// Required due to: https://github.com/rust-lang/rust/issues/95513
#![allow(unused_crate_dependencies)]

use libcnb_test::{assert_contains, BuildConfig, PackResult, TestRunner};

#[test]
#[ignore = "integration test"]
fn test_successful_detection() {
    TestRunner::default().build(
        BuildConfig::new(get_integration_test_builder(), "tests/fixtures/basic")
            .expected_pack_result(PackResult::Success),
        |_| {},
    );
}

#[test]
#[ignore = "integration test"]
fn test_failed_detection() {
    TestRunner::default().build(
        BuildConfig::new(get_integration_test_builder(), "tests/fixtures/no_aptfile")
            .expected_pack_result(PackResult::Failure),
        |ctx| {
            assert_contains!(ctx.pack_stdout, "No Aptfile found.");
        },
    );
}

const DEFAULT_BUILDER: &str = "heroku/builder:22";

fn get_integration_test_builder() -> String {
    std::env::var("INTEGRATION_TEST_CNB_BUILDER").unwrap_or(DEFAULT_BUILDER.to_string())
}
