//! All integration tests are skipped by default (using the `ignore` attribute)
//! since performing builds is slow. To run them use: `cargo test -- --ignored`.

// Required due to: https://github.com/rust-lang/rust/issues/95513
#![allow(unused_crate_dependencies)]
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::str::FromStr;

use libcnb_test::{assert_contains, assert_contains_match, assert_not_contains, BuildConfig, PackResult, TestContext, TestRunner};
use toml_edit::{value, Array, DocumentMut, InlineTable};

#[test]
#[ignore = "integration test"]
fn test_successful_detection() {
    integration_test_with_config(
        "fixtures/general_usage",
        |config| {
            config.expected_pack_result(PackResult::Success);
        },
        |_| {},
    );
}

#[test]
#[ignore = "integration test"]
fn test_failed_detection_when_no_project_file_exists() {
    integration_test_with_config(
        "fixtures/no_project_file",
        |config| {
            config.expected_pack_result(PackResult::Failure);
        },
        |ctx| {
            assert_contains!(ctx.pack_stdout, "No project.toml file found.");
        },
    );
}

#[test]
#[ignore = "integration test"]
fn test_failed_detection_when_project_file_with_empty_config_exists() {
    integration_test_with_config(
        "fixtures/project_file_with_empty_config",
        |config| {
            config.expected_pack_result(PackResult::Failure);
        },
        |ctx| {
            assert_contains!(ctx.pack_stdout, "No configured packages to install found in project.toml file.");
        },
    );
}

#[test]
#[ignore = "integration test"]
fn test_failed_detection_when_project_file_has_no_config() {
    integration_test_with_config(
        "fixtures/project_file_with_no_config",
        |config| {
            config.expected_pack_result(PackResult::Failure);
        },
        |ctx| {
            assert_contains!(ctx.pack_stdout, "No configured packages to install found in project.toml file.");
        },
    );
}

