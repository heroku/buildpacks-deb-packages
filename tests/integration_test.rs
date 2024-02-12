mod macros;

use libcnb::data::buildpack_id;
use libcnb_test::{
    assert_contains, assert_not_contains, BuildConfig, BuildpackReference, TestContext, TestRunner,
};
use std::path::PathBuf;

#[test]
#[ignore = "integration test"]
fn test_basic_package_install() {
    apt_integration_test("./fixtures/basic", |ctx| {
        assert_contains!(ctx.pack_stdout, "Heroku Apt Buildpack");
        assert_contains!(ctx.pack_stdout, "Apt packages cache");
        assert_contains!(ctx.pack_stdout, "Creating cache directory");
        assert_matches!(ctx.pack_stdout, r"Using apt version `\d+\.\d+\.\d+`");
        assert_matches!(
            ctx.pack_stdout,
            r"Running `apt-get --config-file /tmp/.*/apt\.conf update`"
        );
        assert_matches!(
            ctx.pack_stdout,
            r"Running `apt-get --allow-downgrades --allow-remove-essential --allow-change-held-packages --assume-yes --config-file /tmp/.*/apt\.conf --download-only --reinstall install byacc`"
        );
        assert_matches!(
            ctx.pack_stdout,
            r#"Running `dpkg --extract "/tmp/.*/cache/archives/byacc.*\.deb" /layers/heroku_apt/installed_packages`"#
        );
    });
}

#[test]
#[ignore = "integration test"]
fn test_cache_restored() {
    apt_integration_test("./fixtures/basic", |ctx| {
        assert_contains!(ctx.pack_stdout, "Heroku Apt Buildpack");
        assert_contains!(ctx.pack_stdout, "Apt packages cache");
        assert_contains!(ctx.pack_stdout, "Creating cache directory");

        let config = ctx.config.clone();
        ctx.rebuild(config, |ctx| {
            assert_not_contains!(ctx.pack_stdout, "Creating cache directory");
            assert_contains!(ctx.pack_stdout, "Restoring installed packages");
        });
    });
}

#[test]
#[ignore = "integration test"]
fn test_cache_invalidated_when_aptfile_changes() {
    apt_integration_test("./fixtures/basic", |ctx| {
        assert_contains!(ctx.pack_stdout, "Heroku Apt Buildpack");
        assert_contains!(ctx.pack_stdout, "Apt packages cache");
        assert_contains!(ctx.pack_stdout, "Creating cache directory");

        let mut config = ctx.config.clone();
        config.app_dir_preprocessor(|app_dir| {
            std::fs::write(app_dir.join("Aptfile"), "").unwrap();
        });
        ctx.rebuild(config, |ctx| {
            assert_contains!(
                ctx.pack_stdout,
                "Invalidating installed packages (Aptfile changed)"
            );
            assert_contains!(ctx.pack_stdout, "Creating cache directory");
        });
    });
}

const DEFAULT_BUILDER: &str = "heroku/builder:22";

fn get_integration_test_builder() -> String {
    std::env::var("INTEGRATION_TEST_CNB_BUILDER").unwrap_or(DEFAULT_BUILDER.to_string())
}

fn apt_integration_test(fixture: &str, test_body: fn(TestContext)) {
    apt_integration_test_with_config(fixture, |_| {}, test_body);
}

fn apt_integration_test_with_config(
    fixture: &str,
    with_config: fn(&mut BuildConfig),
    test_body: fn(TestContext),
) {
    integration_test_with_config(
        fixture,
        with_config,
        test_body,
        &[BuildpackReference::WorkspaceBuildpack(buildpack_id!(
            "heroku/apt"
        ))],
    );
}

fn integration_test_with_config(
    fixture: &str,
    with_config: fn(&mut BuildConfig),
    test_body: fn(TestContext),
    buildpacks: &[BuildpackReference],
) {
    let cargo_manifest_dir = std::env::var("CARGO_MANIFEST_DIR")
        .map(PathBuf::from)
        .expect("The CARGO_MANIFEST_DIR should be automatically set by Cargo when running tests but it was not");

    let builder = get_integration_test_builder();
    let app_dir = cargo_manifest_dir.join("tests").join(fixture);

    let mut build_config = BuildConfig::new(builder, app_dir);
    build_config.buildpacks(buildpacks);
    with_config(&mut build_config);

    TestRunner::default().build(build_config, test_body);
}
