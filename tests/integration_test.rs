//! All integration tests are skipped by default (using the `ignore` attribute)
//! since performing builds is slow. To run them use: `cargo test -- --ignored`.

// Required due to: https://github.com/rust-lang/rust/issues/95513
#![allow(unused_crate_dependencies)]
#![allow(clippy::unwrap_used)]

use indoc::indoc;
use libcnb_test::{assert_contains, assert_contains_match, assert_not_contains, BuildConfig, BuildpackReference, PackResult, TestContext, TestRunner};
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use toml_edit::{value, Array, DocumentMut, InlineTable};

#[test]
#[ignore = "integration test"]
fn test_successful_detection() {
    integration_test_with_config(
        "fixtures/project_file_with_empty_config",
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
            let stdout_stderr = format!("{}\n{}", ctx.pack_stdout, ctx.pack_stderr);
            assert_contains!(stdout_stderr, "No project.toml or Aptfile found.");
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
            let stdout_stderr = format!("{}\n{}", ctx.pack_stdout, ctx.pack_stderr);
            assert_contains!(stdout_stderr, "project.toml found, but no [com.heroku.buildpacks.deb-packages] configuration present.");
        },
    );
}

#[test]
#[ignore = "integration test"]
fn test_passes_detection_when_project_file_with_empty_config_exists_and_prints_help_during_build() {
    integration_test("fixtures/project_file_with_empty_config", |ctx| {
        assert_contains!(ctx.pack_stderr, "No configured packages to install found in project.toml file.");
    });
}

#[test]
#[ignore = "integration test"]
fn test_aptfile_passes_detection_and_prints_help_during_build() {
    integration_test_with_config(
        "fixtures/no_project_file",
        |config| {
            config.expected_pack_result(PackResult::Success);
            config.app_dir_preprocessor(|dir| {
                std::fs::write(dir.join("Aptfile"), "").unwrap();
            });
        },
        |ctx| {
            assert_contains!(ctx.pack_stderr, "The use of an `Aptfile` is deprecated!");
        },
    );
}