#[test]
#[ignore = "integration test"]
#[allow(clippy::too_many_lines)]
fn test_general_usage_output() {
    integration_test("fixtures/general_usage", |ctx| {
        assert_contains_match!(ctx.pack_stdout, r"# Heroku Debian Packages Buildpack \(v\d+\.\d+\.\d+\)");

        match (get_integration_test_builder().as_str(), get_integration_test_arch().as_str()) {
            ("heroku/builder:22", "amd64") => {
                assert_contains!(ctx.pack_stdout, "## Distribution Info");

                assert_contains!(ctx.pack_stdout, "- Name: ubuntu");
                assert_contains!(ctx.pack_stdout, "- Version: 22.04");
                assert_contains!(ctx.pack_stdout, "- Codename: jammy");
                assert_contains!(ctx.pack_stdout, "- Architecture: amd64");

                assert_contains!(ctx.pack_stdout, "## Creating package index");

                assert_contains_match!(ctx.pack_stdout, r"\[GET\] http://archive.ubuntu.com/ubuntu/dists/jammy/InRelease");
                assert_contains_match!(ctx.pack_stdout, r"\[GET\] http://archive.ubuntu.com/ubuntu/dists/jammy/main/binary-amd64/by-hash/SHA256/[0-9a-f]+$");
                assert_contains_match!(ctx.pack_stdout, r"\[GET\] http://archive.ubuntu.com/ubuntu/dists/jammy/universe/binary-amd64/by-hash/SHA256/[0-9a-f]+$");
                assert_contains_match!(ctx.pack_stdout, r"\[GET\] http://archive.ubuntu.com/ubuntu/dists/jammy-security/InRelease");
                assert_contains_match!(ctx.pack_stdout, r"\[GET\] http://archive.ubuntu.com/ubuntu/dists/jammy-security/main/binary-amd64/by-hash/SHA256/[0-9a-f]+$");
                assert_contains_match!(ctx.pack_stdout, r"\[GET\] http://archive.ubuntu.com/ubuntu/dists/jammy-security/universe/binary-amd64/by-hash/SHA256/[0-9a-f]+$");
                assert_contains_match!(ctx.pack_stdout, r"\[GET\] http://archive.ubuntu.com/ubuntu/dists/jammy-updates/InRelease");
                assert_contains_match!(ctx.pack_stdout, r"\[GET\] http://archive.ubuntu.com/ubuntu/dists/jammy-updates/main/binary-amd64/by-hash/SHA256/[0-9a-f]+$");
                assert_contains_match!(ctx.pack_stdout, r"\[GET\] http://archive.ubuntu.com/ubuntu/dists/jammy-updates/universe/binary-amd64/by-hash/SHA256/[0-9a-f]+$");

                assert_contains!(ctx.pack_stdout, "Processing package files...");
                assert_contains_match!(ctx.pack_stdout, r"Indexed \d+ packages \(\d+ms\)");

                assert_contains!(ctx.pack_stdout, "## Determining packages to install");

                assert_contains!(ctx.pack_stdout, "Adding libgwenhywfar79@5.9.0-1");
                assert_contains!(ctx.pack_stdout, "Adding libgwenhywfar-data@5.9.0-1 [from libgwenhywfar79]");
                assert_contains!(
                    ctx.pack_stdout,
                    "! Skipping libgwenhywfar-data because libgwenhywfar-data@5.9.0-1 was already installed as a dependency of libgwenhywfar79 (consider removing libgwenhywfar-data from your project.toml configuration for this buildpack)"
                );
                assert_contains!(ctx.pack_stdout, "Adding xmlsec1@1.2.33-1build2");
                assert_contains!(ctx.pack_stdout, "! Skipping wget because wget@1.21.2-2ubuntu1 is already installed on the system (consider removing wget from your project.toml configuration for this buildpack)");
                assert_contains!(
                    ctx.pack_stdout,
                    "! Virtual package libvips is provided by libvips42@8.12.1-1build1 (consider replacing libvips for libvips42 in your project.toml configuration for this buildpack)"
                );
                assert_contains!(
                    ctx.pack_stdout,
                    "! Skipping libvips42 because libvips42@8.12.1-1build1 is already installed on the system (consider removing libvips42 from your project.toml configuration for this buildpack)"
                );

                assert_contains!(ctx.pack_stdout, "## Installing packages");

                assert_contains!(ctx.pack_stdout, "Downloading libgwenhywfar79");
                assert_contains!(ctx.pack_stdout, "Downloading xmlsec1");
                assert_contains!(ctx.pack_stdout, "Downloading libgwenhywfar-data");
                assert_contains!(ctx.pack_stdout, "Extracting xmlsec1");
                assert_contains!(ctx.pack_stdout, "Installing xmlsec1 → /layers/heroku_debian-packages/xmlsec1");
                assert_contains!(ctx.pack_stdout, "Extracting libgwenhywfar-data");
                assert_contains!(ctx.pack_stdout, "Installing libgwenhywfar-data → /layers/heroku_debian-packages/libgwenhywfar-data");
                assert_contains!(ctx.pack_stdout, "Extracting libgwenhywfar79");
                assert_contains!(ctx.pack_stdout, "Installing libgwenhywfar79 → /layers/heroku_debian-packages/libgwenhywfar79");

                let path = get_env_var(&ctx, "PATH");
                let ld_library_path = get_env_var(&ctx, "LD_LIBRARY_PATH");
                let library_path = get_env_var(&ctx, "LIBRARY_PATH");

                assert_eq!(ld_library_path, library_path);

                assert_contains!(path, "/layers/heroku_debian-packages/xmlsec1/bin");
                assert_contains!(path, "/layers/heroku_debian-packages/xmlsec1/usr/bin");
                assert_contains!(path, "/layers/heroku_debian-packages/xmlsec1/usr/sbin");
                assert_contains!(ld_library_path, "/layers/heroku_debian-packages/xmlsec1/usr/lib/x86_64-linux-gnu");
                assert_contains!(ld_library_path, "/layers/heroku_debian-packages/xmlsec1/usr/lib");
                assert_contains!(ld_library_path, "/layers/heroku_debian-packages/xmlsec1/lib/x86_64-linux-gnu");
                assert_contains!(ld_library_path, "/layers/heroku_debian-packages/xmlsec1/lib");

                assert_contains!(path, "/layers/heroku_debian-packages/libgwenhywfar-data/bin");
                assert_contains!(path, "/layers/heroku_debian-packages/libgwenhywfar-data/usr/bin");
                assert_contains!(path, "/layers/heroku_debian-packages/libgwenhywfar-data/usr/sbin");

                assert_contains!(path, "/layers/heroku_debian-packages/libgwenhywfar79/bin");
                assert_contains!(path, "/layers/heroku_debian-packages/libgwenhywfar79/usr/bin");
                assert_contains!(path, "/layers/heroku_debian-packages/libgwenhywfar79/usr/sbin");
            }
            ("heroku/builder:24", "amd64") => {
                assert_contains!(ctx.pack_stdout, "## Distribution Info");

                assert_contains!(ctx.pack_stdout, "- Name: ubuntu");
                assert_contains!(ctx.pack_stdout, "- Version: 24.04");
                assert_contains!(ctx.pack_stdout, "- Codename: noble");
                assert_contains!(ctx.pack_stdout, "- Architecture: amd64");

                assert_contains!(ctx.pack_stdout, "## Creating package index");

                assert_contains_match!(ctx.pack_stdout, r"\[GET\] http://archive.ubuntu.com/ubuntu/dists/noble/InRelease");
                assert_contains_match!(ctx.pack_stdout, r"\[GET\] http://archive.ubuntu.com/ubuntu/dists/noble/main/binary-amd64/by-hash/SHA256/[0-9a-f]+$");
                assert_contains_match!(ctx.pack_stdout, r"\[GET\] http://archive.ubuntu.com/ubuntu/dists/noble/universe/binary-amd64/by-hash/SHA256/[0-9a-f]+$");
                assert_contains_match!(ctx.pack_stdout, r"\[GET\] http://security.ubuntu.com/ubuntu/dists/noble-security/InRelease");
                assert_contains_match!(ctx.pack_stdout, r"\[GET\] http://security.ubuntu.com/ubuntu/dists/noble-security/main/binary-amd64/by-hash/SHA256/[0-9a-f]+$");
                assert_contains_match!(ctx.pack_stdout, r"\[GET\] http://security.ubuntu.com/ubuntu/dists/noble-security/universe/binary-amd64/by-hash/SHA256/[0-9a-f]+$");
                assert_contains_match!(ctx.pack_stdout, r"\[GET\] http://archive.ubuntu.com/ubuntu/dists/noble-updates/InRelease");
                assert_contains_match!(ctx.pack_stdout, r"\[GET\] http://archive.ubuntu.com/ubuntu/dists/noble-updates/main/binary-amd64/by-hash/SHA256/[0-9a-f]+$");
                assert_contains_match!(ctx.pack_stdout, r"\[GET\] http://archive.ubuntu.com/ubuntu/dists/noble-updates/universe/binary-amd64/by-hash/SHA256/[0-9a-f]+$");

                assert_contains!(ctx.pack_stdout, "Processing package files...");
                assert_contains_match!(ctx.pack_stdout, r"Indexed \d+ packages \(\d+ms\)");

                assert_contains!(ctx.pack_stdout, "## Determining packages to install");

                assert_contains!(
                    ctx.pack_stdout,
                    "! Virtual package libgwenhywfar79 is provided by libgwenhywfar79t64@5.10.2-2.1build4 (consider replacing libgwenhywfar79 for libgwenhywfar79t64 in your project.toml configuration for this buildpack)"
                );
                assert_contains!(ctx.pack_stdout, "Adding libgwenhywfar79t64@5.10.2-2.1build4");
                assert_contains!(ctx.pack_stdout, "Adding libgwenhywfar-data@5.10.2-2.1build4 [from libgwenhywfar79t64]");
                assert_contains!(
                    ctx.pack_stdout,
                    "! Skipping libgwenhywfar-data because libgwenhywfar-data@5.10.2-2.1build4 was already installed as a dependency of libgwenhywfar79t64 (consider removing libgwenhywfar-data from your project.toml configuration for this buildpack)"
                );
                assert_contains!(ctx.pack_stdout, "Adding xmlsec1@1.2.39-5build2");
                assert_contains!(ctx.pack_stdout, "! Skipping wget because wget@1.21.4-1ubuntu4 is already installed on the system (consider removing wget from your project.toml configuration for this buildpack)");
                assert_contains!(
                    ctx.pack_stdout,
                    "! Virtual package libvips is provided by libvips42t64@8.15.1-1.1build4 (consider replacing libvips for libvips42t64 in your project.toml configuration for this buildpack)"
                );
                assert_contains!(
                    ctx.pack_stdout,
                    "! Skipping libvips42t64 because libvips42t64@8.15.1-1.1build4 is already installed on the system (consider removing libvips42t64 from your project.toml configuration for this buildpack)"
                );

                assert_contains!(ctx.pack_stdout, "## Installing packages");

                assert_contains!(ctx.pack_stdout, "Downloading libgwenhywfar79t64");
                assert_contains!(ctx.pack_stdout, "Downloading xmlsec1");
                assert_contains!(ctx.pack_stdout, "Downloading libgwenhywfar-data");
                assert_contains!(ctx.pack_stdout, "Extracting xmlsec1");
                assert_contains!(ctx.pack_stdout, "Extracting libgwenhywfar-data");
                assert_contains!(ctx.pack_stdout, "Installing xmlsec1 → /layers/heroku_debian-packages/xmlsec1");
                assert_contains!(ctx.pack_stdout, "Installing libgwenhywfar-data → /layers/heroku_debian-packages/libgwenhywfar-data");
                assert_contains!(ctx.pack_stdout, "Extracting libgwenhywfar79t64");
                assert_contains!(ctx.pack_stdout, "Installing libgwenhywfar79t64 → /layers/heroku_debian-packages/libgwenhywfar79t64");
            }
            ("heroku/builder:24", "arm64") => {
                assert_contains!(ctx.pack_stdout, "## Distribution Info");

                assert_contains!(ctx.pack_stdout, "- Name: ubuntu");
                assert_contains!(ctx.pack_stdout, "- Version: 24.04");
                assert_contains!(ctx.pack_stdout, "- Codename: noble");
                assert_contains!(ctx.pack_stdout, "- Architecture: arm64");

                assert_contains!(ctx.pack_stdout, "## Creating package index");

                assert_contains_match!(ctx.pack_stdout, r"\[GET\] http://ports.ubuntu.com/ubuntu-ports/dists/noble/InRelease");
                assert_contains_match!(ctx.pack_stdout, r"\[GET\] http://ports.ubuntu.com/ubuntu-ports/dists/noble/main/binary-arm64/by-hash/SHA256/[0-9a-f]+$");
                assert_contains_match!(ctx.pack_stdout, r"\[GET\] http://ports.ubuntu.com/ubuntu-ports/dists/noble/universe/binary-arm64/by-hash/SHA256/[0-9a-f]+$");
                assert_contains_match!(ctx.pack_stdout, r"\[GET\] http://ports.ubuntu.com/ubuntu-ports/dists/noble-security/InRelease");
                assert_contains_match!(ctx.pack_stdout, r"\[GET\] http://ports.ubuntu.com/ubuntu-ports/dists/noble-security/main/binary-arm64/by-hash/SHA256/[0-9a-f]+$");
                assert_contains_match!(ctx.pack_stdout, r"\[GET\] http://ports.ubuntu.com/ubuntu-ports/dists/noble-security/universe/binary-arm64/by-hash/SHA256/[0-9a-f]+$");
                assert_contains_match!(ctx.pack_stdout, r"\[GET\] http://ports.ubuntu.com/ubuntu-ports/dists/noble-updates/InRelease");
                assert_contains_match!(ctx.pack_stdout, r"\[GET\] http://ports.ubuntu.com/ubuntu-ports/dists/noble-updates/main/binary-arm64/by-hash/SHA256/[0-9a-f]+$");
                assert_contains_match!(ctx.pack_stdout, r"\[GET\] http://ports.ubuntu.com/ubuntu-ports/dists/noble-updates/universe/binary-arm64/by-hash/SHA256/[0-9a-f]+$");

                assert_contains!(ctx.pack_stdout, "Processing package files...");
                assert_contains_match!(ctx.pack_stdout, r"Indexed \d+ packages \(\d+ms\)");

                assert_contains!(ctx.pack_stdout, "## Determining packages to install");

                assert_contains!(
                    ctx.pack_stdout,
                    "! Virtual package libgwenhywfar79 is provided by libgwenhywfar79t64@5.10.2-2.1build4 (consider replacing libgwenhywfar79 for libgwenhywfar79t64 in your project.toml configuration for this buildpack)"
                );
                assert_contains!(ctx.pack_stdout, "Adding libgwenhywfar79t64@5.10.2-2.1build4");
                assert_contains!(ctx.pack_stdout, "Adding libgwenhywfar-data@5.10.2-2.1build4 [from libgwenhywfar79t64]");
                assert_contains!(
                    ctx.pack_stdout,
                    "! Skipping libgwenhywfar-data because libgwenhywfar-data@5.10.2-2.1build4 was already installed as a dependency of libgwenhywfar79t64 (consider removing libgwenhywfar-data from your project.toml configuration for this buildpack)"
                );
                assert_contains!(ctx.pack_stdout, "Adding xmlsec1@1.2.39-5build2");
                assert_contains!(ctx.pack_stdout, "! Skipping wget because wget@1.21.4-1ubuntu4 is already installed on the system (consider removing wget from your project.toml configuration for this buildpack)");
                assert_contains!(
                    ctx.pack_stdout,
                    "! Virtual package libvips is provided by libvips42t64@8.15.1-1.1build4 (consider replacing libvips for libvips42t64 in your project.toml configuration for this buildpack)"
                );
                assert_contains!(
                    ctx.pack_stdout,
                    "! Skipping libvips42t64 because libvips42t64@8.15.1-1.1build4 is already installed on the system (consider removing libvips42t64 from your project.toml configuration for this buildpack)"
                );

                assert_contains!(ctx.pack_stdout, "## Installing packages");

                assert_contains!(ctx.pack_stdout, "Downloading libgwenhywfar79t64");
                assert_contains!(ctx.pack_stdout, "Downloading xmlsec1");
                assert_contains!(ctx.pack_stdout, "Downloading libgwenhywfar-data");
                assert_contains!(ctx.pack_stdout, "Extracting xmlsec1");
                assert_contains!(ctx.pack_stdout, "Extracting libgwenhywfar-data");
                assert_contains!(ctx.pack_stdout, "Installing xmlsec1 → /layers/heroku_debian-packages/xmlsec1");
                assert_contains!(ctx.pack_stdout, "Installing libgwenhywfar-data → /layers/heroku_debian-packages/libgwenhywfar-data");
                assert_contains!(ctx.pack_stdout, "Extracting libgwenhywfar79t64");
                assert_contains!(ctx.pack_stdout, "Installing libgwenhywfar79t64 → /layers/heroku_debian-packages/libgwenhywfar79t64");
            }
            _ => panic_unsupported_test_configuration(),
        }
    });
}

