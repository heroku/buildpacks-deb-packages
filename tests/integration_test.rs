//! All integration tests are skipped by default (using the `ignore` attribute)
//! since performing builds is slow. To run them use: `cargo test -- --ignored`.

// Required due to: https://github.com/rust-lang/rust/issues/95513
#![allow(unused_crate_dependencies)]

use libcnb_test::{
    assert_contains, assert_not_contains, BuildConfig, PackResult, TestContext, TestRunner,
};

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

#[test]
#[ignore = "integration test"]
fn test_cache_restored() {
    TestRunner::default().build(
        BuildConfig::new(get_integration_test_builder(), "tests/fixtures/basic"),
        |ctx| {
            assert_contains!(ctx.pack_stdout, "# Heroku Apt Buildpack");
            assert_contains!(ctx.pack_stdout, "- Apt packages");
            assert_contains!(ctx.pack_stdout, "  - Installing packages from Aptfile");

            let config = ctx.config.clone();
            ctx.rebuild(config, |ctx| {
                assert_contains!(ctx.pack_stdout, "- Apt packages");
                assert_contains!(
                    ctx.pack_stdout,
                    "  - Skipping installation, packages already in cache"
                );

                assert_not_contains!(ctx.pack_stdout, "  - Installing packages from Aptfile");
            });
        },
    );
}

#[test]
#[ignore = "integration test"]
fn test_cache_invalidated_when_aptfile_changes() {
    TestRunner::default().build(
        BuildConfig::new(get_integration_test_builder(), "tests/fixtures/basic"),
        |ctx| {
            assert_contains!(ctx.pack_stdout, "# Heroku Apt Buildpack");
            assert_contains!(ctx.pack_stdout, "- Apt packages");
            assert_contains!(ctx.pack_stdout, "  - Installing packages from Aptfile");

            let mut config = ctx.config.clone();
            config.app_dir_preprocessor(|app_dir| {
                std::fs::write(app_dir.join("Aptfile"), "# empty\n").unwrap();
            });
            ctx.rebuild(config, |ctx| {
                assert_contains!(ctx.pack_stdout, "- Apt packages");
                assert_contains!(
                    ctx.pack_stdout,
                    "  - Invalidating installed packages (Aptfile changed)"
                );
                assert_contains!(ctx.pack_stdout, "  - Installing packages from Aptfile");

                assert_not_contains!(
                    ctx.pack_stdout,
                    "  - Skipping installation, packages already in cache"
                );
            });
        },
    );
}

#[test]
#[ignore = "integration test"]
fn test_environment_configuration() {
    TestRunner::default().build(
        BuildConfig::new(get_integration_test_builder(), "tests/fixtures/basic"),
        |ctx| {
            let layer_dir = "/layers/heroku_apt/installed_packages";

            let path = get_env_var(&ctx, "PATH");
            assert_contains!(path, &format!("{layer_dir}/bin"));
            assert_contains!(path, &format!("{layer_dir}/usr/bin"));
            assert_contains!(path, &format!("{layer_dir}/usr/sbin"));

            let ld_library_path = get_env_var(&ctx, "LD_LIBRARY_PATH");
            assert_contains!(
                ld_library_path,
                &format!("{layer_dir}/usr/lib/x86_64-linux-gnu")
            );
            assert_contains!(ld_library_path, &format!("{layer_dir}/usr/lib"));
            assert_contains!(
                ld_library_path,
                &format!("{layer_dir}/lib/x86_64-linux-gnu")
            );
            assert_contains!(ld_library_path, &format!("{layer_dir}/lib"));

            let library_path = get_env_var(&ctx, "LIBRARY_PATH");
            assert_eq!(ld_library_path, library_path);

            let include_path = get_env_var(&ctx, "INCLUDE_PATH");
            assert_contains!(
                include_path,
                &format!("{layer_dir}/usr/include/x86_64-linux-gnu")
            );
            assert_contains!(include_path, &format!("{layer_dir}/usr/include"));

            let cpath = get_env_var(&ctx, "CPATH");
            assert_eq!(include_path, cpath);

            let cpp_path = get_env_var(&ctx, "CPPPATH");
            assert_eq!(include_path, cpp_path);

            let pkg_config_path = get_env_var(&ctx, "PKG_CONFIG_PATH");
            assert_contains!(
                pkg_config_path,
                &format!("{layer_dir}/usr/lib/x86_64-linux-gnu/pkgconfig")
            );
            assert_contains!(pkg_config_path, &format!("{layer_dir}/usr/lib/pkgconfig"));
        },
    );
}

const DEFAULT_BUILDER: &str = "heroku/builder:22";

fn get_integration_test_builder() -> String {
    std::env::var("INTEGRATION_TEST_CNB_BUILDER").unwrap_or(DEFAULT_BUILDER.to_string())
}

fn get_env_var(ctx: &TestContext, env_var_name: &str) -> String {
    ctx.run_shell_command(format!("echo -n ${env_var_name}"))
        .stdout
}