#[test]
#[ignore = "integration test"]
#[allow(clippy::too_many_lines)]
fn test_general_usage_output() {
    integration_test("fixtures/general_usage", |ctx| {
        assert_contains_match!(ctx.pack_stderr, r"# Heroku .deb Packages \(v\d+\.\d+\.\d+\)");

        match (get_integration_test_builder().as_str(), get_integration_test_arch().as_str()) {
            ("heroku/builder:22", "amd64") => {
                assert_contains!(ctx.pack_stderr, "Distribution Info");
                assert_contains!(ctx.pack_stderr, "Name: ubuntu");
                assert_contains!(ctx.pack_stderr, "Version: 22.04");
                assert_contains!(ctx.pack_stderr, "Codename: jammy");
                assert_contains!(ctx.pack_stderr, "Architecture: amd64");

                assert_contains!(ctx.pack_stderr, "## Creating package index");
                assert_contains!(ctx.pack_stderr, "Package sources");
                assert_contains!(ctx.pack_stderr, "http://archive.ubuntu.com/ubuntu jammy [main, universe]");
                assert_contains!(ctx.pack_stderr, "http://archive.ubuntu.com/ubuntu jammy-security [main, universe]");
                assert_contains!(ctx.pack_stderr, "http://archive.ubuntu.com/ubuntu jammy-updates [main, universe]");
                assert_contains!(ctx.pack_stderr, "Updating");
                assert_contains_match!(ctx.pack_stderr, r"Downloaded release file http://archive.ubuntu.com/ubuntu/dists/jammy/InRelease");
                assert_contains_match!(ctx.pack_stderr, r"Downloaded package index http://archive.ubuntu.com/ubuntu/dists/jammy/main/binary-amd64/by-hash/SHA256/[0-9a-f]+$");
                assert_contains_match!(ctx.pack_stderr, r"Downloaded package index http://archive.ubuntu.com/ubuntu/dists/jammy/universe/binary-amd64/by-hash/SHA256/[0-9a-f]+$");
                assert_contains_match!(ctx.pack_stderr, r"Downloaded release file http://archive.ubuntu.com/ubuntu/dists/jammy-security/InRelease");
                assert_contains_match!(ctx.pack_stderr, r"Downloaded package index http://archive.ubuntu.com/ubuntu/dists/jammy-security/main/binary-amd64/by-hash/SHA256/[0-9a-f]+$");
                assert_contains_match!(ctx.pack_stderr, r"Downloaded package index http://archive.ubuntu.com/ubuntu/dists/jammy-security/universe/binary-amd64/by-hash/SHA256/[0-9a-f]+$");
                assert_contains_match!(ctx.pack_stderr, r"Downloaded release file http://archive.ubuntu.com/ubuntu/dists/jammy-updates/InRelease");
                assert_contains_match!(ctx.pack_stderr, r"Downloaded package index http://archive.ubuntu.com/ubuntu/dists/jammy-updates/main/binary-amd64/by-hash/SHA256/[0-9a-f]+$");
                assert_contains_match!(ctx.pack_stderr, r"Downloaded package index http://archive.ubuntu.com/ubuntu/dists/jammy-updates/universe/binary-amd64/by-hash/SHA256/[0-9a-f]+$");
                assert_contains!(ctx.pack_stderr, "Building package index");
                assert_contains!(ctx.pack_stderr, "Processing package files");
                assert_contains_match!(ctx.pack_stderr, r"Indexed \d+ packages");

                assert_contains!(ctx.pack_stderr, "## Determining packages to install");
                assert_contains!(ctx.pack_stderr, "Collecting system install information");
                assert_contains!(ctx.pack_stderr, "Determining install requirements for requested package `libgwenhywfar79`");
                assert_contains!(ctx.pack_stderr, "Adding `libgwenhywfar79@5.9.0-1`");
                assert_contains!(ctx.pack_stderr, "Adding `libgwenhywfar-data@5.9.0-1` [from libgwenhywfar79]");
                assert_contains!(ctx.pack_stderr, "Determining install requirements for requested package `libgwenhywfar-data`");
                assert_contains!(ctx.pack_stderr, "Skipping `libgwenhywfar-data` because `libgwenhywfar-data@5.9.0-1` was already installed as a dependency of `libgwenhywfar79`");
                assert_contains!(ctx.pack_stderr, "Determining install requirements for requested package `xmlsec1`");
                assert_contains!(ctx.pack_stderr, "Adding `xmlsec1@1.2.33-1build2`");
                assert_contains!(ctx.pack_stderr, "Determining install requirements for requested package `wget`");
                assert_contains_match!(ctx.pack_stderr, "Skipping `wget` because `wget@1.21.2-.*` is already installed on the system");
                assert_contains!(ctx.pack_stderr, "Determining install requirements for requested package `libvips`");
                assert_contains!(ctx.pack_stderr, "Virtual package `libvips` is provided by `libvips42@8.12.1-1build1`");
                assert_contains!(ctx.pack_stderr, "Skipping `libvips42` because `libvips42@8.12.1-1build1` is already installed on the system");
                assert_contains!(ctx.pack_stderr, "Determining install requirements for requested package `curl`");
                assert_contains_match!(ctx.pack_stderr, "Adding `curl@7.81.0-.*` \\(forced\\)");

                assert_contains!(ctx.pack_stderr, "## Installing packages");
                assert_contains!(ctx.pack_stderr, "Requesting packages");
                assert_contains!(ctx.pack_stderr, "`libgwenhywfar79@5.9.0-1` from http://archive.ubuntu.com/ubuntu/pool/universe/libg/libgwenhywfar/libgwenhywfar79_5.9.0-1_amd64.deb");
                assert_contains!(ctx.pack_stderr, "`libgwenhywfar-data@5.9.0-1` from http://archive.ubuntu.com/ubuntu/pool/universe/libg/libgwenhywfar/libgwenhywfar-data_5.9.0-1_all.deb");
                assert_contains!(ctx.pack_stderr, "`xmlsec1@1.2.33-1build2` from http://archive.ubuntu.com/ubuntu/pool/main/x/xmlsec1/xmlsec1_1.2.33-1build2_amd64.deb");
                assert_contains_match!(ctx.pack_stderr, "`curl@7.81.0-.*` from http://archive.ubuntu.com/ubuntu/pool/main/c/curl/curl_7.81.0-.*_amd64.deb");
                assert_contains!(ctx.pack_stderr, "Downloading");
                assert_contains!(ctx.pack_stderr, "Installation complete");

                assert_not_contains!(ctx.pack_stderr, "Layer file listing");
                assert_not_contains!(ctx.pack_stderr, "/layers/heroku_deb-packages/packages/usr/bin/xmlsec1");
                assert_not_contains!(ctx.pack_stderr, "/layers/heroku_deb-packages/packages/usr/lib/x86_64-linux-gnu/libgwenhywfar.so");
            }
            ("heroku/builder:24", "amd64") => {
                assert_contains!(ctx.pack_stderr, "Distribution Info");
                assert_contains!(ctx.pack_stderr, "Name: ubuntu");
                assert_contains!(ctx.pack_stderr, "Version: 24.04");
                assert_contains!(ctx.pack_stderr, "Codename: noble");
                assert_contains!(ctx.pack_stderr, "Architecture: amd64");

                assert_contains!(ctx.pack_stderr, "## Creating package index");
                assert_contains!(ctx.pack_stderr, "Package sources");
                assert_contains!(ctx.pack_stderr, "http://archive.ubuntu.com/ubuntu noble [main, universe]");
                assert_contains!(ctx.pack_stderr, "http://security.ubuntu.com/ubuntu noble-security [main, universe]");
                assert_contains!(ctx.pack_stderr, "http://archive.ubuntu.com/ubuntu noble-updates [main, universe]");
                assert_contains!(ctx.pack_stderr, "Updating");
                assert_contains_match!(ctx.pack_stderr, r"Downloaded release file http://archive.ubuntu.com/ubuntu/dists/noble/InRelease");
                assert_contains_match!(ctx.pack_stderr, r"Downloaded package index http://archive.ubuntu.com/ubuntu/dists/noble/main/binary-amd64/by-hash/SHA256/[0-9a-f]+$");
                assert_contains_match!(ctx.pack_stderr, r"Downloaded package index http://archive.ubuntu.com/ubuntu/dists/noble/universe/binary-amd64/by-hash/SHA256/[0-9a-f]+$");
                assert_contains_match!(ctx.pack_stderr, r"Downloaded release file http://security.ubuntu.com/ubuntu/dists/noble-security/InRelease");
                assert_contains_match!(ctx.pack_stderr, r"Downloaded package index http://security.ubuntu.com/ubuntu/dists/noble-security/main/binary-amd64/by-hash/SHA256/[0-9a-f]+$");
                assert_contains_match!(ctx.pack_stderr, r"Downloaded package index http://security.ubuntu.com/ubuntu/dists/noble-security/universe/binary-amd64/by-hash/SHA256/[0-9a-f]+$");
                assert_contains_match!(ctx.pack_stderr, r"Downloaded release file http://archive.ubuntu.com/ubuntu/dists/noble-updates/InRelease");
                assert_contains_match!(ctx.pack_stderr, r"Downloaded package index http://archive.ubuntu.com/ubuntu/dists/noble-updates/main/binary-amd64/by-hash/SHA256/[0-9a-f]+$");
                assert_contains_match!(ctx.pack_stderr, r"Downloaded package index http://archive.ubuntu.com/ubuntu/dists/noble-updates/universe/binary-amd64/by-hash/SHA256/[0-9a-f]+$");
                assert_contains!(ctx.pack_stderr, "Building package index");
                assert_contains!(ctx.pack_stderr, "Processing package files");
                assert_contains_match!(ctx.pack_stderr, r"Indexed \d+ packages");

                assert_contains!(ctx.pack_stderr, "## Determining packages to install");
                assert_contains!(ctx.pack_stderr, "Collecting system install information");
                assert_contains!(ctx.pack_stderr, "Determining install requirements for requested package `libgwenhywfar79`");
                assert_contains!(ctx.pack_stderr, "Virtual package `libgwenhywfar79` is provided by `libgwenhywfar79t64@5.10.2-2.1build4`");
                assert_contains!(ctx.pack_stderr, "Adding `libgwenhywfar79t64@5.10.2-2.1build4` [from libgwenhywfar79]");
                assert_contains!(ctx.pack_stderr, "Adding `libgwenhywfar-data@5.10.2-2.1build4` [from libgwenhywfar79t64 ← libgwenhywfar79]");
                assert_contains!(ctx.pack_stderr, "Determining install requirements for requested package `libgwenhywfar-data`");
                assert_contains!(ctx.pack_stderr, "Skipping `libgwenhywfar-data` because `libgwenhywfar-data@5.10.2-2.1build4` was already installed as a dependency of `libgwenhywfar79`");
                assert_contains!(ctx.pack_stderr, "Determining install requirements for requested package `xmlsec1`");
                assert_contains!(ctx.pack_stderr, "Adding `xmlsec1@1.2.39-5build2`");
                assert_contains!(ctx.pack_stderr, "Determining install requirements for requested package `wget`");
                assert_contains_match!(ctx.pack_stderr, "Skipping `wget` because `wget@1.21.4-.*` is already installed on the system");
                assert_contains!(ctx.pack_stderr, "Determining install requirements for requested package `libvips`");
                assert_contains!(ctx.pack_stderr, "Virtual package `libvips` is provided by `libvips42t64@8.15.1-1.1build4`");
                assert_contains!(ctx.pack_stderr, "Skipping `libvips42t64` because `libvips42t64@8.15.1-1.1build4` is already installed on the system");
                assert_contains!(ctx.pack_stderr, "Determining install requirements for requested package `curl`");
                assert_contains_match!(ctx.pack_stderr, "Adding `curl@8.5.0-.*` \\(forced\\)");

                assert_contains!(ctx.pack_stderr, "## Installing packages");
                assert_contains!(ctx.pack_stderr, "Requesting packages");
                assert_contains!(ctx.pack_stderr, "`libgwenhywfar79t64@5.10.2-2.1build4` from http://archive.ubuntu.com/ubuntu/pool/universe/libg/libgwenhywfar/libgwenhywfar79t64_5.10.2-2.1build4_amd64.deb");
                assert_contains!(ctx.pack_stderr, "`libgwenhywfar-data@5.10.2-2.1build4` from http://archive.ubuntu.com/ubuntu/pool/universe/libg/libgwenhywfar/libgwenhywfar-data_5.10.2-2.1build4_all.deb");
                assert_contains!(ctx.pack_stderr, "`xmlsec1@1.2.39-5build2` from http://archive.ubuntu.com/ubuntu/pool/main/x/xmlsec1/xmlsec1_1.2.39-5build2_amd64.deb");
                assert_contains_match!(ctx.pack_stderr, "`curl@8.5.0-.*` from http://(security|archive).ubuntu.com/ubuntu/pool/main/c/curl/curl_8.5.0-.*_amd64.deb");
                assert_contains!(ctx.pack_stderr, "Downloading");
                assert_contains!(ctx.pack_stderr, "Installation complete");

                assert_not_contains!(ctx.pack_stderr, "Layer file listing");
                assert_not_contains!(ctx.pack_stderr, "/layers/heroku_deb-packages/packages/usr/bin/xmlsec1");
                assert_not_contains!(ctx.pack_stderr, "/layers/heroku_deb-packages/packages/usr/lib/x86_64-linux-gnu/libgwenhywfar.so");
            }
            ("heroku/builder:24", "arm64") => {
                assert_contains!(ctx.pack_stderr, "Distribution Info");
                assert_contains!(ctx.pack_stderr, "Name: ubuntu");
                assert_contains!(ctx.pack_stderr, "Version: 24.04");
                assert_contains!(ctx.pack_stderr, "Codename: noble");
                assert_contains!(ctx.pack_stderr, "Architecture: arm64");

                assert_contains!(ctx.pack_stderr, "## Creating package index");
                assert_contains!(ctx.pack_stderr, "Package sources");
                assert_contains!(ctx.pack_stderr, "http://ports.ubuntu.com/ubuntu-ports noble [main, universe]");
                assert_contains!(ctx.pack_stderr, "http://ports.ubuntu.com/ubuntu-ports noble-updates [main, universe]");
                assert_contains!(ctx.pack_stderr, "http://ports.ubuntu.com/ubuntu-ports noble-security [main, universe]");
                assert_contains!(ctx.pack_stderr, "Updating");
                assert_contains_match!(ctx.pack_stderr, r"Downloaded release file http://ports.ubuntu.com/ubuntu-ports/dists/noble/InRelease");
                assert_contains_match!(ctx.pack_stderr, r"Downloaded package index http://ports.ubuntu.com/ubuntu-ports/dists/noble/main/binary-arm64/by-hash/SHA256/[0-9a-f]+$");
                assert_contains_match!(ctx.pack_stderr, r"Downloaded package index http://ports.ubuntu.com/ubuntu-ports/dists/noble/universe/binary-arm64/by-hash/SHA256/[0-9a-f]+$");
                assert_contains_match!(ctx.pack_stderr, r"Downloaded release file http://ports.ubuntu.com/ubuntu-ports/dists/noble-security/InRelease");
                assert_contains_match!(ctx.pack_stderr, r"Downloaded package index http://ports.ubuntu.com/ubuntu-ports/dists/noble-security/main/binary-arm64/by-hash/SHA256/[0-9a-f]+$");
                assert_contains_match!(ctx.pack_stderr, r"Downloaded package index http://ports.ubuntu.com/ubuntu-ports/dists/noble-security/universe/binary-arm64/by-hash/SHA256/[0-9a-f]+$");
                assert_contains_match!(ctx.pack_stderr, r"Downloaded release file http://ports.ubuntu.com/ubuntu-ports/dists/noble-updates/InRelease");
                assert_contains_match!(ctx.pack_stderr, r"Downloaded package index http://ports.ubuntu.com/ubuntu-ports/dists/noble-updates/main/binary-arm64/by-hash/SHA256/[0-9a-f]+$");
                assert_contains_match!(ctx.pack_stderr, r"Downloaded package index http://ports.ubuntu.com/ubuntu-ports/dists/noble-updates/universe/binary-arm64/by-hash/SHA256/[0-9a-f]+$");
                assert_contains!(ctx.pack_stderr, "Building package index");
                assert_contains!(ctx.pack_stderr, "Processing package files");
                assert_contains_match!(ctx.pack_stderr, r"Indexed \d+ packages");

                assert_contains!(ctx.pack_stderr, "## Determining packages to install");
                assert_contains!(ctx.pack_stderr, "Collecting system install information");
                assert_contains!(ctx.pack_stderr, "Determining install requirements for requested package `libgwenhywfar79`");
                assert_contains!(ctx.pack_stderr, "Virtual package `libgwenhywfar79` is provided by `libgwenhywfar79t64@5.10.2-2.1build4`");
                assert_contains!(ctx.pack_stderr, "Adding `libgwenhywfar79t64@5.10.2-2.1build4` [from libgwenhywfar79]");
                assert_contains!(ctx.pack_stderr, "Adding `libgwenhywfar-data@5.10.2-2.1build4` [from libgwenhywfar79t64 ← libgwenhywfar79]");
                assert_contains!(ctx.pack_stderr, "Determining install requirements for requested package `libgwenhywfar-data`");
                assert_contains!(ctx.pack_stderr, "Skipping `libgwenhywfar-data` because `libgwenhywfar-data@5.10.2-2.1build4` was already installed as a dependency of `libgwenhywfar79`");
                assert_contains!(ctx.pack_stderr, "Determining install requirements for requested package `xmlsec1`");
                assert_contains!(ctx.pack_stderr, "Adding `xmlsec1@1.2.39-5build2`");
                assert_contains!(ctx.pack_stderr, "Determining install requirements for requested package `wget`");
                assert_contains_match!(ctx.pack_stderr, "Skipping `wget` because `wget@1.21.4-.*` is already installed on the system");
                assert_contains!(ctx.pack_stderr, "Determining install requirements for requested package `libvips`");
                assert_contains!(ctx.pack_stderr, "Virtual package `libvips` is provided by `libvips42t64@8.15.1-1.1build4`");
                assert_contains!(ctx.pack_stderr, "Skipping `libvips42t64` because `libvips42t64@8.15.1-1.1build4` is already installed on the system");
                assert_contains!(ctx.pack_stderr, "Determining install requirements for requested package `curl`");
                assert_contains_match!(ctx.pack_stderr, "Adding `curl@8.5.0-.*` \\(forced\\)");

                assert_contains!(ctx.pack_stderr, "## Installing packages");
                assert_contains!(ctx.pack_stderr, "Requesting packages");
                assert_contains!(ctx.pack_stderr, "`libgwenhywfar79t64@5.10.2-2.1build4` from http://ports.ubuntu.com/ubuntu-ports/pool/universe/libg/libgwenhywfar/libgwenhywfar79t64_5.10.2-2.1build4_arm64.deb");
                assert_contains!(ctx.pack_stderr, "`libgwenhywfar-data@5.10.2-2.1build4` from http://ports.ubuntu.com/ubuntu-ports/pool/universe/libg/libgwenhywfar/libgwenhywfar-data_5.10.2-2.1build4_all.deb");
                assert_contains!(ctx.pack_stderr, "`xmlsec1@1.2.39-5build2` from http://ports.ubuntu.com/ubuntu-ports/pool/main/x/xmlsec1/xmlsec1_1.2.39-5build2_arm64.deb");
                assert_contains_match!(ctx.pack_stderr, "`curl@8.5.0-.*` from http://ports.ubuntu.com/ubuntu-ports/pool/main/c/curl/curl_8.5.0-.*_arm64.deb");
                assert_contains!(ctx.pack_stderr, "Downloading");
                assert_contains!(ctx.pack_stderr, "Installation complete");

                assert_not_contains!(ctx.pack_stderr, "Layer file listing");
                assert_not_contains!(ctx.pack_stderr, "/layers/heroku_deb-packages/packages/usr/bin/xmlsec1");
                assert_not_contains!(ctx.pack_stderr, "/layers/heroku_deb-packages/packages/usr/lib/aarch64-linux-gnu/libgwenhywfar.so");
            }
            _ => panic_unsupported_test_configuration(),
        }
    });
}