#[test]
#[ignore = "integration test"]
fn test_general_usage_output_on_rebuild() {
    integration_test("fixtures/general_usage", |ctx| {
        let config = ctx.config.clone();
        ctx.rebuild(config, |ctx| {
            assert_contains_match!(ctx.pack_stdout, r"# Heroku Debian Packages Buildpack \(v\d+\.\d+\.\d+\)");

            match (get_integration_test_builder().as_str(), get_integration_test_arch().as_str()) {
                ("heroku/builder:22", "amd64") => {
                    assert_contains_match!(ctx.pack_stdout, r"\[CACHED\] http://archive.ubuntu.com/ubuntu/dists/jammy/InRelease");
                    assert_contains_match!(ctx.pack_stdout, r"\[CACHED\] http://archive.ubuntu.com/ubuntu/dists/jammy/main/binary-amd64/by-hash/SHA256/[0-9a-f]+$");
                    assert_contains_match!(ctx.pack_stdout, r"\[CACHED\] http://archive.ubuntu.com/ubuntu/dists/jammy/universe/binary-amd64/by-hash/SHA256/[0-9a-f]+$");
                    assert_contains_match!(ctx.pack_stdout, r"\[CACHED\] http://archive.ubuntu.com/ubuntu/dists/jammy-security/InRelease");
                    assert_contains_match!(ctx.pack_stdout, r"\[CACHED\] http://archive.ubuntu.com/ubuntu/dists/jammy-security/main/binary-amd64/by-hash/SHA256/[0-9a-f]+$");
                    assert_contains_match!(ctx.pack_stdout, r"\[CACHED\] http://archive.ubuntu.com/ubuntu/dists/jammy-security/universe/binary-amd64/by-hash/SHA256/[0-9a-f]+$");
                    assert_contains_match!(ctx.pack_stdout, r"\[CACHED\] http://archive.ubuntu.com/ubuntu/dists/jammy-updates/InRelease");
                    assert_contains_match!(ctx.pack_stdout, r"\[CACHED\] http://archive.ubuntu.com/ubuntu/dists/jammy-updates/main/binary-amd64/by-hash/SHA256/[0-9a-f]+$");
                    assert_contains_match!(ctx.pack_stdout, r"\[CACHED\] http://archive.ubuntu.com/ubuntu/dists/jammy-updates/universe/binary-amd64/by-hash/SHA256/[0-9a-f]+$");

                    assert_contains!(ctx.pack_stdout, "Restoring xmlsec1 from cache");
                    assert_contains!(ctx.pack_stdout, "Restoring libgwenhywfar-data from cache");
                    assert_contains!(ctx.pack_stdout, "Restoring libgwenhywfar79 from cache");
                }
                ("heroku/builder:24", "amd64") => {
                    assert_contains_match!(ctx.pack_stdout, r"\[CACHED\] http://archive.ubuntu.com/ubuntu/dists/noble/InRelease");
                    assert_contains_match!(ctx.pack_stdout, r"\[CACHED\] http://archive.ubuntu.com/ubuntu/dists/noble/main/binary-amd64/by-hash/SHA256/[0-9a-f]+$");
                    assert_contains_match!(ctx.pack_stdout, r"\[CACHED\] http://archive.ubuntu.com/ubuntu/dists/noble/universe/binary-amd64/by-hash/SHA256/[0-9a-f]+$");
                    assert_contains_match!(ctx.pack_stdout, r"\[CACHED\] http://security.ubuntu.com/ubuntu/dists/noble-security/InRelease");
                    assert_contains_match!(ctx.pack_stdout, r"\[CACHED\] http://security.ubuntu.com/ubuntu/dists/noble-security/main/binary-amd64/by-hash/SHA256/[0-9a-f]+$");
                    assert_contains_match!(ctx.pack_stdout, r"\[CACHED\] http://security.ubuntu.com/ubuntu/dists/noble-security/universe/binary-amd64/by-hash/SHA256/[0-9a-f]+$");
                    assert_contains_match!(ctx.pack_stdout, r"\[CACHED\] http://archive.ubuntu.com/ubuntu/dists/noble-updates/InRelease");
                    assert_contains_match!(ctx.pack_stdout, r"\[CACHED\] http://archive.ubuntu.com/ubuntu/dists/noble-updates/main/binary-amd64/by-hash/SHA256/[0-9a-f]+$");
                    assert_contains_match!(ctx.pack_stdout, r"\[CACHED\] http://archive.ubuntu.com/ubuntu/dists/noble-updates/universe/binary-amd64/by-hash/SHA256/[0-9a-f]+$");

                    assert_contains!(ctx.pack_stdout, "Restoring xmlsec1 from cache");
                    assert_contains!(ctx.pack_stdout, "Restoring libgwenhywfar-data from cache");
                    assert_contains!(ctx.pack_stdout, "Restoring libgwenhywfar79t64 from cache");
                }
                ("heroku/builder:24", "arm64") => {
                    assert_contains_match!(ctx.pack_stdout, r"\[CACHED\] http://ports.ubuntu.com/ubuntu-ports/dists/noble/InRelease");
                    assert_contains_match!(ctx.pack_stdout, r"\[CACHED\] http://ports.ubuntu.com/ubuntu-ports/dists/noble/main/binary-arm64/by-hash/SHA256/[0-9a-f]+$");
                    assert_contains_match!(ctx.pack_stdout, r"\[CACHED\] http://ports.ubuntu.com/ubuntu-ports/dists/noble/universe/binary-arm64/by-hash/SHA256/[0-9a-f]+$");
                    assert_contains_match!(ctx.pack_stdout, r"\[CACHED\] http://ports.ubuntu.com/ubuntu-ports/dists/noble-security/InRelease");
                    assert_contains_match!(ctx.pack_stdout, r"\[CACHED\] http://ports.ubuntu.com/ubuntu-ports/dists/noble-security/main/binary-arm64/by-hash/SHA256/[0-9a-f]+$");
                    assert_contains_match!(ctx.pack_stdout, r"\[CACHED\] http://ports.ubuntu.com/ubuntu-ports/dists/noble-security/universe/binary-arm64/by-hash/SHA256/[0-9a-f]+$");
                    assert_contains_match!(ctx.pack_stdout, r"\[CACHED\] http://ports.ubuntu.com/ubuntu-ports/dists/noble-updates/InRelease");
                    assert_contains_match!(ctx.pack_stdout, r"\[CACHED\] http://ports.ubuntu.com/ubuntu-ports/dists/noble-updates/main/binary-arm64/by-hash/SHA256/[0-9a-f]+$");
                    assert_contains_match!(ctx.pack_stdout, r"\[CACHED\] http://ports.ubuntu.com/ubuntu-ports/dists/noble-updates/universe/binary-arm64/by-hash/SHA256/[0-9a-f]+$");

                    assert_contains!(ctx.pack_stdout, "Restoring xmlsec1 from cache");
                    assert_contains!(ctx.pack_stdout, "Restoring libgwenhywfar-data from cache");
                    assert_contains!(ctx.pack_stdout, "Restoring libgwenhywfar79t64 from cache");
                }
                _ => panic_unsupported_test_configuration(),
            }
        });
    });
}