#[test]
#[ignore = "integration test"]
#[allow(clippy::too_many_lines)]
fn test_general_usage_output_when_buildpack_log_level_is_debug() {
    integration_test_with_config(
        "fixtures/general_usage",
        |config| {
            config.env("BP_LOG_LEVEL", "DEBUG");
        },
        |ctx| {
            let multiarch_name = match get_integration_test_arch().as_str() {
                "amd64" => "x86_64-linux-gnu",
                "arm64" => "aarch64-linux-gnu",
                _ => panic_unsupported_test_configuration(),
            };
            assert_contains!(ctx.pack_stderr, "Layer file listing");
            assert_contains!(ctx.pack_stderr, "/layers/heroku_deb-packages/packages/usr/bin/xmlsec1");
            assert_contains!(ctx.pack_stderr, &format!("/layers/heroku_deb-packages/packages/usr/lib/{multiarch_name}/libgwenhywfar.so"));
        },
    );
}

#[test]
#[ignore = "integration test"]
fn test_general_usage_output_on_rebuild() {
    integration_test("fixtures/general_usage", |ctx| {
        let config = ctx.config.clone();
        ctx.rebuild(config, |ctx| {
            assert_contains_match!(ctx.pack_stderr, r"# Heroku .deb Packages \(v\d+\.\d+\.\d+\)");

            match (get_integration_test_builder().as_str(), get_integration_test_arch().as_str()) {
                ("heroku/builder:22", "amd64") => {
                    assert_contains_match!(ctx.pack_stderr, r"Restored release file from cache \(http://archive.ubuntu.com/ubuntu/dists/jammy/InRelease\)");
                    assert_contains_match!(ctx.pack_stderr, r"Restored package index from cache \(http://archive.ubuntu.com/ubuntu/dists/jammy/main/binary-amd64/by-hash/SHA256/[0-9a-f]+\)");
                    assert_contains_match!(ctx.pack_stderr, r"Restored package index from cache \(http://archive.ubuntu.com/ubuntu/dists/jammy/universe/binary-amd64/by-hash/SHA256/[0-9a-f]+\)");
                    assert_contains_match!(ctx.pack_stderr, r"Restored release file from cache \(http://archive.ubuntu.com/ubuntu/dists/jammy-security/InRelease\)");
                    assert_contains_match!(ctx.pack_stderr, r"Restored package index from cache \(http://archive.ubuntu.com/ubuntu/dists/jammy-security/main/binary-amd64/by-hash/SHA256/[0-9a-f]+\)");
                    assert_contains_match!(ctx.pack_stderr, r"Restored package index from cache \(http://archive.ubuntu.com/ubuntu/dists/jammy-security/universe/binary-amd64/by-hash/SHA256/[0-9a-f]+\)");
                    assert_contains_match!(ctx.pack_stderr, r"Restored release file from cache \(http://archive.ubuntu.com/ubuntu/dists/jammy-updates/InRelease\)");
                    assert_contains_match!(ctx.pack_stderr, r"Restored package index from cache \(http://archive.ubuntu.com/ubuntu/dists/jammy-updates/main/binary-amd64/by-hash/SHA256/[0-9a-f]+\)");
                    assert_contains_match!(ctx.pack_stderr, r"Restored package index from cache \(http://archive.ubuntu.com/ubuntu/dists/jammy-updates/universe/binary-amd64/by-hash/SHA256/[0-9a-f]+\)");

                    assert_contains!(ctx.pack_stderr, "Restoring packages from cache");
                    assert_contains!(ctx.pack_stderr, "`libgwenhywfar79@5.9.0-1`");
                    assert_contains!(ctx.pack_stderr, "`libgwenhywfar-data@5.9.0-1`");
                    assert_contains!(ctx.pack_stderr, "`xmlsec1@1.2.33-1build2`");
                    assert_contains_match!(ctx.pack_stderr, "`curl@7.81.0-.*`");
                }
                ("heroku/builder:24", "amd64") => {
                    assert_contains_match!(ctx.pack_stderr, r"Restored release file from cache \(http://archive.ubuntu.com/ubuntu/dists/noble/InRelease\)");
                    assert_contains_match!(ctx.pack_stderr, r"Restored package index from cache \(http://archive.ubuntu.com/ubuntu/dists/noble/main/binary-amd64/by-hash/SHA256/[0-9a-f]+\)");
                    assert_contains_match!(ctx.pack_stderr, r"Restored package index from cache \(http://archive.ubuntu.com/ubuntu/dists/noble/universe/binary-amd64/by-hash/SHA256/[0-9a-f]+\)");
                    assert_contains_match!(ctx.pack_stderr, r"Restored release file from cache \(http://security.ubuntu.com/ubuntu/dists/noble-security/InRelease\)");
                    assert_contains_match!(ctx.pack_stderr, r"Restored package index from cache \(http://security.ubuntu.com/ubuntu/dists/noble-security/main/binary-amd64/by-hash/SHA256/[0-9a-f]+\)");
                    assert_contains_match!(ctx.pack_stderr, r"Restored package index from cache \(http://security.ubuntu.com/ubuntu/dists/noble-security/universe/binary-amd64/by-hash/SHA256/[0-9a-f]+\)");
                    assert_contains_match!(ctx.pack_stderr, r"Restored release file from cache \(http://archive.ubuntu.com/ubuntu/dists/noble-updates/InRelease\)");
                    assert_contains_match!(ctx.pack_stderr, r"Restored package index from cache \(http://archive.ubuntu.com/ubuntu/dists/noble-updates/main/binary-amd64/by-hash/SHA256/[0-9a-f]+\)");
                    assert_contains_match!(ctx.pack_stderr, r"Restored package index from cache \(http://archive.ubuntu.com/ubuntu/dists/noble-updates/universe/binary-amd64/by-hash/SHA256/[0-9a-f]+\)");

                    assert_contains!(ctx.pack_stderr, "Restoring packages from cache");
                    assert_contains!(ctx.pack_stderr, "`libgwenhywfar79t64@5.10.2-2.1build4`");
                    assert_contains!(ctx.pack_stderr, "`libgwenhywfar-data@5.10.2-2.1build4`");
                    assert_contains!(ctx.pack_stderr, "`xmlsec1@1.2.39-5build2`");
                    assert_contains_match!(ctx.pack_stderr, "`curl@8.5.0-.*`");
                }
                ("heroku/builder:24", "arm64") => {
                    assert_contains_match!(ctx.pack_stderr, r"Restored release file from cache \(http://ports.ubuntu.com/ubuntu-ports/dists/noble/InRelease\)");
                    assert_contains_match!(ctx.pack_stderr, r"Restored package index from cache \(http://ports.ubuntu.com/ubuntu-ports/dists/noble/main/binary-arm64/by-hash/SHA256/[0-9a-f]+\)");
                    assert_contains_match!(ctx.pack_stderr, r"Restored package index from cache \(http://ports.ubuntu.com/ubuntu-ports/dists/noble/universe/binary-arm64/by-hash/SHA256/[0-9a-f]+\)");
                    assert_contains_match!(ctx.pack_stderr, r"Restored release file from cache \(http://ports.ubuntu.com/ubuntu-ports/dists/noble-security/InRelease\)");
                    assert_contains_match!(ctx.pack_stderr, r"Restored package index from cache \(http://ports.ubuntu.com/ubuntu-ports/dists/noble-security/main/binary-arm64/by-hash/SHA256/[0-9a-f]+\)");
                    assert_contains_match!(ctx.pack_stderr, r"Restored package index from cache \(http://ports.ubuntu.com/ubuntu-ports/dists/noble-security/universe/binary-arm64/by-hash/SHA256/[0-9a-f]+\)");
                    assert_contains_match!(ctx.pack_stderr, r"Restored release file from cache \(http://ports.ubuntu.com/ubuntu-ports/dists/noble-updates/InRelease\)");
                    assert_contains_match!(ctx.pack_stderr, r"Restored package index from cache \(http://ports.ubuntu.com/ubuntu-ports/dists/noble-updates/main/binary-arm64/by-hash/SHA256/[0-9a-f]+\)");
                    assert_contains_match!(ctx.pack_stderr, r"Restored package index from cache \(http://ports.ubuntu.com/ubuntu-ports/dists/noble-updates/universe/binary-arm64/by-hash/SHA256/[0-9a-f]+\)");

                    assert_contains!(ctx.pack_stderr, "Restoring packages from cache");
                    assert_contains!(ctx.pack_stderr, "`libgwenhywfar79t64@5.10.2-2.1build4`");
                    assert_contains!(ctx.pack_stderr, "`libgwenhywfar-data@5.10.2-2.1build4`");
                    assert_contains!(ctx.pack_stderr, "`xmlsec1@1.2.39-5build2`");
                    assert_contains_match!(ctx.pack_stderr, "`curl@8.5.0-.*`");
                }
                _ => panic_unsupported_test_configuration(),
            }
        });
    });
}

#[test]
#[ignore = "integration test"]
#[allow(clippy::too_many_lines)]
fn test_general_usage_env() {
    integration_test("fixtures/general_usage", |ctx| {
        let layer_path = "/layers/heroku_deb-packages/packages";

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
            (_, "amd64") => {
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
            (_, "arm64") => {
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
        |ctx| match (get_integration_test_builder().as_str(), get_integration_test_arch().as_str()) {
            ("heroku/builder:22", "amd64") => {
                assert_contains!(read_package_config(&ctx, "usr/lib/pkgconfig/opusfile.pc"), "prefix=/layers/heroku_deb-packages/packages/usr");
                assert_contains!(read_package_config(&ctx, "usr/lib/pkgconfig/opusurl.pc"), "prefix=/layers/heroku_deb-packages/packages/usr");
            }
            ("heroku/builder:24", "amd64") => {
                assert_contains!(read_package_config(&ctx, "usr/lib/x86_64-linux-gnu/pkgconfig/opusfile.pc"), "prefix=/layers/heroku_deb-packages/packages/usr");
                assert_contains!(read_package_config(&ctx, "usr/lib/x86_64-linux-gnu/pkgconfig/opusurl.pc"), "prefix=/layers/heroku_deb-packages/packages/usr");
            }
            ("heroku/builder:24", "arm64") => {
                assert_contains!(read_package_config(&ctx, "usr/lib/aarch64-linux-gnu/pkgconfig/opusfile.pc"), "prefix=/layers/heroku_deb-packages/packages/usr");
                assert_contains!(read_package_config(&ctx, "usr/lib/aarch64-linux-gnu/pkgconfig/opusurl.pc"), "prefix=/layers/heroku_deb-packages/packages/usr");
            }
            _ => panic_unsupported_test_configuration(),
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
                    assert_contains!(ctx.pack_stderr, "Requesting packages");
                    assert_contains!(ctx.pack_stderr, "Adding `libxmlsec1@1.2.33-1build2`");
                    assert_contains!(ctx.pack_stderr, "`libxmlsec1@1.2.33-1build2` from http://archive.ubuntu.com/ubuntu/pool/main/x/xmlsec1/libxmlsec1_1.2.33-1build2_amd64.deb");

                    assert_not_contains!(ctx.pack_stderr, "Adding `libgwenhywfar-data@5.9.0-1`");
                    assert_not_contains!(ctx.pack_stderr, "`libgwenhywfar-data@5.9.0-1` from http://archive.ubuntu.com/ubuntu/pool/universe/libg/libgwenhywfar/libgwenhywfar-data_5.9.0-1_all.deb");
                }
                ("heroku/builder:24", "amd64") => {
                    assert_contains!(ctx.pack_stderr, "Requesting packages");
                    assert_contains!(ctx.pack_stderr, "Adding `libxmlsec1t64@1.2.39-5build2`");
                    assert_contains!(ctx.pack_stderr, "`libxmlsec1t64@1.2.39-5build2` from http://archive.ubuntu.com/ubuntu/pool/main/x/xmlsec1/libxmlsec1t64_1.2.39-5build2_amd64.deb");

                    assert_not_contains!(ctx.pack_stderr, "Adding `libgwenhywfar-data@5.10.2-2.1build4`");
                    assert_not_contains!(ctx.pack_stderr, "`libgwenhywfar-data@5.10.2-2.1build4` from http://archive.ubuntu.com/ubuntu/pool/universe/libg/libgwenhywfar/libgwenhywfar-data@5.10.2-2.1build4_all.deb");
                }
                ("heroku/builder:24", "arm64") => {
                    assert_contains!(ctx.pack_stderr, "Requesting packages");
                    assert_contains!(ctx.pack_stderr, "Adding `libxmlsec1t64@1.2.39-5build2`");
                    assert_contains!(ctx.pack_stderr, "`libxmlsec1t64@1.2.39-5build2` from http://ports.ubuntu.com/ubuntu-ports/pool/main/x/xmlsec1/libxmlsec1t64_1.2.39-5build2_arm64.deb");

                    assert_not_contains!(ctx.pack_stderr, "Adding `libgwenhywfar-data@5.10.2-2.1build4`");
                    assert_not_contains!(ctx.pack_stderr, "`libgwenhywfar-data@5.10.2-2.1build4` from http://ports.ubuntu.com/ubuntu-ports/pool/universe/libg/libgwenhywfar/libgwenhywfar-data_5.10.2-2.1build4_all.deb");
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
                        assert_contains!(ctx.pack_stderr, "Requesting packages (packages changed)");
                        assert_contains!(ctx.pack_stderr, "Adding `libgwenhywfar-data@5.9.0-1`");
                        assert_contains!(ctx.pack_stderr, "`libgwenhywfar-data@5.9.0-1` from http://archive.ubuntu.com/ubuntu/pool/universe/libg/libgwenhywfar/libgwenhywfar-data_5.9.0-1_all.deb");

                        assert_not_contains!(ctx.pack_stderr, "Adding `libxmlsec1@1.2.33-1build2`");
                        assert_not_contains!(ctx.pack_stderr, "`libxmlsec1@1.2.33-1build2` from http://archive.ubuntu.com/ubuntu/pool/main/x/xmlsec1/libxmlsec1_1.2.33-1build2_amd64.deb");
                    }
                    ("heroku/builder:24", "amd64") => {
                        assert_contains!(ctx.pack_stderr, "Requesting packages (packages changed)");
                        assert_contains!(ctx.pack_stderr, "Adding `libgwenhywfar-data@5.10.2-2.1build4`");
                        assert_contains!(ctx.pack_stderr, "`libgwenhywfar-data@5.10.2-2.1build4` from http://archive.ubuntu.com/ubuntu/pool/universe/libg/libgwenhywfar/libgwenhywfar-data_5.10.2-2.1build4_all.deb");

                        assert_not_contains!(ctx.pack_stderr, "Adding `libxmlsec1t64@1.2.39-5build2`");
                        assert_not_contains!(ctx.pack_stderr, "`libxmlsec1t64@1.2.39-5build2` from http://archive.ubuntu.com/ubuntu/pool/main/x/xmlsec1/libxmlsec1t64@1.2.39-5build2_amd64.deb");
                    }
                    ("heroku/builder:24", "arm64") => {
                        assert_contains!(ctx.pack_stderr, "Requesting packages (packages changed)");
                        assert_contains!(ctx.pack_stderr, "Adding `libgwenhywfar-data@5.10.2-2.1build4`");
                        assert_contains!(ctx.pack_stderr, "`libgwenhywfar-data@5.10.2-2.1build4` from http://ports.ubuntu.com/ubuntu-ports/pool/universe/libg/libgwenhywfar/libgwenhywfar-data_5.10.2-2.1build4_all.deb");

                        assert_not_contains!(ctx.pack_stderr, "Adding `libxmlsec1t64@1.2.39-5build2`");
                        assert_not_contains!(ctx.pack_stderr, "`libxmlsec1t64@1.2.39-5build2` from http://ports.ubuntu.com/ubuntu-ports/pool/main/x/xmlsec1/libxmlsec1t64_1.2.39-5build2_arm64.deb");
                    }
                    _ => panic_unsupported_test_configuration(),
                },
            );
        },
    );
}