#[test]
#[ignore = "integration test"]
fn test_general_usage_env() {
    integration_test("fixtures/general_usage", |ctx| {
        let buildpack_layer_path = "/layers/heroku_debian-packages";
        let path = get_env_var(&ctx, "PATH");
        let ld_library_path = get_env_var(&ctx, "LD_LIBRARY_PATH");
        let library_path = get_env_var(&ctx, "LIBRARY_PATH");
        let include_path = get_env_var(&ctx, "INCLUDE_PATH");
        let cpath = get_env_var(&ctx, "CPATH");
        let cpp_path = get_env_var(&ctx, "CPPPATH");
        let pkg_config_path = get_env_var(&ctx, "PKG_CONFIG_PATH");

        assert_eq!(ld_library_path, library_path);
        assert_eq!(include_path, cpath);
        assert_eq!(include_path, cpp_path);

        match (get_integration_test_builder().as_str(), get_integration_test_arch().as_str()) {
            ("heroku/builder:22", "amd64") => {
                let packages = ["xmlsec1", "libgwenhywfar-data", "libgwenhywfar79"];
                for package in packages {
                    let layer_path = format!("{buildpack_layer_path}/{package}");
                    assert_contains!(path, &format!("{layer_path}/bin"));
                    assert_contains!(path, &format!("{layer_path}/usr/bin"));
                    assert_contains!(path, &format!("{layer_path}/usr/sbin"));
                    assert_contains!(ld_library_path, &format!("{layer_path}/usr/lib/x86_64-linux-gnu"));
                    assert_contains!(ld_library_path, &format!("{layer_path}/usr/lib"));
                    assert_contains!(ld_library_path, &format!("{layer_path}/lib/x86_64-linux-gnu"));
                    assert_contains!(ld_library_path, &format!("{layer_path}/lib"));
                    assert_contains!(include_path, &format!("{layer_path}/usr/include/x86_64-linux-gnu"));
                    assert_contains!(include_path, &format!("{layer_path}/usr/include"));
                    assert_contains!(pkg_config_path, &format!("{layer_path}/usr/lib/x86_64-linux-gnu/pkgconfig"));
                    assert_contains!(pkg_config_path, &format!("{layer_path}/usr/lib/pkgconfig"));
                }
            }
            ("heroku/builder:24", "amd64") => {
                let packages = ["xmlsec1", "libgwenhywfar-data", "libgwenhywfar79t64"];
                for package in packages {
                    let layer_path = format!("{buildpack_layer_path}/{package}");
                    assert_contains!(path, &format!("{layer_path}/bin"));
                    assert_contains!(path, &format!("{layer_path}/usr/bin"));
                    assert_contains!(path, &format!("{layer_path}/usr/sbin"));
                    assert_contains!(ld_library_path, &format!("{layer_path}/usr/lib/x86_64-linux-gnu"));
                    assert_contains!(ld_library_path, &format!("{layer_path}/usr/lib"));
                    assert_contains!(ld_library_path, &format!("{layer_path}/lib/x86_64-linux-gnu"));
                    assert_contains!(ld_library_path, &format!("{layer_path}/lib"));
                    assert_contains!(include_path, &format!("{layer_path}/usr/include/x86_64-linux-gnu"));
                    assert_contains!(include_path, &format!("{layer_path}/usr/include"));
                    assert_contains!(pkg_config_path, &format!("{layer_path}/usr/lib/x86_64-linux-gnu/pkgconfig"));
                    assert_contains!(pkg_config_path, &format!("{layer_path}/usr/lib/pkgconfig"));
                }
            }
            ("heroku/builder:24", "arm64") => {
                let packages = ["xmlsec1", "libgwenhywfar-data", "libgwenhywfar79t64"];
                for package in packages {
                    let layer_path = format!("{buildpack_layer_path}/{package}");
                    assert_contains!(path, &format!("{layer_path}/bin"));
                    assert_contains!(path, &format!("{layer_path}/usr/bin"));
                    assert_contains!(path, &format!("{layer_path}/usr/sbin"));
                    assert_contains!(ld_library_path, &format!("{layer_path}/usr/lib/aarch64-linux-gnu"));
                    assert_contains!(ld_library_path, &format!("{layer_path}/usr/lib"));
                    assert_contains!(ld_library_path, &format!("{layer_path}/lib/aarch64-linux-gnu"));
                    assert_contains!(ld_library_path, &format!("{layer_path}/lib"));
                    assert_contains!(include_path, &format!("{layer_path}/usr/include/aarch64-linux-gnu"));
                    assert_contains!(include_path, &format!("{layer_path}/usr/include"));
                    assert_contains!(pkg_config_path, &format!("{layer_path}/usr/lib/aarch64-linux-gnu/pkgconfig"));
                    assert_contains!(pkg_config_path, &format!("{layer_path}/usr/lib/pkgconfig"));
                }
            }
            _ => panic_unsupported_test_configuration(),
        }
    });
}