#[test]
#[ignore = "integration test"]
fn ffmpeg_usage() {
    integration_test_with_config(
        "fixtures/project_file_with_empty_config",
        |config| {
            config.app_dir_preprocessor(|app_dir| {
                set_install_config(&app_dir, [requested_package_config("ffmpeg", false)]);
            });
        },
        |ctx| {
            assert_contains!(ctx.run_shell_command("ffmpeg -version").stdout, "ffmpeg version");
        },
    );
}

#[test]
#[ignore = "integration test"]
fn geo_buildpack_use_case() {
    if get_integration_test_builder().as_str() != "heroku/builder:22" {
        return;
    }
    // this test is supposed to validate that this buildpack can be an effective replacement for the
    // heroku-geo-buildpack (https://github.com/heroku/heroku-geo-buildpack) and allow a language like
    // Python to bind to libgdal-dev headers
    integration_test_with_config(
        "fixtures/project_file_with_empty_config",
        |config| {
            config
                .app_dir_preprocessor(|app_dir| {
                    set_install_config(&app_dir, [requested_package_config("libgdal-dev", false)]);
                    std::fs::write(app_dir.join("requirements.txt"), "GDAL==3.4.1").unwrap();
                })
                .buildpacks(vec![BuildpackReference::CurrentCrate, BuildpackReference::Other("heroku/python".to_string())]);
        },
        |ctx| {
            let stdout_stderr = format!("{}\n{}", ctx.pack_stdout, ctx.pack_stderr);
            assert_contains!(stdout_stderr, "Adding `libgdal-dev@3.4.1");
            assert_contains!(stdout_stderr, "Collecting GDAL==3.4.1 (from -r requirements.txt (line 1))");
            assert_contains!(stdout_stderr, "Successfully built GDAL");
            assert_contains!(stdout_stderr, "Successfully installed GDAL-3.4.1");
        },
    );
}

#[test]
#[ignore = "integration test"]
fn vips_usage() {
    integration_test_with_config(
        "fixtures/project_file_with_empty_config",
        |config| {
            config.app_dir_preprocessor(|app_dir| {
                set_install_config(&app_dir, [requested_package_config("libvips-tools", false)]);
            });
        },
        |ctx| {
            assert_contains!(ctx.run_shell_command("vips --version").stdout, "vips-");
        },
    );
}

#[test]
#[ignore = "integration test"]
fn custom_repository_for_noble_distro() {
    if get_integration_test_builder().as_str() != "heroku/builder:24" {
        return;
    }
    integration_test("fixtures/custom_repository_noble", |ctx| {
        assert_contains!(ctx.pack_stderr, "https://repo.mongodb.org/apt/ubuntu noble/mongodb-org/8.0 [multiverse]");
        assert_contains!(ctx.pack_stderr, "Downloaded release file https://repo.mongodb.org/apt/ubuntu/dists/noble/mongodb-org/8.0/InRelease");
        assert_contains_match!(ctx.pack_stderr, r"Downloaded package index https://repo.mongodb.org/apt/ubuntu/dists/noble/mongodb-org/8.0/multiverse/binary-(amd|arm)64/Packages.gz");
        assert_contains!(ctx.pack_stderr, "Adding `mongodb-org-tools");
        assert_contains!(ctx.pack_stderr, "Adding `mongodb-org-shell");
    });
}