#[test]
#[ignore = "integration test"]
fn test_package_config_rewrite() {
    integration_test_with_config(
        "fixtures/project_file_with_empty_config",
        |config| {
            config.app_dir_preprocessor(|app_dir| {
                set_install_config(&app_dir, [requested_package_config("libopusfile-dev", true)]);
            });
        },
        |ctx| {
            match (get_integration_test_builder().as_str(), get_integration_test_arch().as_str()) {
                ("heroku/builder:22", "amd64") => {
                    assert_contains!(read_package_config(&ctx, "libopusfile-dev", "/usr/lib/pkgconfig/opusfile.pc"), "prefix=/layers/heroku_debian-packages/libopusfile-dev/usr");
                    assert_contains!(read_package_config(&ctx, "libopusfile-dev", "/usr/lib/pkgconfig/opusurl.pc"), "prefix=/layers/heroku_debian-packages/libopusfile-dev/usr");
                }
                ("heroku/builder:24", "amd64") => {
                    assert_contains!(read_package_config(&ctx, "libopusfile-dev", "/usr/lib/x86_64-linux-gnu/pkgconfig/opusfile.pc"), "prefix=/layers/heroku_debian-packages/libopusfile-dev/usr");
                    assert_contains!(read_package_config(&ctx, "libopusfile-dev", "/usr/lib/x86_64-linux-gnu/pkgconfig/opusurl.pc"), "prefix=/layers/heroku_debian-packages/libopusfile-dev/usr");
                }
                ("heroku/builder:24", "arm64") => {
                    assert_contains!(read_package_config(&ctx, "libopusfile-dev", "/usr/lib/aarch64-linux-gnu/pkgconfig/opusfile.pc"), "prefix=/layers/heroku_debian-packages/libopusfile-dev/usr");
                    assert_contains!(read_package_config(&ctx, "libopusfile-dev", "/usr/lib/aarch64-linux-gnu/pkgconfig/opusurl.pc"), "prefix=/layers/heroku_debian-packages/libopusfile-dev/usr");
                }
                _ => panic_unsupported_test_configuration(),
            };
        },
    );
}