#[test]
#[ignore = "integration test"]
fn multiple_configuration_sources() {
    if get_integration_test_builder().as_str() != "heroku/builder:24" {
        return;
    }
    integration_test_with_config(
        "fixtures/project_file_with_empty_config",
        |config| {
            config.app_dir_preprocessor(|app_dir| {
                set_install_config(&app_dir, [requested_package_config("libvips-tools", false)]);
            });

            config.buildpacks(vec![
                BuildpackReference::CurrentCrate,
                custom_buildpack()
                    .id("test/buildplan_config")
                    .detect(indoc! { r#"
                        #!/usr/bin/env bash
                        build_plan="$2"

                        cat <<EOF >"$build_plan"
                        [[requires]]
                        name = "heroku/deb-packages"

                        [requires.metadata]
                        install = ["mongodb-org-tools", "mongodb-org-shell"]

                        [[requires.metadata.sources]]
                        uri = "https://repo.mongodb.org/apt/ubuntu"
                        suites = ["noble/mongodb-org/8.0"]
                        components = ["multiverse"]
                        arch = ["amd64", "arm64"]
                        signed_by = """-----BEGIN PGP PUBLIC KEY BLOCK-----

                        mQINBGWgEhwBEADJpjwR+5n6na3tqZ6ueHsW/U8lvcvMFZ1DYNo+/JhrNjHkZ7HR
                        Wbc2IzWej1zqTtctSKZvrCkPGZxiDsKB5xta/NVtnpjSuV02Gp0F6hf0gnvark04
                        HnEFaV2w15Tyr8Z4KHRDbdja6h/24t4tR0KkRzxh5U7FwLL8BpK2drbTog9FBMy+
                        lqYDfOLHx6JDeOMC7eSNe/jJsAiuVcP/y+vQbLuMYAaMPSvJoidRIQ88oFLoUlVZ
                        NxRt3Z+7w5HMIN2laKp+ItxloPWGBdcHU4o2ZnWgsVT8Y/a+RED75DDbAQ6lS3fV
                        sSlmQLExcf75qOPy34XNv3gWP4tbfIXXt8olflF8hwHggmKZzEImnzEozPabDsN7
                        nkhHZEWhGcPRcuHbFOqcirV1sfsKK1gOsTbxS00iD3OivOFCQqujF196cal/utTd
                        WvyJvY2o35eE0WFcDdstU7UiP39usE+jk4jbQS5WbMYk9yyZCCbd74T7eYAfSEXg
                        GqrE1O6pjMmwbEjHwHDkbn/2WGvOSgWKHJVSh8V1K5ijlAd/9SCbsY0Yh5K3G16k
                        gnzHZ7OuQItfvMlPLQA7P2cPj/bGkO2ayyZU4+9rCsXlHw4Cee+u1APFSO2rj1TE
                        vX80grtqXNmj6nV21nIiXASvBKRO3kU4t8yV9i8EEREKYx/gLIl5i3PYGwARAQAB
                        tDdNb25nb0RCIDguMCBSZWxlYXNlIFNpZ25pbmcgS2V5IDxwYWNrYWdpbmdAbW9u
                        Z29kYi5jb20+iQJOBBMBCgA4FiEESwdSwbyiOMC07hTcQd4Fik59ygUFAmWgEhwC
                        GwMFCwkIBwIGFQoJCAsCBBYCAwECHgECF4AACgkQQd4Fik59ygWy4w//e+IQ5eFT
                        rlowx196DaInUTiv+aMkkN5hAtJDMicV9+ZDChEfqqQH1WJuUUKfX00AeEDocQnI
                        LgESy0+rp2FoRPG5bXaJXTv6xQkqIMQQMNMkG4Nx3AxggRVkzd2arOr9FBwcnmf0
                        7xu9EsMJndmzTsDO+ohWnNb0ILSdPVKDafpfg4ycBWDZT7ynD6TT0JpG8WWJi8F+
                        9GR4k4CpBujk49POZbjeVDOuP/o/tosmEO9jo03C/u1qNuVVXy6vvTB6WjO79QTX
                        OlSTLHAiu9N/VknG1B7lW15X1yl3jl3vZ33N68ncXUW2gAJi7Nh6H6RSm288IC4i
                        hSmSBFabffQtwOTVE0CaKge2nU4Oc3Tp2h8moEgi81vYT/CioMt6wmHTzY0grcfF
                        WLwtDMFJ0VQYRrUIOMmYBFyRp2jdRYYkA+vlL+6DNAAjCeuvwCs3PqUhgFvHNxVv
                        bumKiRMIOoNUwpLEKsEq8jBs+U+gUfa+CmBn67G9mjDRu4cXXrtItooxnbfM/m0i
                        hVnssTC1arrx273zFepLosPvgrT0TS7tnyXbzuq5mo0zD1fSj4kuSS9V/SSy9fWF
                        LAtHiNQJkjzGFxu0/9dyQyX6C523uvfdcOzpObTyjBeGKqmEEf0lF5OYLDlkk2Sm
                        iGa6i2oLaGzGaQZDpdqyQZiYpQEYw9xN+8g=
                        =J31U
                        -----END PGP PUBLIC KEY BLOCK-----
                        """
                        EOF
                    "#})
                    .call(),
            ]);
        },
        |ctx| {
            assert_contains!(ctx.pack_stderr, "https://repo.mongodb.org/apt/ubuntu noble/mongodb-org/8.0 [multiverse]");
            assert_contains!(ctx.pack_stderr, "Downloaded release file https://repo.mongodb.org/apt/ubuntu/dists/noble/mongodb-org/8.0/InRelease");
            assert_contains_match!(ctx.pack_stderr, r"Downloaded package index https://repo.mongodb.org/apt/ubuntu/dists/noble/mongodb-org/8.0/multiverse/binary-(amd|arm)64/Packages.gz");
            assert_contains!(ctx.pack_stderr, "Adding `mongodb-org-tools");
            assert_contains!(ctx.pack_stderr, "Adding `mongodb-org-shell");
            assert_contains!(ctx.pack_stderr, "Adding `libvips-tools");
        },
    );
}

#[test]
#[ignore = "integration test"]
fn only_buildplan_configuration_source() {
    if get_integration_test_builder().as_str() != "heroku/builder:24" {
        return;
    }
    integration_test_with_config(
        "fixtures/project_file_with_no_config",
        |config| {
            config.buildpacks(vec![
                BuildpackReference::CurrentCrate,
                custom_buildpack()
                    .id("test/buildplan_config")
                    .detect(indoc! { r#"
                        #!/usr/bin/env bash
                        build_plan="$2"

                        cat <<EOF >"$build_plan"
                        [[requires]]
                        name = "heroku/deb-packages"

                        [requires.metadata]
                        install = ["mongodb-org-tools", "mongodb-org-shell"]

                        [[requires.metadata.sources]]
                        uri = "https://repo.mongodb.org/apt/ubuntu"
                        suites = ["noble/mongodb-org/8.0"]
                        components = ["multiverse"]
                        arch = ["amd64", "arm64"]
                        signed_by = """-----BEGIN PGP PUBLIC KEY BLOCK-----

                        mQINBGWgEhwBEADJpjwR+5n6na3tqZ6ueHsW/U8lvcvMFZ1DYNo+/JhrNjHkZ7HR
                        Wbc2IzWej1zqTtctSKZvrCkPGZxiDsKB5xta/NVtnpjSuV02Gp0F6hf0gnvark04
                        HnEFaV2w15Tyr8Z4KHRDbdja6h/24t4tR0KkRzxh5U7FwLL8BpK2drbTog9FBMy+
                        lqYDfOLHx6JDeOMC7eSNe/jJsAiuVcP/y+vQbLuMYAaMPSvJoidRIQ88oFLoUlVZ
                        NxRt3Z+7w5HMIN2laKp+ItxloPWGBdcHU4o2ZnWgsVT8Y/a+RED75DDbAQ6lS3fV
                        sSlmQLExcf75qOPy34XNv3gWP4tbfIXXt8olflF8hwHggmKZzEImnzEozPabDsN7
                        nkhHZEWhGcPRcuHbFOqcirV1sfsKK1gOsTbxS00iD3OivOFCQqujF196cal/utTd
                        WvyJvY2o35eE0WFcDdstU7UiP39usE+jk4jbQS5WbMYk9yyZCCbd74T7eYAfSEXg
                        GqrE1O6pjMmwbEjHwHDkbn/2WGvOSgWKHJVSh8V1K5ijlAd/9SCbsY0Yh5K3G16k
                        gnzHZ7OuQItfvMlPLQA7P2cPj/bGkO2ayyZU4+9rCsXlHw4Cee+u1APFSO2rj1TE
                        vX80grtqXNmj6nV21nIiXASvBKRO3kU4t8yV9i8EEREKYx/gLIl5i3PYGwARAQAB
                        tDdNb25nb0RCIDguMCBSZWxlYXNlIFNpZ25pbmcgS2V5IDxwYWNrYWdpbmdAbW9u
                        Z29kYi5jb20+iQJOBBMBCgA4FiEESwdSwbyiOMC07hTcQd4Fik59ygUFAmWgEhwC
                        GwMFCwkIBwIGFQoJCAsCBBYCAwECHgECF4AACgkQQd4Fik59ygWy4w//e+IQ5eFT
                        rlowx196DaInUTiv+aMkkN5hAtJDMicV9+ZDChEfqqQH1WJuUUKfX00AeEDocQnI
                        LgESy0+rp2FoRPG5bXaJXTv6xQkqIMQQMNMkG4Nx3AxggRVkzd2arOr9FBwcnmf0
                        7xu9EsMJndmzTsDO+ohWnNb0ILSdPVKDafpfg4ycBWDZT7ynD6TT0JpG8WWJi8F+
                        9GR4k4CpBujk49POZbjeVDOuP/o/tosmEO9jo03C/u1qNuVVXy6vvTB6WjO79QTX
                        OlSTLHAiu9N/VknG1B7lW15X1yl3jl3vZ33N68ncXUW2gAJi7Nh6H6RSm288IC4i
                        hSmSBFabffQtwOTVE0CaKge2nU4Oc3Tp2h8moEgi81vYT/CioMt6wmHTzY0grcfF
                        WLwtDMFJ0VQYRrUIOMmYBFyRp2jdRYYkA+vlL+6DNAAjCeuvwCs3PqUhgFvHNxVv
                        bumKiRMIOoNUwpLEKsEq8jBs+U+gUfa+CmBn67G9mjDRu4cXXrtItooxnbfM/m0i
                        hVnssTC1arrx273zFepLosPvgrT0TS7tnyXbzuq5mo0zD1fSj4kuSS9V/SSy9fWF
                        LAtHiNQJkjzGFxu0/9dyQyX6C523uvfdcOzpObTyjBeGKqmEEf0lF5OYLDlkk2Sm
                        iGa6i2oLaGzGaQZDpdqyQZiYpQEYw9xN+8g=
                        =J31U
                        -----END PGP PUBLIC KEY BLOCK-----
                        """
                        EOF
                    "#})
                    .call(),
            ]);
        },
        |ctx| {
            assert_contains!(ctx.pack_stderr, "https://repo.mongodb.org/apt/ubuntu noble/mongodb-org/8.0 [multiverse]");
            assert_contains!(ctx.pack_stderr, "Downloaded release file https://repo.mongodb.org/apt/ubuntu/dists/noble/mongodb-org/8.0/InRelease");
            assert_contains_match!(ctx.pack_stderr, r"Downloaded package index https://repo.mongodb.org/apt/ubuntu/dists/noble/mongodb-org/8.0/multiverse/binary-(amd|arm)64/Packages.gz");
            assert_contains!(ctx.pack_stderr, "Adding `mongodb-org-tools");
            assert_contains!(ctx.pack_stderr, "Adding `mongodb-org-shell");
        },
    );
}

const DEFAULT_BUILDER: &str = "heroku/builder:24";

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

fn read_package_config(ctx: &TestContext, package_config_path: &str) -> String {
    ctx.run_shell_command(format!("cat /layers/heroku_deb-packages/packages/{package_config_path}")).stdout
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
            .and_then(|buildpacks| buildpacks.get_mut("deb-packages"))
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
    fs::write(&project_toml, doc.to_string()).unwrap();
}

#[bon::builder(on(String, into))]
pub fn custom_buildpack(id: &str, detect: Option<String>, build: Option<String>) -> BuildpackReference {
    let buildpack_dir = tempfile::tempdir().unwrap().keep();
    let bin_dir = buildpack_dir.join("bin");

    fs::create_dir(&bin_dir).unwrap();

    fs::write(
        buildpack_dir.join("buildpack.toml"),
        format!(
            "
api = \"0.10\"

[buildpack]
id = \"{id}\"
version = \"0.0.0\"
    "
        ),
    )
    .unwrap();

    fs::write(bin_dir.join("detect"), detect.unwrap_or("#!/usr/bin/env bash".to_string())).unwrap();

    fs::write(bin_dir.join("build"), build.unwrap_or("#!/usr/bin/env bash".to_string())).unwrap();

    BuildpackReference::Other(buildpack_dir.to_string_lossy().to_string())
}