#[test]
#[ignore = "integration test"]
#[allow(clippy::match_same_arms)]
fn test_cache_invalidated_when_configuration_changes() {
    integration_test_with_config(
        "fixtures/project_file_with_empty_config",
        |config| {
            config.app_dir_preprocessor(|app_dir| {
                set_install_config(&app_dir, [requested_package_config("libxmlsec1", true)]);
            });
        },
        |ctx| {
            match (get_integration_test_builder().as_str(), get_integration_test_arch().as_str()) {
                ("heroku/builder:22", "amd64") => {
                    assert_contains!(ctx.pack_stdout, "Adding libxmlsec1@1.2.33-1build2");
                    assert_contains!(ctx.pack_stdout, "Downloading libxmlsec1");
                    assert_contains!(ctx.pack_stdout, "Extracting libxmlsec1");
                    assert_contains!(ctx.pack_stdout, "Installing libxmlsec1 → /layers/heroku_debian-packages/libxmlsec1");
                }
                ("heroku/builder:24", "amd64") => {
                    assert_contains!(ctx.pack_stdout, "Adding libxmlsec1t64@1.2.39-5build2");
                    assert_contains!(ctx.pack_stdout, "Downloading libxmlsec1t64");
                    assert_contains!(ctx.pack_stdout, "Extracting libxmlsec1t64");
                    assert_contains!(ctx.pack_stdout, "Installing libxmlsec1t64 → /layers/heroku_debian-packages/libxmlsec1t64");
                }
                ("heroku/builder:24", "arm64") => {
                    assert_contains!(ctx.pack_stdout, "Adding libxmlsec1t64@1.2.39-5build2");
                    assert_contains!(ctx.pack_stdout, "Downloading libxmlsec1t64");
                    assert_contains!(ctx.pack_stdout, "Extracting libxmlsec1t64");
                    assert_contains!(ctx.pack_stdout, "Installing libxmlsec1t64 → /layers/heroku_debian-packages/libxmlsec1t64");
                }
                _ => panic_unsupported_test_configuration(),
            }

            let mut config = ctx.config.clone();
            ctx.rebuild(
                config.app_dir_preprocessor(|app_dir| {
                    set_install_config(&app_dir, [requested_package_config("libgwenhywfar-data", true)]);
                }),
                |ctx| match (get_integration_test_builder().as_str(), get_integration_test_arch().as_str()) {
                    ("heroku/builder:22", "amd64") => {
                        assert_contains!(ctx.pack_stdout, "Adding libgwenhywfar-data@5.9.0-1");
                        assert_contains!(ctx.pack_stdout, "Downloading libgwenhywfar-data");
                        assert_contains!(ctx.pack_stdout, "Extracting libgwenhywfar-data");
                        assert_contains!(ctx.pack_stdout, "Installing libgwenhywfar-data → /layers/heroku_debian-packages/libgwenhywfar-data");

                        assert_not_contains!(ctx.pack_stdout, "Adding libxmlsec1@1.2.33-1build2");
                        assert_not_contains!(ctx.pack_stdout, "Downloading libxmlsec1");
                        assert_not_contains!(ctx.pack_stdout, "Extracting libxmlsec1");
                        assert_not_contains!(ctx.pack_stdout, "Installing libxmlsec1 → /layers/heroku_debian-packages/libxmlsec1");
                    }
                    ("heroku/builder:24", "amd64") => {
                        assert_contains!(ctx.pack_stdout, "Adding libgwenhywfar-data@5.10.2-2.1build4");
                        assert_contains!(ctx.pack_stdout, "Downloading libgwenhywfar-data");
                        assert_contains!(ctx.pack_stdout, "Extracting libgwenhywfar-data");
                        assert_contains!(ctx.pack_stdout, "Installing libgwenhywfar-data → /layers/heroku_debian-packages/libgwenhywfar-data");

                        assert_not_contains!(ctx.pack_stdout, "Adding libxmlsec1t64@1.2.39-5build2");
                        assert_not_contains!(ctx.pack_stdout, "Downloading libxmlsec1t64");
                        assert_not_contains!(ctx.pack_stdout, "Extracting libxmlsec1t64");
                        assert_not_contains!(ctx.pack_stdout, "Installing libxmlsec1t64 → /layers/heroku_debian-packages/libxmlsec1t64");
                    }
                    ("heroku/builder:24", "arm64") => {
                        assert_contains!(ctx.pack_stdout, "Adding libgwenhywfar-data@5.10.2-2.1build4");
                        assert_contains!(ctx.pack_stdout, "Downloading libgwenhywfar-data");
                        assert_contains!(ctx.pack_stdout, "Extracting libgwenhywfar-data");
                        assert_contains!(ctx.pack_stdout, "Installing libgwenhywfar-data → /layers/heroku_debian-packages/libgwenhywfar-data");

                        assert_not_contains!(ctx.pack_stdout, "Adding libxmlsec1t64@1.2.39-5build2");
                        assert_not_contains!(ctx.pack_stdout, "Downloading libxmlsec1t64");
                        assert_not_contains!(ctx.pack_stdout, "Extracting libxmlsec1t64");
                        assert_not_contains!(ctx.pack_stdout, "Installing libxmlsec1t64 → /layers/heroku_debian-packages/libxmlsec1t64");
                    }
                    _ => panic_unsupported_test_configuration(),
                },
            );
        },
    );
}

const DEFAULT_BUILDER: &str = "heroku/builder:22";

fn get_integration_test_builder() -> String {
    std::env::var("INTEGRATION_TEST_CNB_BUILDER").unwrap_or(DEFAULT_BUILDER.to_string())
}

const DEFAULT_ARCH: &str = "amd64";

fn get_integration_test_arch() -> String {
    std::env::var("INTEGRATION_TEST_CNB_ARCH").unwrap_or(DEFAULT_ARCH.to_string())
}

fn panic_unsupported_test_configuration() -> ! {
    panic!("Unsupported test configuration:\nINTEGRATION_TEST_CNB_BUILDER={}\nINTEGRATION_TEST_CNB_ARCH={}", get_integration_test_builder(), get_integration_test_arch());
}

fn integration_test(fixture: &str, test_body: fn(TestContext)) {
    integration_test_with_config(fixture, |_| {}, test_body);
}

fn integration_test_with_config(fixture: &str, with_config: fn(&mut BuildConfig), test_body: fn(TestContext)) {
    let builder = get_integration_test_builder();
    let app_dir = PathBuf::from("tests").join(fixture);

    let target_triple = match get_integration_test_arch().as_str() {
        "amd64" => "x86_64-unknown-linux-musl",
        "arm64" => "aarch64-unknown-linux-musl",
        _ => panic_unsupported_test_configuration(),
    };

    let mut build_config = BuildConfig::new(builder, app_dir);
    build_config.target_triple(target_triple);
    with_config(&mut build_config);

    TestRunner::default().build(build_config, test_body);
}

fn get_env_var(ctx: &TestContext, env_var_name: &str) -> String {
    ctx.run_shell_command(format!("echo -n ${env_var_name}")).stdout
}

fn read_package_config(ctx: &TestContext, package: &str, package_config_path: &str) -> String {
    ctx.run_shell_command(format!("cat /layers/heroku_debian-packages/{package}{package_config_path}")).stdout
}

fn set_install_config<I>(app_dir: &Path, requested_packages: I)
where
    I: IntoIterator<Item = InlineTable>,
{
    update_project_toml(app_dir, |doc| {
        let root_config = doc
            .get_mut("com")
            .and_then(|item| item.as_table_like_mut())
            .and_then(|com| com.get_mut("heroku"))
            .and_then(|item| item.as_table_like_mut())
            .and_then(|heroku| heroku.get_mut("buildpacks"))
            .and_then(|item| item.as_table_like_mut())
            .and_then(|buildpacks| buildpacks.get_mut("debian-packages"))
            .and_then(|item| item.as_table_like_mut())
            .unwrap();
        root_config.insert("install", value(Array::from_iter(requested_packages)));
    });
}

fn requested_package_config(package: &str, skip_dependencies: bool) -> InlineTable {
    let mut requested_package = InlineTable::new();
    requested_package.insert("name", value(package).into_value().unwrap());
    requested_package.insert("skip_dependencies", value(skip_dependencies).into_value().unwrap());
    requested_package
}

fn update_project_toml(app_dir: &Path, update_fn: impl FnOnce(&mut DocumentMut)) {
    let project_toml = app_dir.join("project.toml");
    let contents = std::fs::read_to_string(&project_toml).unwrap();
    let mut doc = toml_edit::DocumentMut::from_str(&contents).unwrap();
    update_fn(&mut doc);
    std::fs::write(&project_toml, doc.to_string()).unwrap();
}
